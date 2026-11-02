//! The records' rows: the reads and the two questions the records domain asks
//! the rest of the store.
//!
//! The sessions and the sprints are their own tables (ADR 0031), and their SQL
//! lives here for the same reason the board's does: no module outside the
//! adapter writes a query, and no table shape leaks into the domain or the
//! wire (`docs/port.md`, Store). The rows come back as the store holds them;
//! the domain parses them (a timestamp it cannot read, a status word this build
//! does not know), because a reader that guesses is a reader that hides a
//! corrupt row.

#[cfg(test)]
use rusqlite::params;
use rusqlite::{Connection, OpenFlags};

use super::rows::{SessionRow, SprintRow};
use super::{Result, StoreError, translate};

/// The session columns every version carries, in read order.
const SESSION_COLUMNS: &str = "id, project, agent, brief, status, outcome, summary,
                    started_at, ended_at, tokens_input, tokens_output,
                    tokens_reasoning, tokens_cache_read, tokens_cache_write,
                    seconds, cost, model, tickets";

/// Every session row, in the order the rows were recorded.
///
/// **An additive column is read only from a file that has it.** `skills`
/// arrived with v3 and the harness pointer with v5; a read never writes, so an
/// older home answers those columns NULL rather than an engine error. The
/// fragments are literals; nothing here is a caller's string.
pub(crate) fn session_rows(connection: &Connection) -> Result<Vec<SessionRow>> {
    let sql = if super::schema::has_harness(connection)? {
        format!(
            "SELECT {SESSION_COLUMNS}, skills, harness_session_id, harness_transport \
             FROM sessions ORDER BY rowid"
        )
    } else if super::schema::has_skills(connection)? {
        format!("SELECT {SESSION_COLUMNS}, skills, NULL, NULL FROM sessions ORDER BY rowid")
    } else {
        format!("SELECT {SESSION_COLUMNS}, NULL, NULL, NULL FROM sessions ORDER BY rowid")
    };
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                project: row.get(1)?,
                agent: row.get(2)?,
                brief: row.get(3)?,
                status: row.get(4)?,
                outcome: row.get(5)?,
                summary: row.get(6)?,
                started_at: row.get(7)?,
                ended_at: row.get(8)?,
                tokens_input: row.get(9)?,
                tokens_output: row.get(10)?,
                tokens_reasoning: row.get(11)?,
                tokens_cache_read: row.get(12)?,
                tokens_cache_write: row.get(13)?,
                seconds: row.get(14)?,
                cost: row.get(15)?,
                model: row.get(16)?,
                tickets: row.get(17)?,
                skills: row.get(18)?,
                harness_session_id: row.get(19)?,
                harness_transport: row.get(20)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| translate(error, connection))
}

/// Every sprint row, in the order the rows were recorded.
pub(crate) fn sprint_rows(connection: &Connection) -> Result<Vec<SprintRow>> {
    let mut statement = connection
        .prepare(
            "SELECT lane, number, session, mode, batch, runs, close, status,
                    opened_at, closed_at
             FROM sprints
             ORDER BY rowid",
        )
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok(SprintRow {
                lane: row.get(0)?,
                number: row.get(1)?,
                session: row.get(2)?,
                mode: row.get(3)?,
                batch: row.get(4)?,
                runs: row.get(5)?,
                close: row.get(6)?,
                status: row.get(7)?,
                opened_at: row.get(8)?,
                closed_at: row.get(9)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| translate(error, connection))
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

/// Read one session's `skills` text back, for the records suite: the column is
/// additive, and a row written without it reads NULL.
#[cfg(test)]
pub(crate) fn session_skills(connection: &Connection, id: &str) -> Result<Option<String>> {
    connection
        .query_row(
            "SELECT skills FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get::<_, Option<String>>(0),
        )
        .map_err(|error| translate(error, connection))
}
