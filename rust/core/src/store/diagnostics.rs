//! The store's read-only diagnostics (ADR 0014, ADR 0031), ported from
//! `Store.health()`, `backupHealth()`, `lockIsHeld()`, `commentsHealth()` and
//! `schemaVersion()`.
//!
//! Every one is lock-free and non-mutating: `doctor` reads a machine, it never
//! changes one. A home with no database answers the absent shapes, never an
//! error.

use std::fs;
use std::path::Path;
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
    /// seam (docs/port.md, Store). Named rather than called corrupt, because
    /// the repair is the one-off import, not a restore.
    Legacy {
        version: i64,
    },
}

/// The rolling backup beside the store (ADR 0018): absent, or its age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupHealth {
    /// No `tandem.db.bak`: the normal state before the second write.
    Absent,
    /// A backup exists; it was written `age` ago.
    Present { age: Duration },
}

/// The comments domain's state for `doctor`: the rows the database holds.
///
/// The port has no import marker (ADR 0031's 2026-10-30 amendment): the
/// `comments` table is the live log from the first row, and `imported` is true
/// once a row exists, so the row stops saying "none yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentsHealth {
    pub rows: i64,
    pub imported: bool,
}

impl Store {
    /// A lock-free, non-mutating health read.
    pub fn health(&self) -> Health {
        match self.read(counts) {
            Ok(Some((open, done))) => Health::Counts { open, done },
            Ok(None) => Health::Absent,
            Err(super::StoreError::Legacy { version }) => Health::Legacy { version },
            Err(_) => Health::Unreadable,
        }
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

    /// The comments rows, and whether the table is the live log yet.
    pub fn comments_health(&self) -> CommentsHealth {
        let none = CommentsHealth {
            rows: 0,
            imported: false,
        };
        if !self.database().exists() {
            return none;
        }
        match self.read(comment_count) {
            Ok(Some(rows)) => CommentsHealth {
                rows,
                imported: rows > 0,
            },
            _ => none,
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

fn comment_count(connection: &Connection) -> super::Result<i64> {
    connection
        .query_row("SELECT COUNT(*) FROM comments", [], |row| row.get(0))
        .map_err(|error| translate(error, connection))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentsState {
    Absent,
    Readable,
    Unreadable,
}

/// The comments directory's state: absent is the normal early state, so only a
/// present-but-unusable path counts.
pub fn comments_state(path: &Path) -> CommentsState {
    if !path.exists() {
        return CommentsState::Absent;
    }
    if !path.is_dir() {
        return CommentsState::Unreadable;
    }
    match fs::read_dir(path) {
        Ok(_) => CommentsState::Readable,
        Err(_) => CommentsState::Unreadable,
    }
}
