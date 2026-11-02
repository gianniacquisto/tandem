//! Where the embedded harness keeps its model credentials (TAN-265, ADR 0038).
//!
//! The harness starts unauthenticated: the human's own opencode store is
//! neither read nor copied, and the sign-in lands only in the harness home.
//! `opencode auth list --standalone` under the harness environment reads the
//! same `credential` table this module reads, so the Settings face and
//! `doctor` agree without sharing a process.
//!
//! Two layouts, one decision: `OPENCODE_DB` puts the harness store at
//! `<harness>/opencode.db`, a neighbour of `tandem.db`. Without it the binary
//! falls back to `<harness>/data/opencode/opencode.db` under `XDG_DATA_HOME`.
//! Both are the harness home, so both count; the human's home is never opened.
//! What v2.2.5 stores is the database, not `auth.json`: the credentials are
//! rows, and an absent database is the unsigned state, not an error.

use std::path::PathBuf;

use rusqlite::OpenFlags;

use crate::store::Home;

/// The integration the sign-in wires first: the OpenCode Console account.
pub const CONSOLE_INTEGRATION: &str = "opencode";

/// `<TandemHome>/harness/`: the harness home, one directory the app copies and
/// restores whole (ADR 0038).
pub fn root(home: &Home) -> PathBuf {
    home.directory().join("harness")
}

/// `OPENCODE_DB` per ADR 0038: the harness store beside `tandem.db`.
pub fn database(home: &Home) -> PathBuf {
    root(home).join("opencode.db")
}

/// The fallback without `OPENCODE_DB`: the store under `XDG_DATA_HOME`.
pub fn data_database(home: &Home) -> PathBuf {
    root(home).join("data").join("opencode").join("opencode.db")
}

/// Whether the harness home holds a Console sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessAuth {
    /// The Console credential is stored. `account` is its label (`Personal`),
    /// `integrations` every stored integration id in name order, so a later
    /// provider is a second row, not a redesign.
    Stored {
        account: String,
        integrations: Vec<String>,
    },
    /// No harness database, an empty credential table, or credentials for other
    /// providers only. The harness waits for a sign-in; nothing is wrong.
    NotStored,
    /// A database is present but unreadable. Informational, like the row: the
    /// harness migrates its own store, so this names what was seen.
    Unreadable { detail: String },
}

/// Read the harness sign-in state. Opens nothing but the harness home.
pub fn read(home: &Home) -> HarnessAuth {
    let candidate = [database(home), data_database(home)]
        .into_iter()
        .find(|path| path.is_file());
    let Some(path) = candidate else {
        return HarnessAuth::NotStored;
    };
    match credentials(&path) {
        Ok(rows) => {
            let integrations = rows
                .iter()
                .map(|(integration, _)| integration.clone())
                .collect::<Vec<_>>();
            match rows
                .iter()
                .find(|(integration, _)| integration == CONSOLE_INTEGRATION)
            {
                Some((_, label)) => HarnessAuth::Stored {
                    account: label.clone(),
                    integrations,
                },
                None => HarnessAuth::NotStored,
            }
        }
        Err(error) => HarnessAuth::Unreadable { detail: error },
    }
}

/// Every stored credential, in integration name order: `(integration, label)`.
fn credentials(path: &std::path::Path) -> Result<Vec<(String, String)>, String> {
    let connection = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("the harness store is not readable: {error}"))?;
    let mut statement = connection
        .prepare("SELECT integration_id, label FROM credential ORDER BY integration_id")
        .map_err(|error| format!("the harness credentials are not readable: {error}"))?;
    statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("the harness credentials are not readable: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("the harness credentials are not readable: {error}"))
}
