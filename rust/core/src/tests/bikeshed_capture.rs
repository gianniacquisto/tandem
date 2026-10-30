//! Ported from `BikeshedCaptureTests`: the hidden capture layer's naming rule
//! (path, slug, UTC stamp), collision suffixes, project/session validation, the
//! junk gate, the duplicate no-op, and the property the layer exists for: a
//! captured note is out of the curated graph.

use std::fs;

use crate::bikeshed::{CaptureKind, NodeKind};
use crate::domain::error::DomainError;

use super::bikeshed_support::{INSTANT, Vault, instant};

const STAMP: &str = "20261018T140211Z";

fn capture(vault: &Vault, kind: CaptureKind, body: &str, session: &str) -> String {
    vault
        .bikeshed()
        .capture("tandem", kind, body, session, instant(INSTANT))
        .expect("the capture is filed")
        .value
}

fn list_paths(vault: &Vault) -> Vec<String> {
    let mut paths = Vec::new();
    fn walk(nodes: &[crate::bikeshed::BikeshedNode], paths: &mut Vec<String>) {
        for node in nodes {
            paths.push(node.path.clone());
            walk(&node.children, paths);
        }
    }
    walk(&vault.bikeshed().list().unwrap().value, &mut paths);
    paths
}

fn files_under_raw(vault: &Vault, project: &str) -> Vec<String> {
    let raw = vault.dir().join(format!("projects/{project}/.raw"));
    let Ok(sessions) = fs::read_dir(&raw) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for session in sessions.flatten() {
        if session.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(files) = fs::read_dir(session.path()) else {
            continue;
        };
        for file in files.flatten() {
            found.push(file.file_name().to_string_lossy().to_string());
        }
    }
    found
}

#[test]
fn capture_writes_the_body_verbatim_and_returns_the_path() {
    let vault = Vault::new("capture-write");
    let body = "# Caret not placed\n\nThe editor opened without focus; a click was needed before typing.\n";
    let result = vault
        .bikeshed()
        .capture(
            "tandem",
            CaptureKind::Gotcha,
            body,
            "ses_test",
            instant(INSTANT),
        )
        .unwrap();
    let expected = format!("projects/tandem/.raw/ses_test/{STAMP}-gotcha-caret-not-placed.md");
    assert_eq!(result.value, expected);
    assert_eq!(result.note, None);
    assert_eq!(vault.read(&expected), body);
}

#[test]
fn capture_creates_the_parent_folders() {
    let vault = Vault::new("capture-parents");
    capture(&vault, CaptureKind::Fact, "a fact", "ses_x");
    assert!(vault.dir().join("projects/tandem/.raw/ses_x").is_dir());
}

#[test]
fn capture_slug_strips_leading_hashes_collapses_and_caps_at_40() {
    let vault = Vault::new("capture-slug");
    let body = "\n\n   ### Gotcha — the caret wasn't placed on open!!\nmore";
    assert_eq!(
        capture(&vault, CaptureKind::Gotcha, body, "s"),
        format!("projects/tandem/.raw/s/{STAMP}-gotcha-gotcha-the-caret-wasn-t-placed-on-open.md")
    );
    let long = format!("# {}", "a".repeat(45));
    assert_eq!(
        capture(&vault, CaptureKind::Fact, &long, "s"),
        format!("projects/tandem/.raw/s/{STAMP}-fact-{}.md", "a".repeat(40))
    );
}

#[test]
fn capture_refuses_an_empty_or_whitespace_only_body() {
    let vault = Vault::new("capture-empty");
    let bikeshed = vault.bikeshed();
    for body in ["", "   ", "\n\n", " \t\n "] {
        assert_eq!(
            bikeshed
                .capture("tandem", CaptureKind::Fact, body, "s", instant(INSTANT))
                .unwrap_err(),
            DomainError::EmptyCaptureBody
        );
    }
    assert!(!vault.dir().exists(), "a refused capture creates nothing");
}

#[test]
fn capture_refuses_a_body_whose_slug_is_empty() {
    let vault = Vault::new("capture-slugless");
    let bikeshed = vault.bikeshed();
    for body in ["!!!", "# !!!", "---", "###"] {
        assert_eq!(
            bikeshed
                .capture("tandem", CaptureKind::Fact, body, "s", instant(INSTANT))
                .unwrap_err(),
            DomainError::EmptyCaptureSlug("fact".to_string())
        );
    }
    assert!(!vault.dir().exists());
}

#[test]
fn capture_accepts_a_title_body() {
    let vault = Vault::new("capture-title");
    assert_eq!(
        capture(&vault, CaptureKind::Fact, "# x\n\nbody", "s"),
        format!("projects/tandem/.raw/s/{STAMP}-fact-x.md")
    );
}

#[test]
fn capture_deduplicates_the_same_project_kind_and_body() {
    let vault = Vault::new("capture-dedupe");
    let bikeshed = vault.bikeshed();
    let body = "# Zanzibar\n\nthe body";
    let first = bikeshed
        .capture(
            "tandem",
            CaptureKind::Gotcha,
            body,
            "ses_a",
            instant(INSTANT),
        )
        .unwrap();
    let second = bikeshed
        .capture(
            "tandem",
            CaptureKind::Gotcha,
            body,
            "ses_b",
            instant(INSTANT + 60),
        )
        .unwrap();
    assert_eq!(second.value, first.value);
    assert!(
        second.note.is_some(),
        "the re-file says why nothing was written"
    );
    assert_eq!(files_under_raw(&vault, "tandem").len(), 1);
}

