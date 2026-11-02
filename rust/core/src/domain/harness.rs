//! The harness read: what the reporting harness recorded, as session usage.
//!
//! `session end --usage-from-harness` reads this in the core, so it ships inside
//! the one binary and the CLI composes rather than shells out (`docs/port.md`,
//! Where the old code goes: the wrapper script became this verb at the cutover).
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
/// that name, which is exactly what the harness read avoids by passing
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

/// One page of a harness session's transcript: the messages oldest first, and
/// the cursor the next page continues from, absent when this page is the last.
///
/// The cursor is the page's last position (`seq` in the database, the message's
/// ordinal in an event stream); a page shorter than the asked limit names none,
/// so the renderer stops paging without a second empty read.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptPage {
    pub entries: Vec<TranscriptEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<i64>,
}

/// One harness message: whose turn it was, and the blocks that turn carried.
///
/// `seq` is the database's own `seq` for the `database` transport, and the
/// message's ordinal (first-seen order) for the `event-stream` transport. The
/// role is the harness's own word (`user`, `assistant`); a word this build does
/// not know rides through unchanged, because the transcript shows what the
/// harness recorded, unredacted.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub seq: i64,
    pub id: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
    pub blocks: Vec<TranscriptBlock>,
}

/// One block of a message: what the human wrote, what the assistant thought or
/// said, or a tool call with the inputs it ran and the output it produced.
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TranscriptBlock {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Tool {
        id: String,
        name: String,
        status: String,
        input: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
}

/// A tool block's pieces, borrowed for the read: the call's name, status,
/// inputs and output, unredacted.
pub struct ToolPart<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub status: &'a str,
    pub input: &'a Value,
    pub output: Option<&'a str>,
}

impl TranscriptBlock {
    /// The block's text, when it is one the human or the assistant wrote.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    /// The block's thinking, when it is one the assistant thought.
    pub fn reasoning(&self) -> Option<&str> {
        match self {
            Self::Reasoning { text } => Some(text),
            _ => None,
        }
    }

    /// The block's call, when it is one the assistant ran.
    pub fn tool(&self) -> Option<ToolPart<'_>> {
        match self {
            Self::Tool {
                id,
                name,
                status,
                input,
                output,
            } => Some(ToolPart {
                id,
                name,
                status,
                input,
                output: output.as_deref(),
            }),
            _ => None,
        }
    }
}

/// Read `session_id`'s transcript out of the harness database at `database`,
/// oldest first, paged by `seq`.
///
/// `after` is the last `seq` the caller holds; absent reads from the start.
/// `limit` caps the page, at least one and at most five hundred. A missing file
/// is [`DomainError::NoHarnessDatabase`]; a session the harness never recorded
/// is [`DomainError::UnknownHarnessSession`]. A message whose `data` is not the
/// JSON this build reads is skipped, the way the usage read skips it: one
/// unreadable row is not a reason to lose the session's transcript.
pub fn transcript(
    database: &Path,
    session_id: &str,
    after: Option<i64>,
    limit: usize,
) -> DomainResult<TranscriptPage> {
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
    if !holds_session(&connection, session_id)? {
        return Err(DomainError::UnknownHarnessSession {
            id: session_id.to_string(),
        });
    }
    let limit = limit.clamp(1, 500) as i64;
    let mut statement = connection
        .prepare(
            "SELECT id, type, seq, time_created, time_updated, data
             FROM session_message
             WHERE session_id = ?1 AND seq > ?2
             ORDER BY seq ASC
             LIMIT ?3",
        )
        .map_err(|error| DomainError::from(translate(error, &connection)))?;
    let rows = statement
        .query_map(
            params![session_id, after.unwrap_or(i64::MIN), limit],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|error| DomainError::from(translate(error, &connection)))?;
    let mut entries = Vec::new();
    for row in rows {
        let (id, role, seq, created, updated, data) =
            row.map_err(|error| DomainError::from(translate(error, &connection)))?;
        let Ok(message) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        entries.push(message_entry(&id, &role, seq, created, updated, &message));
    }
    let next_cursor = (entries.len() as i64 == limit)
        .then(|| entries.last().map(|entry| entry.seq))
        .flatten();
    Ok(TranscriptPage {
        entries,
        next_cursor,
    })
}

