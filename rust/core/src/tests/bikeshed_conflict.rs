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
    let record = bikeshed.write("note.md", "saved bytes").unwrap().value;
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
    bikeshed.write("note.md", "one").unwrap();
    let first = bikeshed.read("note.md").unwrap().value;

    // Age the file by five minutes, then rewrite it: the mtime must move.
    vault.set_modified("note.md", SystemTime::now() - Duration::from_secs(300));
    bikeshed.write("note.md", "two").unwrap();
    let second = bikeshed.read("note.md").unwrap().value;

    assert!(second.modified_at > first.modified_at);
    assert!(second.modified_at > SystemTime::now() - Duration::from_secs(30));
}
