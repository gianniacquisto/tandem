//! `docs/adr/README.md` indexes every record (DOC-4).
//!
//! Each `docs/adr/NNNN-*.md` needs a row in the index that links its file name,
//! and a `> **Status:**` line of its own, so the index and the record cannot
//! drift apart unseen. Two records share 0047, so the match is by file name,
//! not by number.

use std::fs;
use std::path::{Path, PathBuf};

fn adr_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/adr")
        .canonicalize()
        .expect("docs/adr resolves")
}

#[test]
fn every_adr_has_an_index_row_and_a_status_line() {
    let dir = adr_dir();
    let index = fs::read_to_string(dir.join("README.md")).expect("the index reads");
    let mut problems = Vec::new();
    let mut seen = 0;
    for entry in fs::read_dir(&dir).expect("docs/adr lists") {
        let name = entry.expect("entry").file_name().to_string_lossy().into_owned();
        let numbered = name.len() > 5
            && name.ends_with(".md")
            && name.chars().take(4).all(|c| c.is_ascii_digit());
        if !numbered {
            continue;
        }
        seen += 1;
        if !index.lines().any(|l| l.starts_with("| ") && l.contains(&format!("]({name})"))) {
            problems.push(format!("{name}: no row in docs/adr/README.md"));
        }
        let text = fs::read_to_string(dir.join(&name)).expect("the record reads");
        if !text.lines().any(|l| l.starts_with("> **Status:**")) {
            problems.push(format!("{name}: no `> **Status:**` line"));
        }
    }
    assert!(seen > 0, "no records found");
    assert!(problems.is_empty(), "ADR index out of step:\n{}", problems.join("\n"));
}