/// Read `session_id`'s transcript out of the harness event stream at `stream`,
/// in the order the messages first appeared, paged by position.
///
/// The stream is the harness's own event JSON, one object per line as the app
/// forwards it. `message.updated` carries each message's shell (`info.id`,
/// `info.role`, `info.modelID`) and `message.part.updated` carries its parts,
/// grouped by `part.messageID`: a part the stream updates twice reads at its
/// last state, never twice. A line that is not JSON, an event this build does
/// not use and another session's events are skipped, not failures. A missing
/// file is [`DomainError::NoHarnessEvents`]; a stream with no message for the
/// session answers an empty page, the way the usage read answers no usage.
pub fn transcript_events(
    stream: &Path,
    session_id: &str,
    after: Option<i64>,
    limit: usize,
) -> DomainResult<TranscriptPage> {
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
    // Last state per message and per part; order is first sight, so a repeated
    // update never moves its message.
    let mut messages: Vec<(String, Value)> = Vec::new();
    let mut seen_message: BTreeSet<String> = BTreeSet::new();
    let mut parts: BTreeMap<String, (String, usize, Value)> = BTreeMap::new();
    let mut part_order: Vec<String> = Vec::new();
    for line in BufReader::new(file).lines() {
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
                    .unwrap_or_else(|| format!("event-{}", messages.len()));
                if seen_message.insert(key.clone()) {
                    messages.push((key, info.clone()));
                } else if let Some(found) = messages.iter_mut().find(|(id, _)| *id == key) {
                    found.1 = info.clone();
                }
            }
            Some("message.part.updated") => {
                let Some(part) = event.pointer("/properties/part") else {
                    continue;
                };
                if !of_session(part, session_id) {
                    continue;
                }
                let message = part
                    .get("messageID")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_default();
                let key = part
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("part-{}", part_order.len()));
                if let std::collections::btree_map::Entry::Vacant(slot) = parts.entry(key.clone()) {
                    part_order.push(key.clone());
                    slot.insert((message, part_order.len() - 1, part.clone()));
                } else if let Some(found) = parts.get_mut(&key) {
                    found.0 = message;
                    found.2 = part.clone();
                }
            }
            _ => {}
        }
    }
    let limit = limit.clamp(1, 500);
    let start = after.map(|after| after + 1).unwrap_or(0).max(0) as usize;
    let mut entries = Vec::new();
    for (ordinal, (id, info)) in messages.iter().enumerate().skip(start).take(limit) {
        let role = info
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let mut blocks = Vec::new();
        if let Some(text) = info.get("text").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                blocks.push(TranscriptBlock::Text {
                    text: text.to_string(),
                });
            }
        }
        let mut owned: Vec<(usize, &Value)> = part_order
            .iter()
            .filter_map(|key| parts.get(key))
            .filter(|(message, _, _)| message == id)
            .map(|(_, order, part)| (*order, part))
            .collect();
        owned.sort_by_key(|(order, _)| *order);
        for (_, part) in owned {
            if let Some(block) = event_block(part) {
                blocks.push(block);
            }
        }
        entries.push(TranscriptEntry {
            seq: ordinal as i64,
            id: id.clone(),
            role,
            agent: None,
            model: info
                .get("modelID")
                .and_then(Value::as_str)
                .and_then(non_blank),
            time_created: 0,
            time_updated: 0,
            blocks,
        });
    }
    let next_cursor = (entries.len() == limit)
        .then(|| entries.last().map(|entry| entry.seq))
        .flatten();
    Ok(TranscriptPage {
        entries,
        next_cursor,
    })
}

