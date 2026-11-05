//! The host instruction files, ported from `SetupInstall`'s edit half.
//!
//! OpenCode takes one `instructions` entry in `opencode.json`; Claude takes a
//! marked block in `CLAUDE.md`; `agents` has no instruction file. Both writes are
//! textual so the human's formatting survives, and the manifest records the exact
//! inserted text so removal is byte-exact.
//!
//! The OpenCode config is read tolerantly: the live file is JSONC (comments and
//! trailing commas) and the host parses it, so this reader strips them from a
//! copy to answer "is the entry there". The write itself is a textual insert.

use std::fs;
use std::path::Path;

use super::manifest::{Edit, Manifest, record_edit};
use super::paths::text;
use super::{Context, Failure, Host, PathAction, Result};

pub struct InstructionEdit {
    pub host: Host,
    pub file: std::path::PathBuf,
    pub kind: &'static str,
    /// The absolute `AGENTS.md` path (OpenCode's entry) or the block (Claude's).
    pub entry: String,
}

/// The bundled binary the MCP server entry spawns: the in-bundle path the
/// skill names, stable across upgrades (ADR 0003).
pub const MCP_BINARY: &str = "/Applications/Tandem.app/Contents/MacOS/tandem-cli";

/// The `tandem` server value inside the `mcp` object: a local spawn of the
/// bundled binary.
fn mcp_tandem_entry() -> String {
    format!("\"tandem\": {{\"type\": \"local\", \"command\": [\"{MCP_BINARY}\", \"mcp\"]}}")
}

/// The one instruction route a host takes, or none (`agents`).
pub fn instruction_edit(host: Host, context: &Context) -> Option<InstructionEdit> {
    let agents_md = text(&context.face().join("AGENTS.md"));
    match host {
        Host::Opencode => Some(InstructionEdit {
            host,
            file: context.opencode_config(),
            kind: "instructions-entry",
            entry: agents_md,
        }),
        Host::Claude => Some(InstructionEdit {
            host,
            file: context.claude_md(),
            kind: "claude-block",
            entry: format!("<!-- tandem:begin -->\n@{agents_md}\n<!-- tandem:end -->"),
        }),
        Host::Agents => None,
    }
}

/// The MCP server route: OpenCode's `mcp.tandem` entry in `opencode.json`,
/// so a fresh session lists working Tandem tools with no manual config.
/// Claude and `agents` take no JSON route here: Claude's servers are added
/// with `claude mcp add`, and `agents` has no instruction file.
pub fn mcp_edit(context: &Context) -> Option<InstructionEdit> {
    Some(InstructionEdit {
        host: Host::Opencode,
        file: context.opencode_config(),
        kind: "mcp-entry",
        entry: mcp_tandem_entry(),
    })
}

/// Apply one host's instruction edit, recording the exact text in the manifest.
pub fn apply(edit: &InstructionEdit, manifest: &mut Manifest) -> Result<PathAction> {
    if edit.kind == "mcp-entry" {
        return apply_mcp_entry(&edit.file, manifest);
    }
    let path = &edit.file;
    let existing = fs::read_to_string(path).ok();
    let path_text = text(path);

    match edit.kind {
        "instructions-entry" => {
            let entry = &edit.entry;
            if let Some(existing) = &existing {
                if let Some(object) = tolerant_json_object(existing) {
                    let present = object
                        .get("instructions")
                        .and_then(|value| value.as_array())
                        .is_some_and(|array| {
                            array
                                .iter()
                                .any(|value| value.as_str() == Some(entry.as_str()))
                        });
                    if present {
                        return Ok(PathAction::new(path_text, "ok"));
                    }
                } else {
                    return Ok(PathAction::with_reason(
                        path_text,
                        "skip",
                        "opencode.json is not JSON or JSONC; edit it by hand",
                    ));
                }
            }
            let (text, inserted) = match insert_instruction(entry, existing.as_deref()) {
                Ok(pair) => pair,
                Err(_) => {
                    return Ok(PathAction::with_reason(
                        path_text,
                        "skip",
                        "the instructions key could not be read",
                    ));
                }
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, text.as_bytes())?;
            record_edit(
                manifest,
                Edit::new(path_text.clone(), edit.kind, inserted, edit.host.as_str()),
            );
            Ok(PathAction::new(path_text, "linked"))
        }
        _ => {
            let block = &edit.entry;
            let (text, recorded, action) = match existing.as_deref() {
                None | Some("") => (format!("{block}\n"), format!("{block}\n"), "linked"),
                Some(existing) => match existing.find("<!-- tandem:begin -->").and_then(|start| {
                    existing[start..]
                        .find("<!-- tandem:end -->")
                        .map(|offset| (start, start + offset + "<!-- tandem:end -->".len()))
                }) {
                    Some((start, end)) => {
                        let current = &existing[start..end];
                        if current == block {
                            return Ok(PathAction::new(path_text, "ok"));
                        }
                        (
                            format!("{}{}{}", &existing[..start], block, &existing[end..]),
                            block.to_string(),
                            "replaced",
                        )
                    }
                    None => (
                        format!("{existing}\n{block}"),
                        format!("\n{block}"),
                        "linked",
                    ),
                },
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, text.as_bytes())?;
            record_edit(
                manifest,
                Edit::new(path_text.clone(), edit.kind, recorded, edit.host.as_str()),
            );
            Ok(PathAction::new(path_text, action))
        }
    }
}

