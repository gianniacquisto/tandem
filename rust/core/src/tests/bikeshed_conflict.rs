//! Ported from `BikeshedConflictTests`: `read`'s hash reflects the on-disk state,
//! a `WriteRecord` hash matches the on-disk read (self-write dedup), and the hash
//! moves when the file changes underneath.

use std::time::{Duration, SystemTime};

use super::bikeshed_support::{Vault, sha256};

#[test]
fn the_read_hash_is_the_sha256_of_the_on_disk_bytes() {
    let vault = Vault::new("conflict-stable");
    vault.stage("note.md", "hello bikeshed");
    let bikeshed = vault.bikeshed();

    let first = bikeshed.read("note.md").unwrap().value;
    let second = bikeshed.read("note.md").unwrap().value;
    assert_eq!(first.hash, sha256("hello bikeshed"));
    assert_eq!(first.hash, second.hash);
    assert_eq!(first.text, "hello bikeshed");
}

#[test]
fn the_write_record_hash_equals_the_on_disk_read() {
    let vault = Vault::new("conflict-write");
    let bikeshed = vault.bikeshed();
    let record = bikeshed
        .write("note.md", "saved bytes", None)
        .unwrap()
        .value;
    let on_disk = bikeshed.read("note.md").unwrap().value;

    assert_eq!(record.path, "note.md");
    assert_eq!(record.hash, on_disk.hash);
    assert_eq!(record.hash, sha256("saved bytes"));
}

#[test]
fn an_external_change_moves_the_hash() {
    let vault = Vault::new("conflict-external");
    vault.stage("note.md", "original");
    let bikeshed = vault.bikeshed();
    let before = bikeshed.read("note.md").unwrap().value;

    // An agent or editor writes the file directly: no lock, no API.
    vault.stage("note.md", "external edit");

    let after = bikeshed.read("note.md").unwrap().value;
    assert_ne!(after.hash, before.hash);
    assert_eq!(after.hash, sha256("external edit"));
    assert_eq!(after.text, "external edit");
}

#[test]
fn the_hash_is_content_based() {
    let vault = Vault::new("conflict-content");
    vault.stage("a.md", "same content");
    vault.stage("b.md", "same content");
    let bikeshed = vault.bikeshed();
    assert_eq!(
        bikeshed.read("a.md").unwrap().value.hash,
        bikeshed.read("b.md").unwrap().value.hash
    );
}

#[test]
fn a_created_note_hashes_the_empty_byte_string() {
    let vault = Vault::new("conflict-empty");
    let bikeshed = vault.bikeshed();
    bikeshed.create("note.md").unwrap();
    let content = bikeshed.read("note.md").unwrap().value;
    assert_eq!(content.hash, sha256(""));
    assert!(content.text.is_empty());
}

#[test]
fn the_read_mtime_tracks_the_on_disk_modification_time() {
    let vault = Vault::new("conflict-mtime");
    let bikeshed = vault.bikeshed();
    bikeshed.write("note.md", "one", None).unwrap();
    let first = bikeshed.read("note.md").unwrap().value;

    // Age the file by five minutes, then rewrite it: the mtime must move.
    vault.set_modified("note.md", SystemTime::now() - Duration::from_secs(300));
    bikeshed.write("note.md", "two", None).unwrap();
    let second = bikeshed.read("note.md").unwrap().value;

    assert!(second.modified_at > first.modified_at);
    assert!(second.modified_at > SystemTime::now() - Duration::from_secs(30));
}

// MARK: - The expected-hash rule (MCP-5)

use std::sync::{Arc, Barrier};
use std::thread;

use crate::bikeshed::ABSENT_HASH;
use crate::domain::DomainError;

#[test]
fn a_write_with_the_old_hash_after_a_change_on_disk_is_refused_and_the_disk_is_intact() {
    let vault = Vault::new("conflict-stale");
    vault.stage("note.md", "first");
    let bikeshed = vault.bikeshed();

    let read = bikeshed.read("note.md").unwrap().value;
    vault.stage("note.md", "the human's edit");

    let error = bikeshed
        .write("note.md", "my stale write", Some(&read.hash))
        .unwrap_err();
    assert_eq!(error, DomainError::StaleNote("note.md".to_string()));
    let message = error.message();
    assert!(message.contains("note.md"), "names the path: {message}");
    assert!(
        message.contains("Read it again") || message.contains("note_read"),
        "says what to do next: {message}"
    );
    assert_eq!(vault.read("note.md"), "the human's edit");
}

#[test]
fn a_write_with_the_current_hash_lands_and_none_overwrites() {
    let vault = Vault::new("conflict-fresh");
    vault.stage("note.md", "first");
    let bikeshed = vault.bikeshed();

    let read = bikeshed.read("note.md").unwrap().value;
    let record = bikeshed
        .write("note.md", "second", Some(&read.hash))
        .unwrap()
        .value;
    assert_eq!(vault.read("note.md"), "second");

    vault.stage("note.md", "changed again");
    bikeshed.write("note.md", "keep mine", None).unwrap();
    assert_eq!(vault.read("note.md"), "keep mine");
    assert_ne!(record.hash, sha256("keep mine"));
}

#[test]
fn the_absent_sentinel_creates_only_when_no_file_is_there() {
    let vault = Vault::new("conflict-absent");
    let bikeshed = vault.bikeshed();

    bikeshed
        .write("new.md", "created", Some(ABSENT_HASH))
        .unwrap();
    assert_eq!(vault.read("new.md"), "created");

    let error = bikeshed
        .write("new.md", "again", Some(ABSENT_HASH))
        .unwrap_err();
    assert_eq!(error, DomainError::StaleNote("new.md".to_string()));
    assert_eq!(vault.read("new.md"), "created");

    let error = bikeshed
        .write("other.md", "x", Some("deadbeef"))
        .unwrap_err();
    assert_eq!(error, DomainError::StaleNote("other.md".to_string()));
}

#[test]
fn two_writers_with_the_same_expected_hash_cannot_both_succeed() {
    for round in 0..20 {
        let vault = Vault::new(&format!("conflict-race-{round}"));
        vault.stage("note.md", "base");
        let bikeshed = Arc::new(vault.bikeshed());
        let expected = bikeshed.read("note.md").unwrap().value.hash;
        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = ["X", "Y"]
            .into_iter()
            .map(|tag| {
                let bikeshed = Arc::clone(&bikeshed);
                let barrier = Arc::clone(&barrier);
                let expected = expected.clone();
                thread::spawn(move || {
                    barrier.wait();
                    bikeshed.write("note.md", tag, Some(&expected))
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let wins = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(wins, 1, "exactly one writer wins: {results:?}");
        let loser = results.iter().find_map(|r| r.as_ref().err()).unwrap();
        assert_eq!(loser, &DomainError::StaleNote("note.md".to_string()));
        let on_disk = vault.read("note.md");
        assert!(on_disk == "X" || on_disk == "Y", "whole text: {on_disk}");
    }
}
