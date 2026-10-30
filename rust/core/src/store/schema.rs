//! The store's schema and its migration seam.
//!
//! **Version 1 is a fresh schema, written once.** It mirrors today's v5 tables
//! as they are *used*, not as they are named (docs/port.md, Store): the ticket
//! domain keeps its lane, its per-lane number and its dependency edge; the
//! records domains keep their rows; every mutation row carries its `author`, a
//! column that cannot be retrofitted onto a store that assumed one user.
//!
//! `schema_migrations` is the seam, ported from `StoreSchema.swift`: one row
//! per applied migration, the version read **inside** the transaction that
//! records it, so two connections opening a fresh home serialise and apply each
//! migration once. A migration that fails rolls its own transaction back and
//! fails the open; nothing half-applies.
//!
//! **A v5 file is refused, never read.** The port has no dual writer and no v5
//! reader (docs/port.md, Migration): a file whose seam is newer than this build
//! answers one error naming `Scripts/import-legacy.py`, the one-off import
//! (PRT-18), and exits non-zero.
//!
//! All SQL in the port lives under this module.

use rusqlite::{Connection, params};

use super::{Actor, Result, StoreError, timestamp, translate};

/// The version this build writes and reads. Always the last version applied, so
/// the constant is the migration count.
pub const CURRENT_VERSION: i64 = 3;

/// One migration: its version, a name for the error text, and its DDL.
pub(crate) struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub apply: fn(&Connection) -> rusqlite::Result<()>,
}

/// Every migration, in order. Append only: a shipped version's DDL never
/// changes, so a later shape lands as a new migration rather than an edit.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "the store",
        apply: version_one,
    },
    Migration {
        version: 2,
        name: "the board and comment indexes",
        apply: version_two,
    },
    Migration {
        version: 3,
        name: "the session skill counts",
        apply: version_three,
    },
];

/// Version 1: the tickets, the edge, the comments, the records, the lanes and
/// the settings. `tickets.position` is the stored order (ADR 0006) and
/// `tickets.number` the per-lane number; the ref (`PRT-4`) is derived from the
/// lane's key and that number on read, never a second stored truth.
///
/// A comment is a row: its sidecar (`comments/<id>.jsonl`) has no table,
/// because the only reader of the old file is the one-off import (PRT-18) and
/// a table beside the rows would be a second truth. `comments` keeps its
/// `author` because a comment's author is the comment's, and
/// `schema_migrations` keeps the actor that moved the schema.
const VERSION_ONE: &str = "
CREATE TABLE IF NOT EXISTS tickets (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    description  TEXT,
    status       TEXT NOT NULL,
    archived     INTEGER NOT NULL,
    lane         TEXT,
    number       INTEGER,
    origin       TEXT NOT NULL,
    assignee     TEXT,
    link         TEXT,
    position     INTEGER NOT NULL,
    created_at   TEXT NOT NULL,
    completed_at TEXT,
    author       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS ticket_dependencies (
    ticket_id     TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    author        TEXT NOT NULL,
    PRIMARY KEY (ticket_id, depends_on_id)
);
CREATE TABLE IF NOT EXISTS comments (
    ticket_id TEXT NOT NULL,
    author    TEXT NOT NULL,
    at        TEXT NOT NULL,
    body      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    id                 TEXT PRIMARY KEY,
    project            TEXT NOT NULL,
    agent              TEXT,
    brief              TEXT NOT NULL,
    status             TEXT NOT NULL,
    outcome            TEXT,
    summary            TEXT,
    started_at         TEXT NOT NULL,
    ended_at           TEXT,
    tokens_input       INTEGER,
    tokens_output      INTEGER,
    tokens_reasoning   INTEGER,
    tokens_cache_read  INTEGER,
    tokens_cache_write INTEGER,
    seconds            REAL,
    cost               REAL,
    model              TEXT,
    tickets            TEXT,
    author             TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sprints (
    lane      TEXT NOT NULL,
    number    INTEGER NOT NULL,
    session   TEXT,
    mode      TEXT NOT NULL,
    batch     TEXT,
    runs      TEXT,
    close     TEXT,
    status    TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    author    TEXT NOT NULL,
    PRIMARY KEY (lane, number)
);
CREATE TABLE IF NOT EXISTS lanes (
    name        TEXT PRIMARY KEY,
    key         TEXT NOT NULL,
    last_number INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL,
    author      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL,
    author TEXT NOT NULL
);
";

/// Version 2 proves the seam: the two indexes the ported tests need, the
/// board's `(lane, position)` walk and the comment read's `(ticket_id)`.
const VERSION_TWO: &str = "
CREATE INDEX IF NOT EXISTS tickets_lane_position ON tickets (lane, position);
CREATE INDEX IF NOT EXISTS comments_ticket ON comments (ticket_id);
";

/// Version 3: the skill counts a session's pass loaded, stored as one JSON
/// object in a nullable column (TAN-241). **Additive**: an INSERT that names its
/// columns without `skills` leaves NULL, and the counts are absent on every
/// record written before the column existed. Absent is absent, never an empty
/// object.
///
/// Additive is not the same as backward compatible: a build that only knows v2
/// refuses the home this migration marks v3 (`refuse_legacy`), which is the
/// revert path a merge of this change has.
const VERSION_THREE: &str = "ALTER TABLE sessions ADD COLUMN skills TEXT;";

fn version_one(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_ONE)
}

fn version_two(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_TWO)
}

