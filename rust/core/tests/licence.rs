//! The licence facts the relicense (ADR 0057) pins, read from the tree.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

#[test]
fn the_licence_file_is_the_agpl() {
    let l = read("LICENSE");
    assert!(l.contains("GNU AFFERO GENERAL PUBLIC LICENSE"));
    assert!(l.contains("Version 3"));
}

#[test]
fn the_workspace_is_agpl_and_both_crates_inherit_it() {
    assert!(read("rust/Cargo.toml").contains("license = \"AGPL-3.0-only\""));
    for c in ["rust/core/Cargo.toml", "rust/cli/Cargo.toml"] {
        assert!(read(c).contains("license.workspace = true"), "{c}");
    }
}

#[test]
fn examples_stay_mit() {
    assert!(read("examples/LICENSE").starts_with("MIT License"));
    for m in [
        "examples/.claude-plugin/plugin.json",
        ".claude-plugin/marketplace.json",
    ] {
        assert!(read(m).contains("\"license\": \"MIT\""), "{m}");
    }
}

#[test]
fn the_notice_keeps_the_mit_notice_and_lists_real_skills() {
    let n = read("NOTICE.txt");
    assert!(n.contains("MIT License"));
    assert!(n.contains("Copyright (c) 2026 Gianni Acquisto"));
    assert!(!n.contains("Swift"));
    let mut seen = 0;
    for line in n.lines() {
        if let Some(p) = line.trim().strip_prefix("* examples/skills/") {
            assert!(root().join("examples/skills").join(p).is_file(), "{p}");
            seen += 1;
        }
    }
    assert_eq!(seen, 7);
}

#[test]
fn the_readme_licence_section_names_both_licences() {
    let r = read("README.md");
    let sec = r.split("## Licence").nth(1).expect("## Licence");
    assert!(sec.contains("AGPL-3.0-only"));
    assert!(sec.contains("examples/LICENSE"));
}
