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

/// Every session row, in the order the rows were recorded.
pub(crate) fn session_rows(connection: &Connection) -> Result<Vec<SessionRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, project, agent, brief, status, outcome, summary,
                    started_at, ended_at, tokens_input, tokens_output,
                    tokens_reasoning, tokens_cache_read, tokens_cache_write,
                    seconds, cost, model, tickets, skills
             FROM sessions
             ORDER BY rowid",
        )
        .map_err(translate)?;
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
            })
        })
        .map_err(translate)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(translate)
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
        .map_err(translate)?;
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
        .map_err(translate)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(translate)
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
        .map_err(translate)
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
        .map_err(translate)
}

/// The machine's offset east of UTC, in seconds, at the instant `at` (whole
/// seconds since the epoch). The offset is the zone's at *that* instant, so a
/// reader can resolve days the way [`local_day`] does without a second query
/// per record.
pub fn local_offset_seconds(at: i64) -> Result<i64> {
    let calendar = calendar_connection()?;
    calendar
        .query_row(
            "SELECT CAST(strftime('%s', ?1, 'unixepoch', 'localtime') AS INTEGER) - ?1",
            [at],
            |row| row.get::<_, i64>(0),
        )
        .map_err(translate)
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
        .map_err(translate)
}