fn version_three(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_THREE)
}

/// Apply every migration newer than the recorded version, in order, one
/// transaction each. The version is read inside the transaction that records
/// it, so two writers opening a fresh home apply each migration once: the
/// second sees the first's row and applies nothing.
pub(crate) fn migrate(connection: &Connection, actor: &Actor) -> Result<()> {
    if CURRENT_VERSION != MIGRATIONS.len() as i64 {
        return Err(StoreError::Io(format!(
            "this build is schema v{CURRENT_VERSION} but ships {} migrations: the constant and \
             the list must agree",
            MIGRATIONS.len()
        )));
    }
    migrate_with(connection, MIGRATIONS, actor)
}

/// [`migrate`] over an explicit set, so the seam's own suite can prove the
/// order, the rollback and the upgrade without a second copy of the DDL.
pub(crate) fn migrate_with(
    connection: &Connection,
    migrations: &[Migration],
    actor: &Actor,
) -> Result<()> {
    require_contiguous(migrations)?;
    // A cheap pre-check so an up-to-date database pays no write transaction per
    // write. It only decides whether there is work: the version that counts is
    // read inside each migration's transaction below.
    if !needs_migration(connection)? {
        return Ok(());
    }
    for migration in migrations {
        let outcome = super::transaction(connection, || {
            create_seam(connection)?;
            let applied = version(connection)?.unwrap_or(0);
            if migration.version <= applied {
                return Ok(());
            }
            (migration.apply)(connection).map_err(translate)?;
            connection
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at, author)
                     VALUES (?1, ?2, ?3)",
                    params![migration.version, timestamp::now(), actor.as_str()],
                )
                .map_err(translate)?;
            Ok(())
        });
        if let Err(error) = outcome {
            // Name the migration that failed; its transaction already rolled
            // back, so the store is never left half-migrated.
            return Err(match error {
                StoreError::Io(message) => StoreError::Io(format!(
                    "migration {} ({}) failed: {message}",
                    migration.version, migration.name
                )),
                other => other,
            });
        }
    }
    Ok(())
}

/// The highest applied migration, or `None` when the database has no seam yet.
pub(crate) fn version(connection: &Connection) -> Result<Option<i64>> {
    if !has_table(connection, "schema_migrations")? {
        return Ok(None);
    }
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .map_err(translate)
}

/// Whether a file is the Swift store's or newer than this build reads.
///
/// **The seam's actor is what tells the two stores apart.** The Swift store's
/// `schema_migrations` carries `version` and `appliedAt`; every version this
/// port writes carries its `author` (PRT-4), so a seam without that column is
/// the Swift store's, whatever its version says. The number alone cannot decide
/// it any more: the port's versions have moved past the Swift store's v5, so a
/// v5 file would otherwise read as merely behind this build and be migrated.
///
/// A Rust file whose seam is past [`CURRENT_VERSION`] is refused on the version,
/// as before: that is a newer build's store, and this one cannot read it.
pub fn refuse_legacy(connection: &Connection) -> Result<()> {
    if has_table(connection, "schema_migrations")?
        && !has_column(connection, "schema_migrations", "author")?
    {
        return Err(StoreError::Legacy {
            version: version(connection)?.unwrap_or(0),
        });
    }
    match version(connection)? {
        Some(applied) if applied > CURRENT_VERSION => Err(StoreError::Legacy { version: applied }),
        _ => Ok(()),
    }
}

/// Whether a write has to migrate before it runs.
pub(crate) fn behind(connection: &Connection) -> Result<bool> {
    match version(connection)? {
        Some(applied) => Ok(applied < CURRENT_VERSION),
        None => Ok(true),
    }
}

/// Whether a store is behind the version this build writes.
fn needs_migration(connection: &Connection) -> Result<bool> {
    if !has_table(connection, "schema_migrations")? {
        return Ok(CURRENT_VERSION > 0);
    }
    Ok(version(connection)?.unwrap_or(0) < CURRENT_VERSION)
}

/// A migration list is exactly `1..=len`, in order: no hole, no renumbering.
///
/// **The seam walks a list, so a hole is a version nothing ever applied**, and
/// every later build would read a store whose shape skipped a step. The rule is
/// checked here rather than in a suite alone, because a suite only guards the
/// list this build ships; this guards every list, a test's own included.
fn require_contiguous(migrations: &[Migration]) -> Result<()> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = index as i64 + 1;
        if migration.version != expected {
            return Err(StoreError::Io(format!(
                "the migrations are not contiguous (version {} sits where {expected} belongs): \
                 a migration continues the count, it never leaves a hole",
                migration.version
            )));
        }
    }
    Ok(())
}

/// The seam's own table, created outside the versioned migrations because it is
/// the seam, not schema a version owns. The author is the actor of the write
/// that applied the migration: a migration is a write too.
fn create_seam(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL,
                author     TEXT NOT NULL
            )",
        )
        .map_err(translate)
}

pub(crate) fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(translate)
}

fn has_table(connection: &Connection, name: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(translate)
}
