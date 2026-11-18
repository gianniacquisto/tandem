//! The store's read-only diagnostics (ADR 0014, ADR 0031), ported from
//! `Store.health()`, `backupHealth()`, `lockIsHeld()`, `commentsHealth()` and
//! `schemaVersion()`.
//!
//! Every one is lock-free and non-mutating: `doctor` reads a machine, it never
//! changes one. A home with no database answers the absent shapes, never an
//! error.

use std::fs;
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OpenFlags};

use super::{Store, is_busy, translate};
use crate::domain::Status;

/// What the store row reports: no database (a valid empty store), readable
/// counts, or a file that is present but cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Absent,
    Counts {
        open: i64,
        done: i64,
    },
    Unreadable,
    /// A database this build does not read: a v5 file, refused at the schema
    /// seam. Named rather than called corrupt, because
    /// the repair is the one-off import, not a restore.
    Legacy {
        version: i64,
    },
    /// A database written by a newer build: the repair is to upgrade this
    /// binary, not to restore or import.
    Newer {
        store: i64,
        build: i64,
    },
}

/// How many example ids or findings a check names before it says "and more".
pub const SAMPLE: usize = 3;

/// Rows that point at nothing: the schema has no foreign keys, so these are
/// counted, never refused.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct References {
    pub orphan_comments: i64,
    pub orphan_edges: i64,
    pub missing_labels: i64,
    /// Ticket ids of the first orphan comments.
    pub comment_samples: Vec<String>,
    /// `from -> to` ids of the first orphan edges.
    pub edge_samples: Vec<String>,
    /// Ticket ids of the first tickets naming a label that does not exist.
    pub label_samples: Vec<String>,
}

/// Page counts from the store's own pragmas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pages {
    pub page_count: i64,
    pub free_pages: i64,
    pub page_size: i64,
}

/// The deep read of an existing store: integrity, references and space. Ids
/// and counts only, never ticket text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInspection {
    /// `Ok` when `PRAGMA quick_check` answered `ok`, else its first findings
    /// (or the error that stopped it).
    pub integrity: std::result::Result<(), Vec<String>>,
    /// `None` when the tables could not be read (a store the store row already
    /// reports as legacy, newer or unreadable).
    pub references: Option<References>,
    pub pages: Option<Pages>,
    /// Size of `tandem.db-wal` in bytes, 0 when there is none.
    pub wal_bytes: u64,
}

/// The rolling backup beside the store (ADR 0018): absent, or its age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupHealth {
    /// No `tandem.db.bak`: the normal state before the second write.
    Absent,
    /// A backup exists; it was written `age` ago.
    Present { age: Duration },
}

impl Store {
    /// A lock-free, non-mutating health read.
    pub fn health(&self) -> Health {
        match self.read(counts) {
            Ok(Some((open, done))) => Health::Counts { open, done },
            Ok(None) => Health::Absent,
            Err(super::StoreError::Legacy { version }) => Health::Legacy { version },
            Err(super::StoreError::Newer { store, build }) => Health::Newer { store, build },
            Err(_) => Health::Unreadable,
        }
    }

    /// The deep read behind the integrity, references and space checks: one
    /// read-only connection, nothing written. `None` when there is no database.
    pub fn inspect(&self) -> Option<StoreInspection> {
        let database = self.database();
        if !database.exists() {
            return None;
        }
        let wal_bytes = fs::metadata(self.wal()).map(|m| m.len()).unwrap_or(0);
        let Ok(connection) = super::open_read_only(&database) else {
            return Some(StoreInspection {
                integrity: Err(vec!["the file could not be opened read-only".to_string()]),
                references: None,
                pages: None,
                wal_bytes,
            });
        };
        Some(StoreInspection {
            integrity: quick_check(&connection),
            references: references(&connection),
            pages: pages(&connection),
            wal_bytes,
        })
    }

    /// The rolling backup's presence and age.
    pub fn backup_health(&self) -> BackupHealth {
        let Ok(metadata) = fs::metadata(self.backup()) else {
            return BackupHealth::Absent;
        };
        let Ok(modified) = metadata.modified() else {
            return BackupHealth::Absent;
        };
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();
        BackupHealth::Present { age }
    }

    /// Whether another process currently holds the store's write side. The probe
    /// begins a write transaction on a read-write no-wait connection and rolls it
    /// straight back; a missing database is free, and the probe writes nothing.
    pub fn lock_is_held(&self) -> bool {
        let database = self.database();
        if !database.exists() {
            return false;
        }
        let Ok(connection) =
            Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)
        else {
            return false;
        };
        let _ = connection.busy_timeout(Duration::ZERO);
        match connection.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => {
                let _ = connection.execute_batch("ROLLBACK");
                false
            }
            Err(error) => is_busy(&error),
        }
    }
}

fn counts(connection: &Connection) -> super::Result<(i64, i64)> {
    let total: i64 = connection
        .query_row("SELECT COUNT(*) FROM tickets", [], |row| row.get(0))
        .map_err(|error| translate(error, connection))?;
    let done: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM tickets WHERE status = ?1",
            [Status::Done.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| translate(error, connection))?;
    Ok((total - done, done))
}

fn quick_check(connection: &Connection) -> std::result::Result<(), Vec<String>> {
    let mut statement = connection
        .prepare("PRAGMA quick_check")
        .map_err(|error| vec![error.to_string()])?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| vec![error.to_string()])?;
    let mut findings = Vec::new();
    for row in rows {
        match row {
            Ok(text) if text == "ok" => {}
            Ok(text) => findings.push(text),
            Err(error) => {
                findings.push(error.to_string());
                break;
            }
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        findings.truncate(SAMPLE);
        Err(findings)
    }
}

fn count(connection: &Connection, sql: &str) -> Option<i64> {
    connection.query_row(sql, [], |row| row.get(0)).ok()
}

fn samples(connection: &Connection, sql: &str) -> Vec<String> {
    let Ok(mut statement) = connection.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(|row| row.ok()).collect()
}

fn references(connection: &Connection) -> Option<References> {
    const COMMENTS: &str = "FROM comments WHERE ticket_id NOT IN (SELECT id FROM tickets)";
    const EDGES: &str = "FROM ticket_dependencies WHERE ticket_id NOT IN (SELECT id FROM tickets) \
         OR depends_on_id NOT IN (SELECT id FROM tickets)";
    const LABELS: &str = "FROM tickets WHERE label IS NOT NULL \
         AND label NOT IN (SELECT name FROM labels)";
    Some(References {
        orphan_comments: count(connection, &format!("SELECT COUNT(*) {COMMENTS}"))?,
        orphan_edges: count(connection, &format!("SELECT COUNT(*) {EDGES}"))?,
        missing_labels: count(connection, &format!("SELECT COUNT(*) {LABELS}"))?,
        comment_samples: samples(
            connection,
            &format!("SELECT DISTINCT ticket_id {COMMENTS} LIMIT {SAMPLE}"),
        ),
        edge_samples: samples(
            connection,
            &format!("SELECT ticket_id || ' -> ' || depends_on_id {EDGES} LIMIT {SAMPLE}"),
        ),
        label_samples: samples(connection, &format!("SELECT id {LABELS} LIMIT {SAMPLE}")),
    })
}

fn pages(connection: &Connection) -> Option<Pages> {
    Some(Pages {
        page_count: count(connection, "PRAGMA page_count")?,
        free_pages: count(connection, "PRAGMA freelist_count")?,
        page_size: count(connection, "PRAGMA page_size")?,
    })
}
