//! The records' rows: the reads and the two questions the records domain asks
//! the rest of the store.
//!
//! The sprints are their own table (ADR 0031), and their SQL lives here for the
//! same reason the board's does: no module outside the adapter writes a query,
//! and no table shape leaks into the domain or the wire (`docs/port.md`,
//! Store). The rows come back as the store holds them; the domain parses them
//! (a timestamp it cannot read, a status word this build does not know),
//! because a reader that guesses is a reader that hides a corrupt row.

use rusqlite::{Connection, OpenFlags};

use super::rows::SprintRow;
use super::{Result, Store, StoreError, translate};

/// Every sprint row, in the order the rows were recorded.
///
/// **A pre-v10 home reads `sprints` and supplies the default workflow** (ADR
/// 0031: a read never migrates). The table is the same record under its old
/// name, and every row it holds predates the `workflow` column, so the default
/// is the only truthful answer.
pub(crate) fn sprint_rows(connection: &Connection) -> Result<Vec<SprintRow>> {
    let current = super::schema::has_workflow_runs(connection)?;
    let sql = if current {
        "SELECT lane, number, workflow, session, mode, batch, runs, close, status,
                opened_at, closed_at
         FROM workflow_runs
         ORDER BY rowid"
    } else {
        "SELECT lane, number, ?1, session, mode, batch, runs, close, status,
                opened_at, closed_at
         FROM sprints
         ORDER BY rowid"
    };
    let default: [&dyn rusqlite::ToSql; 1] = [&super::schema::DEFAULT_WORKFLOW_ID];
    let values: &[&dyn rusqlite::ToSql] = if current { &[] } else { &default };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map(values, |row| {
            Ok(SprintRow {
                lane: row.get(0)?,
                number: row.get(1)?,
                workflow: row.get(2)?,
                session: row.get(3)?,
                mode: row.get(4)?,
                batch: row.get(5)?,
                runs: row.get(6)?,
                close: row.get(7)?,
                status: row.get(8)?,
                opened_at: row.get(9)?,
                closed_at: row.get(10)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| translate(error, connection))
}

/// The sprints and the revision they are a snapshot of, read in one
/// transaction: a listing's rows and its number cannot be split by a
/// concurrent writer. `None` when there is no database yet.
impl Store {
    /// The sprints and the revision they are a snapshot of, read in one
    /// transaction: a listing's rows and its number cannot be split by a
    /// concurrent writer. `None` when there is no database yet.
    pub(crate) fn sprint_snapshot(&self) -> Result<Option<(Vec<SprintRow>, i64)>> {
        self.read_snapshot(sprint_rows)
    }
}

/// Whether the board carries a ticket id, archive included. The sprint's batch
/// takes ids the store holds; a ticket deleted *after* the write is not this
/// question.
pub(crate) fn ticket_carried(connection: &Connection, id: &str) -> Result<bool> {
    connection
        .query_row("SELECT COUNT(*) FROM tickets WHERE id = ?1", [id], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count > 0)
        .map_err(|error| translate(error, connection))
}

/// One setting's value, as the row carries it: the JSON text the Swift store
/// wrote. An absent key is `None`, never an empty string, so a caller can tell
/// "never edited" from "edited to nothing".
pub(crate) fn setting(connection: &Connection, key: &str) -> Result<Option<String>> {
    connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(translate(other, connection)),
        })
}

/// The local calendar day `started_at` names, as a zero-padded `YYYY-MM-DD`
/// key.
///
/// **The zone is the machine's, resolved per instant by the engine's own
/// `localtime`**, so a report spanning a daylight-saving change puts each
/// record on the date its reader lived through, exactly as `Calendar` did in
/// the Swift derivation (TAN-208). The engine is opened in memory: this is a
/// calendar question, not a store read, and it touches no file.
pub fn local_day(started_at: &str) -> Result<String> {
    let calendar = calendar_connection()?;
    calendar
        .query_row("SELECT date(?1, 'localtime')", [started_at], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| translate(error, &calendar))
}

/// The in-memory engine the calendar questions run on. Bundled SQLite, no file,
/// no sidecar: `localtime` is the C library's zone, the same one the Swift
/// `Calendar` read.
fn calendar_connection() -> Result<Connection> {
    Connection::open_with_flags(
        ":memory:",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| StoreError::Io(format!("could not open the calendar: {error}")))
}
