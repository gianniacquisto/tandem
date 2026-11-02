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
use std::time::Duration;

use serde_json::Value;

use super::error::{DomainError, DomainResult};
use super::session::{SessionTokens, SkillCounts};
use crate::store::StoreError;

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
