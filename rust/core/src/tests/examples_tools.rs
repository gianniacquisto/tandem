//! The examples name only tools the MCP server exposes.
//!
//! `examples/` and the repo's `AGENTS.md` are prompts an agent reads as
//! instructions. A tool name that the server no longer serves (the retired
//! `session_*` and `note_capture` verbs were the case) makes the prompt order
//! a call that cannot happen. This suite reads every markdown file under
//! `examples/`, collects each snake_case word that starts like a tool family,
//! and requires it to be a registered tool or a named non-tool.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::mcp::tools;

/// The families a tool name starts with, plus the retired ones: a word that
/// begins with one of these is read as a tool name and checked.
const FAMILIES: [&str; 8] = [
    "ticket_",
    "note_",
    "lane_",
    "project_",
    "doctor_",
    "session_",
    "sprint_",
    "workflow_",
];

/// Words that start like a tool family but are not tools: a table name.
const NOT_TOOLS: [&str; 1] = ["ticket_dependencies"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn markdown_under(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read the examples folder") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            markdown_under(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            found.push(path);
        }
    }
}

/// Every snake_case word in `text` that starts with a tool family.
fn candidates(text: &str) -> BTreeSet<String> {
    let is_word = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
    let mut found = BTreeSet::new();
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if is_word(c) {
            word.push(c);
        } else {
            if !word.is_empty() && FAMILIES.iter().any(|f| word.starts_with(f)) {
                found.insert(word.trim_end_matches('_').to_string());
            }
            word.clear();
        }
    }
    found
}

#[test]
fn every_tool_an_example_names_is_served() {
    let served: BTreeSet<&str> = tools().iter().map(|tool| tool.name).collect();

    let mut files = Vec::new();
    markdown_under(&root().join("examples"), &mut files);
    assert!(!files.is_empty(), "the examples folder holds markdown");

    let mut unknown = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read an example");
        for word in candidates(&text) {
            if !served.contains(word.as_str()) && !NOT_TOOLS.contains(&word.as_str()) {
                unknown.push(format!("{}: `{word}`", file.display()));
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "examples name tools the MCP server does not expose:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn the_scan_reads_tool_names_and_skips_the_rest() {
    let found = candidates(
        "call `ticket_set`, then note_write's text; see ticket_dependencies and `tickets`.",
    );
    assert!(found.contains("ticket_set"));
    assert!(found.contains("note_write"));
    assert!(found.contains("ticket_dependencies"));
    assert!(!found.contains("tickets"));
}

#[test]
fn a_retired_verb_is_caught() {
    let served: BTreeSet<&str> = tools().iter().map(|tool| tool.name).collect();
    for retired in ["session_start", "session_end", "note_capture", "note_prune"] {
        assert!(!served.contains(retired), "{retired} is not served");
        assert!(candidates(&format!("run `{retired}`")).contains(retired));
    }
}
