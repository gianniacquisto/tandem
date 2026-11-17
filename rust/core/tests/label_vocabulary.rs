//! The word "lane" is retired from everything live (UI-37).
//!
//! A ticket carries a label: the board, the CLI, the MCP tools, the wire, the
//! store and the docs all say so. This suite reads every tracked text file and
//! fails on a "lane" that is not in the allowlist below, which is the history
//! (shipped migrations, the ADRs that recorded the word, the epic that named
//! it) and the cases that prove the old names are gone.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Files that may say "lane" and why.
const ALLOWED: [&str; 11] = [
    // Shipped migrations are never edited, and the v12 rename names the old shape.
    "rust/core/src/store/schema.rs",
    // The migration tests build v1 to v11 files, which are lane-shaped.
    "rust/core/src/store/tests.rs",
    // A read before the first write still answers on a v11 file's lane names.
    "rust/core/src/store/board_rows.rs",
    // The export's table classification names the v11 file's table, so an
    // export of a store behind this build classifies every table it finds.
    "rust/core/src/store/export.rs",
    // The record of the word, and the epic that named it.
    "docs/epic-lanes.md",
    "docs/adr/0047-lanes-are-labels.md",
    // The cases that prove the old names answer as unknown.
    "rust/cli/tests/label_move.rs",
    "rust/core/tests/mcp_server.rs",
    "rust/core/src/serve/tests.rs",
    // This suite names the word it hunts.
    "rust/core/tests/label_vocabulary.rs",
    // The glossary's `_Avoid_` lines teach the retired word.
    "GLOSSARY.md",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root resolves")
}

#[test]
fn no_live_file_says_lane() {
    let root = repo_root();
    let listed = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&root)
        .output()
        .expect("git lists the tracked files");
    assert!(listed.status.success(), "git ls-files answers");
    let allowed: BTreeSet<&str> = ALLOWED.into_iter().collect();

    let mut offenders = Vec::new();
    for name in String::from_utf8_lossy(&listed.stdout).split('\0') {
        if name.is_empty()
            || allowed.contains(name)
            || (name.starts_with("docs/adr/") && name != "docs/adr/0047-lanes-are-labels.md")
            || name.ends_with(".db")
        {
            continue;
        }
        let Ok(text) = fs::read(root.join(name)).map(String::from_utf8) else {
            continue;
        };
        let Ok(text) = text else { continue };
        for (number, line) in text.lines().enumerate() {
            let line = line.to_lowercase().replace("swimlane", "");
            if line.contains("lane") {
                offenders.push(format!("{name}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a live file still says lane (say label, or allowlist history):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_glossary_names_lane_only_as_a_word_to_avoid() {
    let glossary = fs::read_to_string(repo_root().join("GLOSSARY.md")).expect("the glossary");
    for (number, line) in glossary.lines().enumerate() {
        if line.to_lowercase().replace("swimlane", "").contains("lane") {
            assert!(
                line.starts_with("_Avoid_"),
                "GLOSSARY.md:{} names lane outside an _Avoid_ line: {line}",
                number + 1
            );
        }
    }
}
