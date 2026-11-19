//! The access record (CUT-39, ADR 0052): the durable, append-only trail of
//! every change of access an admin may need to read.
//!
//! A row is written in the same transaction as the act it records, so there is
//! no change without a row and no row without a change. A row holds who acted (a
//! member name, `owner` or `operator`), the kind of credential they acted with,
//! the event, the member it concerned, a short detail (the role, or a token's
//! label) and the time, and for an export the door. It never holds a token, a
//! digest, a key, a header value, a note or a ticket body.
//!
//! **Not board data.** Rows go through [`Store::session_write`], so they move
//! neither the revision (no `/events` stream wakes for a people change) nor the
//! rolling backup, like sessions. **Append-only:** no method here, and no route
//! or tool, updates or deletes a row; two triggers abort a raw `UPDATE` or
//! `DELETE` as well. Rows are kept: at about 200 bytes an event a prune rule is
//! not worth its risk. The table is carried by the archive (it holds no secret,
//! and an import only lands in an empty home), so who exported and when
//! travels with a move.
//!
//! All SQL lives under the store module (ADR 0009); serve holds none.

use rusqlite::{Connection, params};

use super::schema::has_table;
use super::{Result, Store, translate};

/// Version 15: the access record and the two triggers that keep it append-only.
pub(crate) const VERSION_FIFTEEN: &str = "
CREATE TABLE IF NOT EXISTS access_record (
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    actor TEXT NOT NULL,
    credential TEXT NOT NULL,
    event TEXT NOT NULL,
    target TEXT,
    detail TEXT,
    door TEXT
);
CREATE TRIGGER IF NOT EXISTS access_record_no_update BEFORE UPDATE ON access_record
BEGIN SELECT RAISE(ABORT, 'the access record is append-only'); END;
CREATE TRIGGER IF NOT EXISTS access_record_no_delete BEFORE DELETE ON access_record
BEGIN SELECT RAISE(ABORT, 'the access record is append-only'); END;
";

/// The kind of credential an act was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessCredential {
    /// The owner: loopback, `--allow-unauthenticated`, `TANDEM_TOKEN`, or the
    /// process holder at the home's own files.
    Owner,
    /// A member's browser session.
    MemberSession,
    /// A member's personal token.
    MemberToken,
    /// The hosting provider's key.
    Operator,
}

impl AccessCredential {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::MemberSession => "member-session",
            Self::MemberToken => "member-token",
            Self::Operator => "operator",
        }
    }
}

/// Who made a change of access: the name the record shows and the kind of
/// credential. Built by the door that resolved the request, never from a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessActor {
    name: String,
    credential: AccessCredential,
}

impl AccessActor {
    /// The owner, and the process holder at the home's files.
    pub fn owner() -> Self {
        Self {
            name: "owner".to_string(),
            credential: AccessCredential::Owner,
        }
    }

    /// The hosting provider's key.
    pub fn operator() -> Self {
        Self {
            name: "operator".to_string(),
            credential: AccessCredential::Operator,
        }
    }

    /// A member acting through a browser session.
    pub fn member_session(name: &str) -> Self {
        Self {
            name: name.to_string(),
            credential: AccessCredential::MemberSession,
        }
    }

    /// A member acting through a personal token.
    pub fn member_token(name: &str) -> Self {
        Self {
            name: name.to_string(),
            credential: AccessCredential::MemberToken,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn credential(&self) -> AccessCredential {
        self.credential
    }

    /// Whether this actor is the member `name` themselves.
    pub(super) fn is_member(&self, name: &str) -> bool {
        matches!(
            self.credential,
            AccessCredential::MemberSession | AccessCredential::MemberToken
        ) && self.name == name
    }
}

/// One row of the record, as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRow {
    pub id: i64,
    pub at: i64,
    pub actor: String,
    pub credential: String,
    pub event: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub door: Option<String>,
}

/// Append one row on the caller's connection, inside the caller's transaction.
pub(super) fn append(
    connection: &Connection,
    by: &AccessActor,
    event: &str,
    target: Option<&str>,
    detail: Option<&str>,
    door: Option<&str>,
    now: i64,
) -> Result<()> {
    connection
        .execute(
            "INSERT INTO access_record (at, actor, credential, event, target, detail, door)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                now,
                by.name,
                by.credential.as_str(),
                event,
                target,
                detail,
                door
            ],
        )
        .map_err(|error| translate(error, connection))?;
    Ok(())
}

impl Store {
    /// Record a board-door export: one row, written before the first byte
    /// streams. The CLI door writes none (it is read-only and offline). The
    /// door is `"board"`.
    pub fn access_record_export(&self, by: &AccessActor, door: &str, now: i64) -> Result<()> {
        self.session_write(|connection| {
            append(connection, by, "export", None, None, Some(door), now)
        })
    }

    /// Every row, oldest first. A store with no table reads as an empty record.
    pub fn access_record(&self) -> Result<Vec<AccessRow>> {
        Ok(self
            .read(|connection| {
                if !has_table(connection, "access_record")? {
                    return Ok(Vec::new());
                }
                let mut statement = connection
                    .prepare(
                        "SELECT id, at, actor, credential, event, target, detail, door
                         FROM access_record ORDER BY id",
                    )
                    .map_err(|error| translate(error, connection))?;
                let rows = statement
                    .query_map([], |row| {
                        Ok(AccessRow {
                            id: row.get(0)?,
                            at: row.get(1)?,
                            actor: row.get(2)?,
                            credential: row.get(3)?,
                            event: row.get(4)?,
                            target: row.get(5)?,
                            detail: row.get(6)?,
                            door: row.get(7)?,
                        })
                    })
                    .map_err(|error| translate(error, connection))?;
                rows.map(|row| row.map_err(|error| translate(error, connection)))
                    .collect()
            })?
            .unwrap_or_default())
    }
}

impl Store {
    /// Up to `limit` rows, newest first: the newest of all, or with `before` the
    /// rows older than that id (the last id a reader's page ended on). A store
    /// with no table reads as an empty record.
    pub fn access_record_page(&self, before: Option<i64>, limit: usize) -> Result<Vec<AccessRow>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        Ok(self
            .read(|connection| {
                if !has_table(connection, "access_record")? {
                    return Ok(Vec::new());
                }
                let mut statement = connection
                    .prepare(
                        "SELECT id, at, actor, credential, event, target, detail, door
                         FROM access_record
                         WHERE ?1 IS NULL OR id < ?1
                         ORDER BY id DESC LIMIT ?2",
                    )
                    .map_err(|error| translate(error, connection))?;
                let rows = statement
                    .query_map(params![before, limit], |row| {
                        Ok(AccessRow {
                            id: row.get(0)?,
                            at: row.get(1)?,
                            actor: row.get(2)?,
                            credential: row.get(3)?,
                            event: row.get(4)?,
                            target: row.get(5)?,
                            detail: row.get(6)?,
                            door: row.get(7)?,
                        })
                    })
                    .map_err(|error| translate(error, connection))?;
                rows.map(|row| row.map_err(|error| translate(error, connection)))
                    .collect()
            })?
            .unwrap_or_default())
    }
}