/// Ensure OpenCode's `mcp.tandem` server entry, recording the exact inserted
/// text so removal is byte-exact. A present entry is `ok` whatever it points
/// at: a human customization is theirs, and setup never overwrites it.
fn apply_mcp_entry(file: &Path, manifest: &mut Manifest) -> Result<PathAction> {
    let path_text = text(file);
    let existing = fs::read_to_string(file).ok();
    let Some(existing) = existing.as_deref() else {
        let inserted = format!("{{\n  \"mcp\": {{{}}}\n}}\n", mcp_tandem_entry());
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, inserted.as_bytes())?;
        record_edit(
            manifest,
            Edit::new(path_text.clone(), "mcp-entry", inserted, "opencode"),
        );
        return Ok(PathAction::new(path_text, "linked"));
    };
    let Some(object) = tolerant_json_object(existing) else {
        return Ok(PathAction::with_reason(
            path_text,
            "skip",
            "opencode.json is not JSON or JSONC; edit it by hand",
        ));
    };
    if object
        .get("mcp")
        .and_then(|mcp| mcp.get("tandem"))
        .is_some()
    {
        return Ok(PathAction::new(path_text, "ok"));
    }
    let Some((file_text, snippet)) = insert_mcp_server(existing) else {
        return Ok(PathAction::with_reason(
            path_text,
            "skip",
            "the mcp key is unreadable; edit it by hand",
        ));
    };
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, file_text.as_bytes())?;
    record_edit(
        manifest,
        Edit::new(path_text.clone(), "mcp-entry", snippet, "opencode"),
    );
    Ok(PathAction::new(path_text, "linked"))
}

/// The file's new text and the recorded snippet for an `mcp.tandem` insert:
/// the whole `mcp` key when none is there, else the `tandem` entry at the
/// start of the existing `mcp` object. `None` when the shape is not an object
/// the text can carry.
fn insert_mcp_server(existing: &str) -> Option<(String, String)> {
    let entry = mcp_tandem_entry();
    if !existing.contains("\"mcp\"") {
        let open = existing.find('{')?;
        let snippet = format!("\"mcp\": {{{entry}}},\n");
        let mut file_text = existing.to_string();
        file_text.insert_str(open + 1, &snippet);
        return Some((file_text, snippet));
    }
    let key = existing.find("\"mcp\"")?;
    let open = existing[key..].find('{').map(|offset| key + offset)?;
    let after: Vec<char> = existing[open + 1..].chars().collect();
    let mut index = 0;
    while index < after.len() && after[index].is_whitespace() {
        index += 1;
    }
    let snippet = if after.get(index) == Some(&'}') {
        entry
    } else {
        format!("{entry}, ")
    };
    let mut file_text = existing.to_string();
    file_text.insert_str(open + 1, &snippet);
    Some((file_text, snippet))
}

