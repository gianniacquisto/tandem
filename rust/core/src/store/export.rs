//! The store's half of the export and the import (ADR 0050): a scrubbed,
//! single-file copy of the live store to archive, and the checks an unpacked
//! store passes before it is trusted.
//!
//! All SQL for the archive lives here (ADR 0009); [`crate::archive`] owns the
//! format and the walk and never opens a connection.
//!
//! **Every table is classified, once, in one list.** A table is either
//! *carried* (board data, copied as it is) or *emptied* (a secret: a sign-in
//! session or a token digest, never archived). The export fails closed on a
//! table that is in neither, so a later migration that adds a secret table
//! cannot leak it by being forgotten: the classification test below breaks the
//! build until the new table is placed.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::{
    CURRENT_VERSION, Result, Store, StoreError, copy_into, open_read_only, schema, sibling,
};

/// Tables an archive carries. The first group is the current schema; the second
/// is what an older store still holds before its first write migrates it (a
/// table a later version dropped or renamed), because an export of a store
/// behind this build must classify every table it finds too.
pub(crate) const CARRIED_TABLES: &[&str] = &[
    // The current schema.
    "tickets",
    "ticket_dependencies",
    "comments",
    "labels",
    "projects",
    "store_meta",
    "schema_migrations",
    // Older stores, before their first migrating write.
    "lanes",
    // The legacy agent-session table of older stores, carried as data. It is
    // not CUT-34's `browser_sessions` (sign-in sessions), which never travel.
    "sessions",
    "sprints",
    "workflow_runs",
    "workflows",
    "workflow_stages",
    "settings",
    // The engine's own, present once an AUTOINCREMENT table has been used.
    "sqlite_sequence",
];

/// Tables whose rows are secrets and never travel: the scrub deletes their rows
/// from the export's copy only, and an import refuses an archive that carries
/// any. `browser_sessions` (CUT-34, schema v13) is the sign-in session table;
/// CUT-36 adds its token table here in its own change.
pub(crate) const EMPTIED_TABLES: &[&str] = &["browser_sessions"];

/// The file the export archives: a committed generation of the store, scrubbed,
/// vacuumed and left in `journal_mode=DELETE`, so no emptied row survives in a
/// free page and no WAL travels with it. Private to the owner, and removed when
/// this guard drops, on every path.
#[derive(Debug)]
pub struct ExportCopy {
    path: PathBuf,
    schema_version: i64,
}

impl ExportCopy {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version the copy was taken at: the store's own seam, not a
    /// claim made elsewhere.
    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }
}

impl Drop for ExportCopy {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Store {
    /// Take the export's copy of the store: one committed generation through the
    /// engine's backup API (the rolling backup's own copy path, [`copy_into`]),
    /// into a temp beside the database, then scrubbed.
    ///
    /// A read in every respect that matters: the source opens read-only, takes
    /// no write gate, writes no row, moves no revision and touches neither the
    /// live store nor `tandem.db.bak`, so it runs beside a live `tandem serve`.
    pub fn export_copy(&self) -> Result<ExportCopy> {
        let database = self.database();
        if !database.exists() {
            return Err(StoreError::Io(format!(
                "there is no store at {} to export",
                database.display()
            )));
        }
        {
            let connection = open_read_only(&database)?;
            schema::refuse_legacy(&connection)?;
        }
        let temp = sibling(&database, ".tandem-export-");
        let outcome = (|| {
            create_private(&temp)?;
            copy_into(&database, &temp)?;
            scrub(&temp, CARRIED_TABLES, EMPTIED_TABLES)?;
            let connection = open_read_only(&temp)?;
            Ok(schema::version(&connection)?.unwrap_or(0))
        })();
        match outcome {
            Ok(schema_version) => Ok(ExportCopy {
                path: temp,
                schema_version,
            }),
            Err(error) => {
                let _ = fs::remove_file(&temp);
                Err(error)
            }
        }
    }
}

/// Create `path` empty and private to the owner (0600 on unix) before any byte
/// of the store lands in it, so a copy is never readable by others, even
/// briefly.
fn create_private(path: &Path) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .map(|_| ())
        .map_err(|error| StoreError::Io(format!("could not create {}: {error}", path.display())))
}

/// Empty the secret tables of the copy at `path`, then vacuum it and leave it a
/// single file in `journal_mode=DELETE`.
///
/// Takes its two lists so a test can name a table and search the file's bytes.
/// Every table of the file must be in one of them: an unclassified table fails
/// the scrub, so the export fails closed rather than carrying what nobody
/// decided about.
pub(crate) fn scrub(path: &Path, carried: &[&str], emptied: &[&str]) -> Result<()> {
    let connection = Connection::open(path).map_err(|error| {
        StoreError::Io(format!(
            "could not open the export copy {}: {error}",
            path.display()
        ))
    })?;
    let io = |error: rusqlite::Error| StoreError::Io(error.to_string());
    for table in table_names(&connection)? {
        if !carried.contains(&table.as_str()) && !emptied.contains(&table.as_str()) {
            return Err(StoreError::Io(format!(
                "the store has a table `{table}` that the export classifies as neither carried \
                 nor emptied, so it is not exported; classify it in store/export.rs"
            )));
        }
    }
    for table in emptied {
        if schema::has_table(&connection, table)? {
            connection
                .execute_batch(&format!("DELETE FROM \"{table}\""))
                .map_err(io)?;
        }
    }
    connection.execute_batch("VACUUM").map_err(io)?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .map_err(io)?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::Io(format!(
            "the export copy is in journal mode `{mode}`, not a single file"
        )));
    }
    drop(connection);
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        if Path::new(&name).exists() {
            return Err(StoreError::Io(format!(
                "the export copy has a {suffix} file beside it, so it is not one generation"
            )));
        }
    }
    Ok(())
}

