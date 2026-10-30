//! The harness read: what the reporting harness recorded, as session usage.
//!
//! `Scripts/usage-report.sh` reads this today with `sqlite3 -readonly` and passes
//! the numbers back as flags (`docs/port.md`, Where the old code goes: the script
//! becomes `tandem session end --usage-from-harness`). The port keeps the read in
//! the core, so it ships inside the one binary and the CLI composes rather than
//! shells out.
//!
//! **The database is opened read-only and its path is a path**, never text in a
//! statement: the session id is bound, and a file that is not there answers a
//! domain failure rather than a panic. Nothing here writes: the harness owns its
//! database, and Tandem only reads it.
//!
//! **Two reads, one shape.** The row (`session_v2` in opencode's own schema) is
//! the tokens, the cost and the model; the messages carry the skill loads, one
//! `skill` tool part per load, counted by name (TAN-241). A number the harness
//! did not report stays absent, and a skill object with no loads stays absent
//! too, never an empty map.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};

use super::{StoreError, translate};
use crate::error::{DomainError, DomainResult};
use crate::session::{SessionTokens, SkillCounts};

/// What the harness reported for one session: the usage triple and the skill
/// counts, each piece absent when the harness did not report it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HarnessUsage {
    pub tokens: SessionTokens,
    pub cost: Option<f64>,
    pub model: Option<String>,
    pub skills: Option<SkillCounts>,
}

impl HarnessUsage {
    /// The record's usage pieces, ready for `session end`: `None` where the
    /// harness reported nothing, so the record never carries an invented zero.
    pub fn tokens(&self) -> Option<SessionTokens> {
        self.tokens.reported().then(|| self.tokens.clone())
    }
}

/// Read `session_id`'s usage out of the harness database at `database`.
///
/// A missing file is [`DomainError::NoHarnessDatabase`]; a session the harness
/// never recorded is [`DomainError::UnknownHarnessSession`]. Neither is an I/O
/// surprise the caller has to recognise: both name what to do next.
pub fn read(database: &Path, session_id: &str) -> DomainResult<HarnessUsage> {
    if !database.is_file() {
        return Err(DomainError::NoHarnessDatabase {
            path: database.display().to_string(),
        });
    }
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            DomainError::from(StoreError::Io(format!(
                "could not open the harness database at {} read-only: {error}",
                database.display()
            )))
        })?;
    let mut usage = row(&connection, session_id)?;
    usage.skills = skills(&connection, session_id)?;
    Ok(usage)
}

/// The session row: the five token counts, the cost and the model.
fn row(connection: &Connection, session_id: &str) -> DomainResult<HarnessUsage> {
    let found = connection
        .query_row(
            "SELECT tokens_input, tokens_output, tokens_reasoning,
                    tokens_cache_read, tokens_cache_write, cost, model
             FROM session_v2 WHERE id = ?1 LIMIT 1",
            params![session_id],
            |row| {
                let model: Option<String> = row.get(6)?;
                Ok(HarnessUsage {
                    tokens: SessionTokens {
                        input: row.get(0)?,
                        output: row.get(1)?,
                        reasoning: row.get(2)?,
                        cache_read: row.get(3)?,
                        cache_write: row.get(4)?,
                    },
                    cost: row.get(5)?,
                    model: model.map(|raw| model_id(&raw)),
                    skills: None,
                })
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => DomainError::UnknownHarnessSession {
                id: session_id.to_string(),
            },
            other => DomainError::from(translate(other)),
        })?;
    Ok(found)
}

/// The skill loads: one count per `skill` tool part in the session's messages,
/// by the name the part asked for.
///
/// A message whose `data` is not the JSON this build reads is skipped rather than
/// failing the stamp: the usage the pass spent is the fact the record exists for,
/// and one unreadable message is not a reason to lose it. No loads at all is
/// `None`, so the record says nothing rather than "nothing was loaded".
fn skills(connection: &Connection, session_id: &str) -> DomainResult<Option<SkillCounts>> {
    let mut statement = connection
        .prepare("SELECT data FROM session_message WHERE session_id = ?1")
        .map_err(|error| DomainError::from(translate(error)))?;
    let rows = statement
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(|error| DomainError::from(translate(error)))?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for data in rows {
        let data = data.map_err(|error| DomainError::from(translate(error)))?;
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&data) else {
            continue;
        };
        let Some(content) = message
            .get("content")
            .and_then(|content| content.as_array())
        else {
            continue;
        };
        for part in content {
            if part.get("type").and_then(|kind| kind.as_str()) != Some("tool")
                || part.get("name").and_then(|name| name.as_str()) != Some("skill")
            {
                continue;
            }
            let Some(name) = part
                .pointer("/state/input/id")
                .and_then(|name| name.as_str())
                .filter(|name| !name.trim().is_empty())
            else {
                continue;
            };
            *counts.entry(name.trim().to_string()).or_insert(0) += 1;
        }
    }
    Ok((!counts.is_empty()).then_some(counts))
}

/// The model string the harness reports: the `id` inside the row's JSON model,
/// or the text itself when it is not the object opencode writes.
fn model_id(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(model) => model
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}
