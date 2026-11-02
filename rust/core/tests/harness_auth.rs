//! The harness sign-in seam (TAN-265, ADR 0038): where the embedded harness keeps
//! its model credentials, and what counts as signed in.
//!
//! The credentials live in the harness home only, never in the human's own
//! opencode store. `auth list --standalone` under the harness environment reads
//! the same table this module reads, so the Settings face and `doctor` agree
//! without sharing a process.

use std::path::PathBuf;

use rusqlite::Connection;
use tandem_core::harness::{self, HarnessAuth};

mod support;
use support::Sandbox;

/// A harness database holding credentials, in the shape `auth list` reads:
/// one `credential` row per stored account.
fn harness_database(path: &PathBuf, rows: &[(&str, &str)]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the harness data dir is creatable");
    }
    let connection = Connection::open(path).expect("the harness database opens");
    connection
        .execute_batch(
            "CREATE TABLE credential (id TEXT PRIMARY KEY, integration_id TEXT NOT NULL, label TEXT NOT NULL)",
        )
        .expect("the credential table is creatable");
    for (index, (integration, label)) in rows.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO credential (id, integration_id, label) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("cred_{index}"), integration, label],
            )
            .expect("the credential row is writable");
    }
}

#[test]
fn the_harness_root_is_a_folder_of_the_tandem_home() {
    let sandbox = Sandbox::new("harness-root");
    let root = harness::root(&sandbox.home());
    assert_eq!(root, sandbox.home().directory().join("harness"));
    assert_eq!(
        harness::database(&sandbox.home()),
        root.join("opencode.db"),
        "OPENCODE_DB per ADR 0038: the store beside tandem.db, not under data/"
    );
}

#[test]
fn a_home_with_no_harness_database_is_not_signed_in() {
    let sandbox = Sandbox::new("harness-absent");
    assert_eq!(
        harness::read(&sandbox.home()),
        HarnessAuth::NotStored,
        "the harness starts unauthenticated: no database is no credential, not an error"
    );
}

#[test]
fn a_console_credential_in_the_harness_home_is_stored() {
    let sandbox = Sandbox::new("harness-stored");
    harness_database(
        &harness::database(&sandbox.home()),
        &[("opencode", "Personal")],
    );
    match harness::read(&sandbox.home()) {
        HarnessAuth::Stored {
            account,
            integrations,
        } => {
            assert_eq!(account, "Personal");
            assert!(integrations.contains(&"opencode".to_string()));
        }
        other => panic!("a Console credential is stored, got {other:?}"),
    }
}

#[test]
fn other_integrations_alone_are_not_a_console_sign_in() {
    let sandbox = Sandbox::new("harness-other");
    harness_database(
        &harness::database(&sandbox.home()),
        &[("openrouter", "OpenRouter")],
    );
    match harness::read(&sandbox.home()) {
        HarnessAuth::Stored { .. } => {
            panic!("a key for another provider is not the Console sign-in")
        }
        HarnessAuth::NotStored => {}
        other => panic!("expected not-stored, got {other:?}"),
    }
}

#[test]
fn the_data_fallback_holds_when_opencode_db_is_unset() {
    let sandbox = Sandbox::new("harness-fallback");
    harness_database(
        &harness::data_database(&sandbox.home()),
        &[("opencode", "Personal")],
    );
    assert!(
        matches!(harness::read(&sandbox.home()), HarnessAuth::Stored { .. }),
        "without OPENCODE_DB the credentials sit under data/opencode/, and that still counts"
    );
}

#[test]
fn a_console_credential_lists_every_stored_integration() {
    let sandbox = Sandbox::new("harness-integrations");
    harness_database(
        &harness::database(&sandbox.home()),
        &[("opencode", "Personal"), ("openrouter", "OpenRouter")],
    );
    match harness::read(&sandbox.home()) {
        HarnessAuth::Stored { integrations, .. } => {
            assert_eq!(
                integrations,
                vec!["opencode".to_string(), "openrouter".to_string()]
            );
        }
        other => panic!("expected stored, got {other:?}"),
    }
}

#[test]
fn nothing_is_created_or_touched_outside_the_harness_home() {
    let sandbox = Sandbox::new("harness-sealed");
    let user = sandbox.root().join("user");
    let before = ledger(&user);
    harness_database(
        &harness::database(&sandbox.home()),
        &[("opencode", "Personal")],
    );
    let _ = harness::read(&sandbox.home());
    assert_eq!(
        ledger(&user),
        before,
        "reading the harness status leaves the human's home byte-identical"
    );
}

fn ledger(root: &std::path::Path) -> Vec<(PathBuf, u64, u64)> {
    let mut entries = Vec::new();
    if !root.exists() {
        return entries;
    }
    for entry in walkdir_simple(root) {
        let metadata = std::fs::symlink_metadata(&entry).expect("the entry stats");
        entries.push((
            entry,
            metadata.len(),
            metadata
                .modified()
                .expect("the mtime reads")
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the mtime is after the epoch")
                .as_nanos() as u64,
        ));
    }
    entries.sort();
    entries
}

fn walkdir_simple(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(next) = stack.pop() {
        out.push(next.clone());
        if next.is_dir()
            && let Ok(children) = std::fs::read_dir(&next)
        {
            for child in children.flatten() {
                stack.push(child.path());
            }
        }
    }
    out
}
