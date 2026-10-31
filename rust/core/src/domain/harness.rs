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
//! **Two transports, one shape.** The `database` transport reads the row
//! (`session_v2` in opencode's own schema) for the tokens, the cost and the
//! model, and the messages for the skill loads, one `skill` tool part per load,
//! counted by name (TAN-241). The `event-stream` transport reads the harness's
//! own event JSON, one object per line as the app forwards it, and sums the
//! assistant messages the same way; the skill loads ride the `message.part`
//! events. A number the harness did not report stays absent, and a skill object
//! with no loads stays absent too, never an empty map.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;

use super::error::{DomainError, DomainResult};
use super::session::{SessionTokens, SkillCounts};
use crate::store::{StoreError, translate};

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

/// Read `session_id`'s usage out of the harness event stream at `stream`.
///
/// The stream is the harness's own event JSON, one object per line. Tandem
/// reads the `message.updated` events (`info.sessionID`, `info.tokens`,
/// `info.cost`, `info.modelID`) and the `message.part.updated` events for the
/// `skill` tool loads, and it reads only the events of `session_id`: another
/// session's events, a line that is not JSON and a type this build does not use
/// are skipped, not failures. A missing file is
/// [`DomainError::NoHarnessEvents`]; a stream with no message for the session
/// answers no usage at all.
///
/// A message is updated repeatedly as it streams, so the last event per message
/// id is the one that counts and a message is never billed twice. The model is
/// the last assistant message's, and a skill load is counted once per part id.
pub fn read_events(stream: &Path, session_id: &str) -> DomainResult<HarnessUsage> {
    if !stream.is_file() {
        return Err(DomainError::NoHarnessEvents {
            path: stream.display().to_string(),
        });
    }
    let file = File::open(stream).map_err(|error| {
        DomainError::from(StoreError::Io(format!(
            "could not open the harness event stream at {}: {error}",
            stream.display()
        )))
    })?;
    let mut messages: BTreeMap<String, (SessionTokens, Option<f64>)> = BTreeMap::new();
    let mut parts: BTreeSet<String> = BTreeSet::new();
    let mut skills: BTreeMap<String, i64> = BTreeMap::new();
    let mut model: Option<String> = None;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| {
            DomainError::from(StoreError::Io(format!(
                "could not read the harness event stream at {}: {error}",
                stream.display()
            )))
        })?;
        let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("message.updated") => {
                let Some(info) = event.pointer("/properties/info") else {
                    continue;
                };
                if !of_session(info, session_id) {
                    continue;
                }
                let key = info
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("event-{index}"));
                messages.insert(
                    key,
                    (
                        event_tokens(info.get("tokens")),
                        info.get("cost").and_then(Value::as_f64),
                    ),
                );
                if let Some(name) = info
                    .get("modelID")
                    .and_then(Value::as_str)
                    .and_then(non_blank)
                {
                    model = Some(name);
                }
            }
            Some("message.part.updated") => {
                let Some(part) = event.pointer("/properties/part") else {
                    continue;
                };
                if !of_session(part, session_id) {
                    continue;
                }
                if part.get("type").and_then(Value::as_str) != Some("tool") {
                    continue;
                }
                let tool = part
                    .get("tool")
                    .or_else(|| part.get("name"))
                    .and_then(Value::as_str);
                if tool != Some("skill") {
                    continue;
                }
                let key = part
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("part-{index}"));
                if !parts.insert(key) {
                    continue;
                }
                if let Some(name) = part
                    .pointer("/state/input/id")
                    .and_then(Value::as_str)
                    .and_then(non_blank)
                {
                    *skills.entry(name).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
    let mut usage = HarnessUsage {
        model,
        skills: (!skills.is_empty()).then_some(skills),
        ..HarnessUsage::default()
    };
    let mut spent: Option<f64> = None;
    for (tokens, cost) in messages.values() {
        add(&mut usage.tokens.input, tokens.input);
        add(&mut usage.tokens.output, tokens.output);
        add(&mut usage.tokens.reasoning, tokens.reasoning);
        add(&mut usage.tokens.cache_read, tokens.cache_read);
        add(&mut usage.tokens.cache_write, tokens.cache_write);
        if let Some(cost) = cost {
            spent = Some(spent.unwrap_or(0.0) + cost);
        }
    }
    usage.cost = spent;
    Ok(usage)
}

/// Whether an event value belongs to `session_id`.
fn of_session(value: &Value, session_id: &str) -> bool {
    value.get("sessionID").and_then(Value::as_str) == Some(session_id)
}

/// The five counts one assistant message reported, absent where it reported none.
fn event_tokens(value: Option<&Value>) -> SessionTokens {
    let Some(value) = value else {
        return SessionTokens::default();
    };
    SessionTokens {
        input: value.get("input").and_then(Value::as_i64),
        output: value.get("output").and_then(Value::as_i64),
        reasoning: value.get("reasoning").and_then(Value::as_i64),
        cache_read: value.pointer("/cache/read").and_then(Value::as_i64),
        cache_write: value.pointer("/cache/write").and_then(Value::as_i64),
    }
}

/// Add one message's piece to the session's total, keeping absence absent.
fn add(total: &mut Option<i64>, piece: Option<i64>) {
    if let Some(piece) = piece {
        *total = Some(total.unwrap_or(0) + piece);
    }
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
                    model: model.as_deref().and_then(model_id),
                    skills: None,
                })
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => DomainError::UnknownHarnessSession {
                id: session_id.to_string(),
            },
            other => DomainError::from(translate(other, connection)),
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
        .map_err(|error| DomainError::from(translate(error, connection)))?;
    let rows = statement
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(|error| DomainError::from(translate(error, connection)))?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for data in rows {
        let data = data.map_err(|error| DomainError::from(translate(error, connection)))?;
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

/// The model string the harness reports: the `id` inside the row's JSON model, a
/// JSON string's own value, or the text itself when it is not JSON.
///
/// **A JSON `null` is no model at all.** The harness writes `null` for a row it
/// never attributed, and recording the word `"null"` would invent a model by
/// that name, which is exactly what `Scripts/usage-report.sh` avoids by passing
/// no `--model` flag for an empty value.
fn model_id(raw: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(model)) => model
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::to_string),
        Ok(serde_json::Value::String(text)) => non_blank(&text),
        Ok(_) => None,
        Err(_) => non_blank(raw),
    }
}

fn non_blank(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
