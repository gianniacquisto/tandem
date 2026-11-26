//! A fixture hook for suites outside this crate. Nothing in a shipped path
//! calls it.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use super::schema::{MIGRATIONS, migrate_with};
use super::{Actor, Result, StoreError};

/// Build `<directory>/tandem.db` at schema `version` (1 to the current one):
/// the first `version` migrations applied to a fresh file, so a suite can
/// stand up the home an older build left. The directory is created.
pub fn home_at_version(directory: &Path, version: usize) -> Result<()> {
    let io = |error: &dyn std::fmt::Display| StoreError::Io(error.to_string());
    if version == 0 || version > MIGRATIONS.len() {
        return Err(StoreError::Io(format!(
            "no schema v{version}: this build ships v1 to v{}",
            MIGRATIONS.len()
        )));
    }
    fs::create_dir_all(directory).map_err(|error| io(&error))?;
    let connection = Connection::open(directory.join("tandem.db")).map_err(|error| io(&error))?;
    migrate_with(&connection, &MIGRATIONS[..version], &Actor::Human)
}

/// Run `sql` against `<directory>/tandem.db`, so a suite can seed rows into an
/// older home or mark it as a newer build's, with no second SQLite client.
pub fn execute(directory: &Path, sql: &str) -> Result<()> {
    Connection::open(directory.join("tandem.db"))
        .and_then(|connection| connection.execute_batch(sql))
        .map_err(|error| StoreError::Io(error.to_string()))
}
