//! The write side: one transaction, one actor, one chance to roll the backup.
//!
//! A [`Write`] is the only way a row enters the store, so the actor cannot be
//! forgotten at a call site: the write takes it once and every insert records
//! it. The guard also owns the rolling backup's timing: the pre-write state is
//! copied **before the first mutation of the transaction**, while the database
//! still holds the last committed state (ADR 0018), and a write that changes
//! nothing rolls no backup at all.
//!
//! The `New*` structs are the insert parameters, not the domain's types: PRT-5
//! and PRT-6 map the ticket and record aggregates onto them, and the wire never
//! sees them (`docs/port.md`: the messages are domain verbs, not rows). Each
//! struct names every column its insert writes; the author is the only column it
//! does not carry, because the write's [`Actor`] fills it. Timestamps are
//! ISO-8601 UTC text ([`super::encode`]), whole seconds.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::{Actor, Result};

/// A cycle lane: its name, its key (the ref prefix), its issued-number counter,
/// its render position and its parent project.
pub struct NewLane<'a> {
    pub name: &'a str,
    pub key: &'a str,
    pub last_number: i64,
    pub position: i64,
    pub project: &'a str,
}

/// One project: its name and its render position. Add appends, there is no
/// move verb, and a rename keeps its position.
pub struct NewProject<'a> {
    pub name: &'a str,
    pub position: i64,
}

/// A ticket. The ref is not a column: it is the lane's key and `number`, read
/// together, so a lane move cannot leave a second truth behind.
pub struct NewTicket<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub status: &'a str,
    pub archived: bool,
    pub lane: Option<&'a str>,
    pub number: Option<i64>,
    pub origin: &'a str,
    pub assignee: Option<&'a str>,
    pub link: Option<&'a str>,
    pub position: i64,
    pub created_at: &'a str,
    pub completed_at: Option<&'a str>,
}

/// One dependency edge, both ends a ticket's UUID. A table, not a column of
/// refs: the far side (`blocks`) is the same rows read backwards.
pub struct NewDependency<'a> {
    pub ticket_id: &'a str,
    pub depends_on_id: &'a str,
}

/// One comment. Its author is the write's actor: the comment's author and the
/// mutation's author are the same fact, so the row carries it once.
pub struct NewComment<'a> {
    pub ticket_id: &'a str,
    pub at: &'a str,
    pub body: &'a str,
}

/// A write in progress. Rows enter the store through its inserts only, so no
/// caller writes SQL and no caller chooses the author.
pub struct Write<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) actor: &'a Actor,
    pub(crate) backup: BackupState<'a>,
}

/// Whether this write still owes the rolling backup, and whether it has been
/// taken. Armed only when the database existed and did not migrate: the first
/// write has no earlier state, and a migrating write already rolled its
/// pre-image before the migration ran.
pub(crate) struct BackupState<'a> {
    pub(crate) database: &'a Path,
    pub(crate) target: PathBuf,
    pub(crate) armed: bool,
    pub(crate) taken: Cell<bool>,
}

impl Write<'_> {
    /// The actor every row of this write records.
    pub fn actor(&self) -> &Actor {
        self.actor
    }

    pub fn insert_lane(&self, lane: NewLane<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO lanes (name, key, last_number, position, project, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                lane.name,
                lane.key,
                lane.last_number,
                lane.position,
                lane.project,
                self.actor.as_str()
            ],
        )
    }

    pub fn insert_project(&self, project: NewProject<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO projects (name, position, author) VALUES (?1, ?2, ?3)",
            params![project.name, project.position, self.actor.as_str()],
        )
    }

    pub fn insert_ticket(&self, ticket: NewTicket<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO tickets
                (id, title, description, status, archived, lane, number, origin,
                 assignee, link, position, created_at, completed_at, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                ticket.id,
                ticket.title,
                ticket.description,
                ticket.status,
                i64::from(ticket.archived),
                ticket.lane,
                ticket.number,
                ticket.origin,
                ticket.assignee,
                ticket.link,
                ticket.position,
                ticket.created_at,
                ticket.completed_at,
                self.actor.as_str(),
            ],
        )
    }

    pub fn insert_dependency(&self, dependency: NewDependency<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO ticket_dependencies (ticket_id, depends_on_id, author)
             VALUES (?1, ?2, ?3)",
            params![
                dependency.ticket_id,
                dependency.depends_on_id,
                self.actor.as_str()
            ],
        )
    }

    /// Insert a comment whose author is the comment's own, not the write's actor.
    /// The two are different facts: the actor made the write, the author said the
    /// words, and only the CLI's `--author` names the second (the minutes rule).
    pub(crate) fn insert_comment_authored(
        &self,
        author: &str,
        comment: NewComment<'_>,
    ) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO comments (ticket_id, author, at, body) VALUES (?1, ?2, ?3, ?4)",
            params![comment.ticket_id, author, comment.at, comment.body],
        )
    }

    pub fn insert_comment(&self, comment: NewComment<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO comments (ticket_id, author, at, body) VALUES (?1, ?2, ?3, ?4)",
            params![
                comment.ticket_id,
                self.actor.as_str(),
                comment.at,
                comment.body
            ],
        )
    }

    /// The pre-write state, copied once, before this transaction modifies a
    /// page. The copy is taken through its own read-only connection: a WAL
    /// reader sees the last committed state while this write waits, and the
    /// backup API does not run against a connection holding the write lock.
    pub(crate) fn before_mutation(&self) -> Result<()> {
        if !self.backup.armed || self.backup.taken.get() {
            return Ok(());
        }
        super::backup_database(self.backup.database, &self.backup.target)?;
        self.backup.taken.set(true);
        Ok(())
    }

    pub(crate) fn execute(&self, sql: &str, values: impl rusqlite::Params) -> Result<()> {
        self.connection
            .execute(sql, values)
            .map_err(|error| super::translate(error, self.connection))?;
        Ok(())
    }
}
