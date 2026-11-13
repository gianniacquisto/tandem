//! Timestamps on the wire and in the store: ISO-8601 UTC, whole seconds.
//!
//! The Swift store writes `2026-10-30T12:27:28Z` and reads it back
//! (`Timestamp.swift`), and the port keeps the same text because the schema
//! mirrors the tables as they are used. No date library: the store needs an
//! encoder for "now" and a decoder for the rows it reads, and both are the
//! civil-date arithmetic below.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::{Result, StoreError};

/// The current instant, whole seconds, as the store writes it.
pub fn now() -> String {
    encode(unix_now())
}

/// The current instant as whole seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(_) => 0,
    }
}

/// `seconds` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn encode(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3_600,
        (of_day % 3_600) / 60,
        of_day % 60
    )
}

/// The instant `text` names, or `None` when it is not `YYYY-MM-DDTHH:MM:SSZ`.
/// A row the store cannot read is an error at its reader, never a silent zero.
///
/// The read side is the domain's: the record aggregates decode a row's stamps
/// and derive the span between them (PRT-6), so a row the store cannot read
/// fails at its reader rather than at a later render.
pub fn decode(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let year: i64 = text[0..4].parse().ok()?;
    let month: u32 = text[5..7].parse().ok()?;
    let day: u32 = text[8..10].parse().ok()?;
    let hour: i64 = text[11..13].parse().ok()?;
    let minute: i64 = text[14..16].parse().ok()?;
    let second: i64 = text[17..19].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
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
        .map_err(|error| super::translate(error, &calendar))
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

/// The civil date `days` since the epoch names (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// The day count `year-month-day` names since the epoch, the inverse of
/// [`civil_from_days`].
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