/// Whether the harness database holds a row for `session_id` at all.
fn holds_session(connection: &Connection, session_id: &str) -> DomainResult<bool> {
    let found: Option<String> = connection
        .query_row(
            "SELECT id FROM session_v2 WHERE id = ?1 LIMIT 1",
            params![session_id],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(DomainError::from(translate(other, connection))),
        })?;
    Ok(found.is_some())
}

/// One database message as a transcript entry: the human's text, or the
/// assistant's thinking, words and tool calls in the order they stream.
fn message_entry(
    id: &str,
    role: &str,
    seq: i64,
    created: i64,
    updated: i64,
    message: &Value,
) -> TranscriptEntry {
    let mut blocks = Vec::new();
    if role == "user" {
        if let Some(text) = message.get("text").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                blocks.push(TranscriptBlock::Text {
                    text: text.to_string(),
                });
            }
        }
    } else if let Some(content) = message.get("content").and_then(Value::as_array) {
        for part in content {
            if let Some(block) = content_block(part) {
                blocks.push(block);
            }
        }
    }
    TranscriptEntry {
        seq,
        id: id.to_string(),
        role: role.to_string(),
        agent: message
            .get("agent")
            .and_then(Value::as_str)
            .and_then(non_blank),
        model: message_model(message.get("model")),
        time_created: created,
        time_updated: updated,
        blocks,
    }
}

/// One content part of a database message: thinking, words, or a tool call.
fn content_block(part: &Value) -> Option<TranscriptBlock> {
    let name = part.get("name").and_then(Value::as_str);
    part_block(part, name)
}

/// One part of a forwarded event: text or thinking read their `text`, and a
/// tool part reads its call the way a database part does, under `tool` or
/// `name`.
fn event_block(part: &Value) -> Option<TranscriptBlock> {
    let name = part
        .get("tool")
        .or_else(|| part.get("name"))
        .and_then(Value::as_str);
    part_block(part, name)
}

/// One content part under either spelling: thinking and words read their
/// `text`, and a tool call reads its id, name and state.
fn part_block(part: &Value, name: Option<&str>) -> Option<TranscriptBlock> {
    match part.get("type").and_then(Value::as_str) {
        Some("text") => {
            part.get("text")
                .and_then(Value::as_str)
                .map(|text| TranscriptBlock::Text {
                    text: text.to_string(),
                })
        }
        Some("reasoning") => {
            part.get("text")
                .and_then(Value::as_str)
                .map(|text| TranscriptBlock::Reasoning {
                    text: text.to_string(),
                })
        }
        Some("tool") => Some(tool_block(
            part.get("id").and_then(Value::as_str).unwrap_or(""),
            name.unwrap_or("tool"),
            part.get("state"),
        )),
        _ => None,
    }
}

/// One tool call: its name and status, the inputs it ran, and the text its
/// output parts carried, joined in order.
fn tool_block(id: &str, name: &str, state: Option<&Value>) -> TranscriptBlock {
    let status = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let input = state
        .and_then(|state| state.get("input"))
        .cloned()
        .unwrap_or(Value::Null);
    let from_content = state
        .and_then(|state| state.get("content"))
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter_map(|part| {
                    if part.get("type").and_then(Value::as_str) == Some("text") {
                        part.get("text").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        });
    let from_output = state
        .and_then(|state| state.get("output"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let output = from_content
        .filter(|text| !text.is_empty())
        .or(from_output)
        .filter(|text| !text.is_empty());
    TranscriptBlock::Tool {
        id: id.to_string(),
        name: name.to_string(),
        status,
        input,
        output,
    }
}

/// The model one message names: the `id` inside its JSON model, a JSON string's
/// own value, or the text itself when it is not JSON.
fn message_model(model: Option<&Value>) -> Option<String> {
    match model {
        Some(Value::Object(model)) => model.get("id").and_then(Value::as_str).and_then(non_blank),
        Some(Value::String(text)) => non_blank(text),
        Some(Value::Null) | None => None,
        Some(_) => None,
    }
}
