//! `Notes::edit`: one occurrence replaced under the vault lock, refusals that
//! change nothing, and two edits to one note that both land (MCP-6).

use std::sync::{Arc, Barrier};
use std::thread;

use super::notes_support::{Vault, sha256};
use crate::domain::DomainError;

#[test]
fn one_match_is_replaced_and_the_record_carries_the_new_hash() {
    let vault = Vault::new("edit-one");
    vault.stage("note.md", "alpha\nbeta\ngamma\n");
    let record = vault
        .notes()
        .edit("note.md", "beta", "BETA", None)
        .unwrap()
        .value;
    assert_eq!(vault.read("note.md"), "alpha\nBETA\ngamma\n");
    assert_eq!(record.hash, sha256("alpha\nBETA\ngamma\n"));
}

#[test]
fn zero_and_two_matches_are_refused_and_the_bytes_stay() {
    let vault = Vault::new("edit-refuse");
    vault.stage("note.md", "one two two\n");
    let notes = vault.notes();
    let none = notes.edit("note.md", "three", "3", None).unwrap_err();
    assert_eq!(none, DomainError::EditNoMatch("note.md".to_string()));
    let many = notes.edit("note.md", "two", "2", None).unwrap_err();
    assert_eq!(
        many,
        DomainError::EditAmbiguous {
            path: "note.md".to_string(),
            count: 2
        }
    );
    assert_eq!(vault.read("note.md"), "one two two\n");
}

#[test]
fn an_empty_old_text_appends_once() {
    let vault = Vault::new("edit-append");
    vault.stage("note.md", "start\n");
    let notes = vault.notes();
    notes.edit("note.md", "", "end\n", None).unwrap();
    assert_eq!(vault.read("note.md"), "start\nend\n");
    let again = notes.edit("note.md", "", "end\n", None).unwrap().value;
    assert_eq!(vault.read("note.md"), "start\nend\n", "a retry lands once");
    assert_eq!(again.hash, sha256("start\nend\n"));
    assert_eq!(
        notes.edit("note.md", "", "", None).unwrap_err(),
        DomainError::EditEmpty
    );
}

#[test]
fn a_stale_hash_a_missing_note_and_an_escape_are_refused() {
    let vault = Vault::new("edit-guards");
    vault.stage("note.md", "text");
    let notes = vault.notes();
    assert_eq!(
        notes
            .edit("note.md", "text", "new", Some("deadbeef"))
            .unwrap_err(),
        DomainError::StaleNote("note.md".to_string())
    );
    assert!(notes.edit("missing.md", "a", "b", None).is_err());
    assert!(notes.edit("../escape.md", "a", "b", None).is_err());
    assert_eq!(vault.read("note.md"), "text");
}

#[test]
fn two_concurrent_edits_to_different_lines_both_land() {
    for round in 0..20 {
        let vault = Vault::new(&format!("edit-race-{round}"));
        vault.stage("note.md", "line one\nline two\n");
        let notes = Arc::new(vault.notes());
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [("line one", "FIRST"), ("line two", "SECOND")]
            .into_iter()
            .map(|(old, new)| {
                let notes = Arc::clone(&notes);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    notes.edit("note.md", old, new, None)
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(vault.read("note.md"), "FIRST\nSECOND\n", "round {round}");
    }
}