/// Every table of the file, the engine's internal `sqlite_` tables aside.
fn table_names(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' \
             ESCAPE '\\' ORDER BY name",
        )
        .map_err(|error| StoreError::Io(error.to_string()))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| StoreError::Io(error.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| StoreError::Io(error.to_string()))?;
    Ok(names)
}

/// What an unpacked store is, once it has passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub schema_version: i64,
    pub tickets: u64,
}

/// Why an unpacked store is not trusted. The archive module words each one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The file is not a Tandem store, or is a store this build cannot read
    /// the shape of (the retired v5 store, or no seam at all).
    Foreign(String),
    /// A newer build's store: its schema is past what this build reads.
    Newer { version: i64 },
    /// The engine's integrity check, or an open, failed.
    Damaged(String),
    /// A trigger, a view or a table this build's schema does not define, or a
    /// table that must travel empty and does not.
    Unexpected(String),
}

/// Open the unpacked `tandem.db` read-only, before it is trusted, and check it
/// against the file itself, never the manifest's claim: the integrity check, the
/// schema seam, no trigger or view the schema does not define, no table outside
/// the classification, and every emptied table empty.
pub fn inspect_unpacked(path: &Path) -> std::result::Result<Inspection, Refusal> {
    inspect_with(path, CARRIED_TABLES, EMPTIED_TABLES)
}

/// [`inspect_unpacked`] over explicit lists, so a test can fill a table the
/// classification names without waiting for a secret table to exist.
pub(crate) fn inspect_with(
    path: &Path,
    carried: &[&str],
    emptied: &[&str],
) -> std::result::Result<Inspection, Refusal> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| Refusal::Damaged(format!("the store does not open: {error}")))?;
    let damaged = |error: rusqlite::Error| Refusal::Damaged(error.to_string());
    // A file that is not a database fails on the first read, not on the open.
    let verdict: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(damaged)?;
    if verdict != "ok" {
        return Err(Refusal::Damaged(format!(
            "the store failed the engine's integrity check ({verdict})"
        )));
    }
    match schema::refuse_legacy(&connection) {
        Ok(()) => {}
        Err(StoreError::Legacy { version }) if version > CURRENT_VERSION => {
            return Err(Refusal::Newer { version });
        }
        Err(StoreError::Legacy { version }) => {
            return Err(Refusal::Foreign(format!(
                "its seam is the retired v5 store's (v{version})"
            )));
        }
        Err(error) => return Err(Refusal::Damaged(error.to_string())),
    }
    let schema_version = match schema::version(&connection) {
        Ok(Some(version)) if version >= 1 => version,
        Ok(_) => return Err(Refusal::Foreign("it carries no schema seam".into())),
        Err(error) => return Err(Refusal::Damaged(error.to_string())),
    };
    let allowed = defined_objects().map_err(|error| Refusal::Damaged(error.to_string()))?;
    let mut statement = connection
        .prepare("SELECT type, name FROM sqlite_master WHERE type IN ('trigger', 'view')")
        .map_err(damaged)?;
    let found = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(damaged)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(damaged)?;
    for (kind, name) in found {
        if !allowed.contains(&name) {
            return Err(Refusal::Unexpected(format!(
                "it holds a {kind} `{name}` that this build's schema does not define"
            )));
        }
    }
    drop(statement);
    for table in table_names(&connection).map_err(|error| Refusal::Damaged(error.to_string()))? {
        if !carried.contains(&table.as_str()) && !emptied.contains(&table.as_str()) {
            return Err(Refusal::Unexpected(format!(
                "it holds a table `{table}` that this build does not know"
            )));
        }
    }
    for table in emptied {
        let has = schema::has_table(&connection, table)
            .map_err(|error| Refusal::Damaged(error.to_string()))?;
        if !has {
            continue;
        }
        let rows: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .map_err(damaged)?;
        if rows > 0 {
            return Err(Refusal::Unexpected(format!(
                "its `{table}` table holds rows, and an archive never carries them"
            )));
        }
    }
    let tickets = if schema::has_table(&connection, "tickets")
        .map_err(|error| Refusal::Damaged(error.to_string()))?
    {
        connection
            .query_row("SELECT COUNT(*) FROM tickets", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(damaged)? as u64
    } else {
        0
    };
    Ok(Inspection {
        schema_version,
        tickets,
    })
}

/// The names of every trigger and view this build's own schema defines, read off
/// a freshly migrated in-memory database, so the list can never drift from the
/// migrations.
fn defined_objects() -> Result<Vec<String>> {
    let connection =
        Connection::open_in_memory().map_err(|error| StoreError::Io(error.to_string()))?;
    schema::migrate(&connection, &super::Actor::Human)?;
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('trigger', 'view')")
        .map_err(|error| StoreError::Io(error.to_string()))?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| StoreError::Io(error.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| StoreError::Io(error.to_string()))
}

#[cfg(test)]
pub(crate) fn current_tables() -> Vec<String> {
    let connection = Connection::open_in_memory().expect("an in-memory database opens");
    schema::migrate(&connection, &super::Actor::Human).expect("the schema applies");
    table_names(&connection).expect("the tables list")
}