#[test]
fn dedupe_identity_is_kind_body_and_project() {
    let vault = Vault::new("capture-identity");
    let bikeshed = vault.bikeshed();
    let body = "# Zanzibar";
    bikeshed
        .capture("tandem", CaptureKind::Gotcha, body, "s", instant(INSTANT))
        .unwrap();

    let other_kind = bikeshed
        .capture("tandem", CaptureKind::Fact, body, "s", instant(INSTANT))
        .unwrap();
    let other_body = bikeshed
        .capture(
            "tandem",
            CaptureKind::Gotcha,
            "# Other",
            "s",
            instant(INSTANT),
        )
        .unwrap();
    let other_project = bikeshed
        .capture("other", CaptureKind::Gotcha, body, "s", instant(INSTANT))
        .unwrap();

    assert_eq!(other_kind.note, None);
    assert_eq!(other_body.note, None);
    assert_eq!(other_project.note, None);
    assert_eq!(files_under_raw(&vault, "tandem").len(), 3);
    assert_eq!(files_under_raw(&vault, "other").len(), 1);
}

#[test]
fn dedupe_ignores_the_archive() {
    let vault = Vault::new("capture-archive");
    let done = vault.dir().join("projects/tandem/.raw/.done/ses_old");
    fs::create_dir_all(&done).unwrap();
    fs::write(
        done.join(format!("{STAMP}-gotcha-zanzibar.md")),
        "# Zanzibar",
    )
    .unwrap();

    let filed = vault
        .bikeshed()
        .capture(
            "tandem",
            CaptureKind::Gotcha,
            "# Zanzibar",
            "ses_new",
            instant(INSTANT),
        )
        .unwrap();
    assert_eq!(filed.note, None, "the archive is out of the pending scan");
    assert!(filed.value.starts_with("projects/tandem/.raw/ses_new/"));
}

#[test]
fn capture_suffixes_a_same_second_same_slug_collision_and_keeps_both() {
    let vault = Vault::new("capture-collision");
    let body = "# Caret not placed";
    let twin = "# caret not placed";
    let first = capture(&vault, CaptureKind::Gotcha, body, "ses");
    let second = capture(&vault, CaptureKind::Gotcha, twin, "ses");
    assert_eq!(
        first,
        format!("projects/tandem/.raw/ses/{STAMP}-gotcha-caret-not-placed.md")
    );
    assert_eq!(
        second,
        format!("projects/tandem/.raw/ses/{STAMP}-gotcha-caret-not-placed-2.md")
    );
    assert_eq!(vault.read(&first), body);
    assert_eq!(vault.read(&second), twin);
}

#[test]
fn capture_rejects_a_bad_project_or_session() {
    let vault = Vault::new("capture-bad-names");
    let bikeshed = vault.bikeshed();
    for bad in ["", ".", "..", ".hidden", "a/b", "a#b", "a|b"] {
        assert_eq!(
            bikeshed
                .capture(bad, CaptureKind::Fact, "x", "ses", instant(INSTANT))
                .unwrap_err(),
            DomainError::InvalidCaptureName(bad.to_string())
        );
        assert_eq!(
            bikeshed
                .capture("tandem", CaptureKind::Fact, "x", bad, instant(INSTANT))
                .unwrap_err(),
            DomainError::InvalidCaptureName(bad.to_string())
        );
    }
    assert!(!vault.dir().exists(), "a rejected capture creates nothing");
}

#[test]
fn a_captured_note_is_absent_from_list_and_note_paths() {
    let vault = Vault::new("capture-out-of-graph");
    let bikeshed = vault.bikeshed();
    bikeshed.write("Real.md", "a curated note").unwrap();
    let path = capture(&vault, CaptureKind::Fact, "# Zanzibar\n", "ses");

    let listed = list_paths(&vault);
    assert!(!listed.contains(&path));
    assert!(!listed.iter().any(|entry| entry.contains(".raw")));
    assert!(!bikeshed.note_paths().contains(&path));
    assert!(listed.contains(&"Real.md".to_string()));
    assert!(listed.contains(&"projects".to_string()));
    assert!(listed.contains(&"projects/tandem".to_string()));
}

#[test]
fn a_captured_note_is_not_searchable_or_resolvable() {
    let vault = Vault::new("capture-hidden-graph");
    let bikeshed = vault.bikeshed();
    let path = capture(&vault, CaptureKind::Fact, "# Zanzibar unicorn\n", "ses");
    assert!(bikeshed.search("Zanzibar").unwrap().value.is_empty());

    let stem = path
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".md")
        .to_string();
    assert_eq!(
        bikeshed.resolve(&stem),
        crate::bikeshed::wikilink::LinkResolution::Broken
    );
    assert!(bikeshed.note_paths().iter().all(|note| {
        note.rsplit('/')
            .next()
            .map(|name| name.trim_end_matches(".md"))
            != Some(stem.as_str())
    }));
}

#[test]
fn the_kind_parser_refuses_the_retired_and_the_unknown() {
    assert_eq!(
        CaptureKind::parse("decision").unwrap(),
        CaptureKind::Decision
    );
    assert_eq!(
        CaptureKind::parse("work").unwrap_err(),
        DomainError::RetiredCaptureKind("work".to_string())
    );
    assert_eq!(
        CaptureKind::parse("nonsense").unwrap_err(),
        DomainError::InvalidCaptureKind("nonsense".to_string())
    );
}

#[test]
fn a_captured_note_is_a_note_node_only_when_revealed() {
    let vault = Vault::new("capture-node");
    capture(&vault, CaptureKind::Fact, "# Zanzibar\n", "ses");
    let revealed = vault.revealed().list().unwrap().value;
    fn walk(nodes: &[crate::bikeshed::BikeshedNode]) -> bool {
        nodes.iter().any(|node| {
            (node.kind == NodeKind::Note && node.path.contains(".raw")) || walk(&node.children)
        })
    }
    assert!(walk(&revealed));
}