/// The exact text inserted, and the file's new text.
fn insert_instruction(entry: &str, existing: Option<&str>) -> Result<(String, String)> {
    let Some(existing) = existing else {
        let inserted = format!("{{\n  \"instructions\": [\"{entry}\"]\n}}\n");
        return Ok((inserted.clone(), inserted));
    };
    let Some(open) = existing.find('{') else {
        return Err(Failure::Setup(super::SetupError::InvalidBundle(format!(
            "{} has no object",
            &existing[..existing.len().min(20)]
        ))));
    };
    if let Some(key) = existing.find("\"instructions\"") {
        let Some(bracket) = existing[key..].find('[').map(|offset| key + offset) else {
            return Err(Failure::Setup(super::SetupError::InvalidBundle(
                "the instructions array is unreadable".to_string(),
            )));
        };
        let Some(close) = closing_bracket(existing, bracket) else {
            return Err(Failure::Setup(super::SetupError::InvalidBundle(
                "the instructions array is unreadable".to_string(),
            )));
        };
        let inside = &existing[bracket + 1..close];
        let inserted = if inside.trim().is_empty() {
            format!("\"{entry}\"")
        } else {
            format!(", \"{entry}\"")
        };
        let mut text = existing.to_string();
        text.insert_str(close, &inserted);
        return Ok((text, inserted));
    }
    let inserted = format!("\"instructions\": [\"{entry}\"],\n");
    let mut text = existing.to_string();
    text.insert_str(open + 1, &inserted);
    Ok((text, inserted))
}

/// The `]` matching the `[` at `start`, scanning a string array.
fn closing_bracket(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut index = start;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let character = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == b'\\' {
                escaped = true;
            } else if character == b'"' {
                in_string = false;
            }
        } else {
            match character {
                b'"' => in_string = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// Parse strictly, then tolerantly: the live config is JSONC (comments and
/// trailing commas), and the host reads it, so this reader must too.
fn tolerant_json_object(text: &str) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && value.is_object()
    {
        return Some(value);
    }
    let stripped = strip_jsonc(text);
    serde_json::from_str::<serde_json::Value>(&stripped)
        .ok()
        .filter(|value| value.is_object())
}

/// Remove `//` and `/* */` comments and trailing commas from a copy. The answer
/// it serves is "is the entry there", and the write is textual.
fn strip_jsonc(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < characters.len() {
        let character = characters[index];
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            index += 1;
            continue;
        }
        if character == '/' && index + 1 < characters.len() {
            let next = characters[index + 1];
            if next == '/' {
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
                continue;
            }
            if next == '*' {
                index += 2;
                while index < characters.len() {
                    if characters[index] == '*'
                        && index + 1 < characters.len()
                        && characters[index + 1] == '/'
                    {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
        }
        output.push(character);
        index += 1;
    }
    remove_trailing_commas(&output)
}

fn remove_trailing_commas(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in characters.iter().enumerate() {
        if in_string {
            output.push(*character);
            if escaped {
                escaped = false;
            } else if *character == '\\' {
                escaped = true;
            } else if *character == '"' {
                in_string = false;
            }
            continue;
        }
        if *character == '"' {
            in_string = true;
            output.push(*character);
            continue;
        }
        if *character == ',' {
            let mut next = index + 1;
            while next < characters.len() && characters[next].is_whitespace() {
                next += 1;
            }
            if next < characters.len() && (characters[next] == ']' || characters[next] == '}') {
                continue;
            }
        }
        output.push(*character);
    }
    output
}

/// The block marker Claude's file carries; `half-installed` reads it.
pub const CLAUDE_BLOCK_BEGIN: &str = "<!-- tandem:begin -->";

/// Whether `path`'s text carries the edit the host expects: OpenCode's entry,
/// or Claude's marked block.
pub fn carries(path: &Path, kind: &str, entry: &str) -> bool {
    let text = fs::read_to_string(path).unwrap_or_default();
    if kind == "instructions-entry" {
        text.contains(entry)
    } else if kind == "mcp-entry" {
        text.contains("\"tandem\"")
    } else {
        text.contains(CLAUDE_BLOCK_BEGIN)
    }
}
