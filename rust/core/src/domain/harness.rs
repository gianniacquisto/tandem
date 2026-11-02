//! The harness read: what the reporting harness recorded, as session usage.
//!
//! `session end --usage-from-harness` reads this in the core, so it ships inside
//! the one binary and the CLI composes rather than shells out (`docs/port.md`,
//! Where the old code goes: the wrapper script became this verb at the cutover).
//!
//! The loopback API is the only read (TAN-276). The harness serves its numbers
//! as first-class fields: `GET /api/session/{id}` carries the tokens, the cost
//! and the model, and `GET /api/session/{id}/message?limit=100` carries the
//! tool parts a skill load rides in, one `skill` part per load counted by name
//! (TAN-241). A number the harness did not report stays absent, and a skill
//! object with no loads stays absent too, never an empty map.
//!
//! Authentication is ADR 0038's: `Authorization: Basic base64("opencode:" +
//! password)` on every request. The password never reaches an error string, a
//! log, or argv. The URL's host must be loopback, and the connect and read
//! budgets are bounded in single-digit seconds, so a hung server cannot hang
//! `session end`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

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

/// Read `session_id`'s usage over the harness loopback API.
///
/// `base_url` is the harness origin (`http://127.0.0.1:port`), `password` the
/// loopback credential. A host that is not loopback is refused without
/// connecting; an unreachable server, a timeout, or a refused host answers
/// [`DomainError::NoHarnessServer`]. A wrong password answers
/// [`DomainError::HarnessUnauthorized`]; a session the harness never recorded
/// answers [`DomainError::UnknownHarnessSession`].
pub fn read_api(base_url: &str, password: &str, session_id: &str) -> DomainResult<HarnessUsage> {
    let base = base_url.trim().trim_end_matches('/').to_string();
    if !is_loopback(&base) {
        return Err(DomainError::NoHarnessServer { url: base });
    }
    let id = encode_segment(session_id.trim());
    if id.is_empty() {
        return Err(DomainError::UnknownHarnessSession {
            id: session_id.to_string(),
        });
    }
    let agent = loopback_agent();
    let auth = format!(
        "Basic {}",
        encode_base64(format!("opencode:{password}").as_bytes())
    );

    let session_url = format!("{base}/api/session/{id}");
    let session_body = get_json(&agent, &session_url, &auth, &base)?;
    let data = session_body
        .get("data")
        .ok_or_else(|| unreadable(&base, 200, &session_body.to_string()))?;
    let mut usage = HarnessUsage {
        tokens: session_tokens(data.get("tokens")),
        cost: data.get("cost").and_then(Value::as_f64),
        model: session_model(data.get("model")),
        skills: None,
    };

    let mut skills: BTreeMap<String, i64> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut part_index = 0usize;
    let mut cursor: Option<String> = None;
    loop {
        let mut page_url = format!("{base}/api/session/{id}/message?limit=100");
        if let Some(next) = cursor.as_deref() {
            page_url.push_str("&cursor=");
            page_url.push_str(&encode_segment(next));
        }
        let page_body = get_json(&agent, &page_url, &auth, &base)?;
        let messages = page_body
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| unreadable(&base, 200, &page_body.to_string()))?;
        for message in messages {
            let Some(content) = message.get("content").and_then(Value::as_array) else {
                continue;
            };
            for part in content {
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
                if let Some(owner) = part.get("sessionID").and_then(Value::as_str)
                    && owner != session_id.trim()
                {
                    continue;
                }
                let key = part
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        part_index += 1;
                        format!("part-{part_index}")
                    });
                part_index += 1;
                if !seen.insert(key) {
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
        }
        let next = page_body
            .pointer("/cursor/next")
            .and_then(Value::as_str)
            .filter(|next| !next.is_empty())
            .map(str::to_string);
        match next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    usage.skills = (!skills.is_empty()).then_some(skills);
    Ok(usage)
}

/// A bounded blocking client for the loopback: a 3 s connect budget and an
/// 8 s whole-call budget, so a hung server cannot hang `session end`.
fn loopback_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

/// One GET with the harness credential, mapped to the domain failures.
/// A 401 names the password sources; a 404 on the session names the id; any
/// other non-2xx or an unreadable body is an `Io` with the status and a body
/// prefix. Transport failures (unreachable, timeout, refused) name the URL
/// tried and the flag or env that overrides it.
fn get_json(agent: &ureq::Agent, url: &str, auth: &str, base: &str) -> DomainResult<Value> {
    let mut response = agent
        .get(url)
        .header("Authorization", auth)
        .call()
        .map_err(|error| match error {
            ureq::Error::StatusCode(401) => DomainError::HarnessUnauthorized {
                url: base.to_string(),
            },
            ureq::Error::StatusCode(status) => {
                DomainError::from(StoreError::Io(harness_io(base, status, "")))
            }
            ureq::Error::Timeout(_)
            | ureq::Error::ConnectionFailed
            | ureq::Error::Io(_)
            | ureq::Error::HostNotFound
            | ureq::Error::BadUri(_) => DomainError::NoHarnessServer {
                url: base.to_string(),
            },
            _ => DomainError::NoHarnessServer {
                url: base.to_string(),
            },
        })?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|error| {
        DomainError::from(StoreError::Io(format!(
            "could not read the harness response from {base} (status {status}): {error}"
        )))
    })?;
    if status == 401 {
        return Err(DomainError::HarnessUnauthorized {
            url: base.to_string(),
        });
    }
    if status == 404 && url.contains("/api/session/") {
        let id = url
            .rsplit("/api/session/")
            .next()
            .unwrap_or("")
            .split(['/', '?'])
            .next()
            .unwrap_or("");
        let id = percent_decode(id);
        return Err(DomainError::UnknownHarnessSession { id });
    }
    if !(200..300).contains(&status) {
        return Err(DomainError::from(StoreError::Io(harness_io(
            base, status, &body,
        ))));
    }
    serde_json::from_str::<Value>(&body).map_err(|_| unreadable(base, status, &body))
}

/// Whether `base_url` names a loopback host. The credential never leaves the
/// machine, so a URL that would send it elsewhere is refused before connecting.
fn is_loopback(base_url: &str) -> bool {
    let Some(after_scheme) = base_url.split_once("://").map(|(_, rest)| rest) else {
        return false;
    };
    let host_port = after_scheme.split('/').next().unwrap_or("");
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "::1"
    )
}

/// Percent-encode one path segment: session ids are URL-safe (`ses_...`), and
/// anything else rides encoded rather than breaking the path.
fn encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((high << 4) | low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Standard base64 for `Basic opencode:password`. The harness credential is
/// small and the alphabet is fixed, so the encoder lives here rather than as
/// a second dependency beside the HTTP client.
fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let a = bytes[i] as u32;
        let b = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let c = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if i + 1 < bytes.len() {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < bytes.len() {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

/// The five counts the session reported, each absent where it reported none.
fn session_tokens(value: Option<&Value>) -> SessionTokens {
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

/// The model the session reports: the `id` inside the harness's model object.
/// A JSON `null` or a missing model is no model at all, never a model called
/// `"null"`, under the same rule the retired reader used.
fn session_model(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Object(model)) => model.get("id").and_then(Value::as_str).and_then(non_blank),
        Some(Value::String(text)) => non_blank(text),
        Some(_) => None,
    }
}

fn non_blank(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn unreadable(base: &str, status: u16, body: &str) -> DomainError {
    DomainError::from(StoreError::Io(harness_io(base, status, body)))
}

fn harness_io(base: &str, status: u16, body: &str) -> String {
    let prefix: String = body.chars().take(200).collect();
    format!("could not read the harness response from {base} (status {status}): {prefix}")
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

/// Whether an event value belongs to `session_id`.
fn of_session(value: &Value, session_id: &str) -> bool {
    value.get("sessionID").and_then(Value::as_str) == Some(session_id)
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
