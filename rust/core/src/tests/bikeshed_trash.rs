//! The vault trash: a delete moves the note or folder under `.trash/`, a restore
//! moves it back through the same path rule as any note, and the next delete
//! past 30 days sweeps an entry out. The clock is injected through `delete_at`,
//! so the retention is pinned without sleeping.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::bikeshed::{Bikeshed, TrashEntry};
use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn at_day(days: u32) -> SystemTime {
    UNIX_EPOCH + DAY * days
}

fn trashed(entry: &TrashEntry) -> String {
    format!(".trash/{}", entry.id)
}

fn stamp_of(entry: &TrashEntry) -> String {
    entry.id.split('/').next().unwrap_or_default().to_string()
}

fn delete_at(bikeshed: &Bikeshed, path: &str, now: SystemTime) -> TrashEntry {
    bikeshed
        .delete_at(path, now)
        .expect("the note deletes")
        .value
}

#[test]
fn delete_moves_a_note_into_the_trash_and_out_of_every_view() {
    let vault = Vault::new("trash-note");
    let bikeshed = vault.bikeshed();
    vault.stage("projects/a.md", "# A\nthe body\n");

    let entry = delete_at(&bikeshed, "projects/a.md", at_day(100));

    assert_eq!(entry.path, "projects/a.md");
    assert!(!vault.exists("projects/a.md"), "the note leaves the vault");
    assert_eq!(
        vault.read(&trashed(&entry)),
        "# A\nthe body\n",
        "the bytes sit under .trash/ unchanged"
    );
    assert!(
        bikeshed.note_paths().is_empty(),
        "the trashed note is in no walk"
    );
    assert!(
        bikeshed
            .tree()
            .iter()
            .all(|node| !node.name.starts_with('.')),
        "the hidden trash is never drawn"
    );
    assert!(
        bikeshed
            .search("the body")
            .expect("the search answers")
            .value
            .is_empty(),
        "the trashed note is not searched"
    );
}

#[test]
fn delete_moves_a_folder_with_its_notes_byte_for_byte() {
    let vault = Vault::new("trash-folder");
    let bikeshed = vault.bikeshed();
    vault.stage("f/inner.md", "one");
    vault.stage("f/deep/x.md", "é\n");

    let entry = delete_at(&bikeshed, "f", at_day(100));

    assert!(!vault.exists("f"), "the folder leaves the vault");
    assert_eq!(vault.read(&format!("{}/inner.md", trashed(&entry))), "one");
    assert_eq!(vault.read(&format!("{}/deep/x.md", trashed(&entry))), "é\n");
    assert!(bikeshed.note_paths().is_empty());
}

#[test]
fn restore_brings_a_note_back_byte_for_byte_and_reopens_its_path() {
    let vault = Vault::new("trash-restore-note");
    let bikeshed = vault.bikeshed();
    vault.stage("projects/a.md", "bytes\r\nwith a tail é\n");
    let entry = delete_at(&bikeshed, "projects/a.md", at_day(100));

    let restored = bikeshed.restore(&entry.id).expect("the note restores");

    assert_eq!(restored.value, "projects/a.md");
    assert_eq!(vault.read("projects/a.md"), "bytes\r\nwith a tail é\n");
    assert!(
        !vault.exists(&trashed(&entry)),
        "the entry leaves the trash"
    );
    assert_eq!(bikeshed.note_paths(), vec!["projects/a.md".to_string()]);
}

#[test]
fn restore_brings_a_folder_back_with_its_notes() {
    let vault = Vault::new("trash-restore-folder");
    let bikeshed = vault.bikeshed();
    vault.stage("f/inner.md", "one");
    vault.stage("f/deep/x.md", "two");
    let entry = delete_at(&bikeshed, "f", at_day(100));

    bikeshed.restore(&entry.id).expect("the folder restores");

    assert_eq!(vault.read("f/inner.md"), "one");
    assert_eq!(vault.read("f/deep/x.md"), "two");
    assert!(!vault.exists(&trashed(&entry)));
}

#[test]
fn restore_onto_a_taken_path_is_refused_and_changes_nothing() {
    let vault = Vault::new("trash-restore-taken");
    let bikeshed = vault.bikeshed();
    vault.stage("a.md", "old");
    let entry = delete_at(&bikeshed, "a.md", at_day(100));
    vault.stage("a.md", "new");

    let error = bikeshed.restore(&entry.id).unwrap_err();

    assert_eq!(error, DomainError::NoteAlreadyExists("a.md".to_string()));
    assert_eq!(
        vault.read("a.md"),
        "new",
        "the note that took the path stays"
    );
    assert_eq!(
        vault.read(&trashed(&entry)),
        "old",
        "the trashed entry stays in the trash"
    );
}

#[test]
fn restore_refuses_any_id_that_names_a_path_outside_the_trash_or_the_vault() {
    let vault = Vault::new("trash-crafted");
    let bikeshed = vault.bikeshed();
    vault.stage("projects/a.md", "keep");
    let entry = delete_at(&bikeshed, "projects/a.md", at_day(100));
    let stamp = stamp_of(&entry);
    let outside = vault.parent().join("outside.md");
    fs::write(&outside, "outside").expect("the outside file is writable");

    let crafted = [
        String::new(),
        "/etc/passwd".to_string(),
        stamp.clone(),
        format!("{stamp}/"),
        format!("{stamp}/../../outside.md"),
        format!("{stamp}/../../../outside.md"),
        format!("{stamp}/projects/../../../outside.md"),
        format!("{stamp}/..\\..\\outside.md"),
        format!("{stamp}/C:outside.md"),
        format!("{stamp}//projects/a.md"),
        format!("{stamp}/.hidden.md"),
        format!("{stamp}/projects/.hidden/a.md"),
        format!("../{stamp}/projects/a.md"),
        "19700101T000000000Z/projects/a.md".to_string(),
        "not-a-stamp/projects/a.md".to_string(),
    ];
    for id in &crafted {
        assert!(
            bikeshed.restore(id).is_err(),
            "the id {id:?} must be refused"
        );
    }

    assert_eq!(
        fs::read_to_string(&outside).expect("the outside file reads"),
        "outside",
        "no crafted id reaches a file outside the vault"
    );
    assert!(!vault.exists("projects/a.md"), "nothing was restored");
    assert_eq!(vault.read(&trashed(&entry)), "keep");
}

#[cfg(unix)]
#[test]
fn restore_refuses_a_destination_that_runs_through_a_link() {
    let vault = Vault::new("trash-restore-link");
    let bikeshed = vault.bikeshed();
    vault.stage("escape-me.md", "stay");
    let entry = delete_at(&bikeshed, "escape-me.md", at_day(100));
    std::os::unix::fs::symlink(vault.parent(), vault.dir().join("link"))
        .expect("the link is creatable");
    let stamp = stamp_of(&entry);

    let result = bikeshed.restore(&format!("{stamp}/link/planted.md"));

    assert!(result.is_err(), "a link on the destination path is refused");
    assert!(
        !vault.parent().join("planted.md").exists(),
        "nothing lands through the link"
    );
}

#[test]
fn a_delete_sweeps_trash_entries_older_than_thirty_days_and_keeps_newer_ones() {
    let vault = Vault::new("trash-retention");
    let bikeshed = vault.bikeshed();
    vault.stage("old.md", "old");
    let old = delete_at(&bikeshed, "old.md", at_day(100));
    vault.stage("recent.md", "recent");
    let recent = delete_at(&bikeshed, "recent.md", at_day(120));

    // Exactly thirty days is not older than thirty days: the old entry survives.
    vault.stage("edge.md", "edge");
    delete_at(&bikeshed, "edge.md", at_day(130));
    assert!(
        vault.exists(&format!(".trash/{}", stamp_of(&old))),
        "an entry of exactly thirty days stays"
    );

    // Thirty-one days on, the next delete sweeps the old entry only.
    vault.stage("third.md", "third");
    delete_at(&bikeshed, "third.md", at_day(131));

    assert!(
        !vault.exists(&format!(".trash/{}", stamp_of(&old))),
        "an entry past thirty days is swept"
    );
    assert_eq!(
        vault.read(&trashed(&recent)),
        "recent",
        "a newer entry stays"
    );
}

#[test]
fn a_delete_leaves_links_alone_and_a_restore_brings_the_backlink_back() {
    let vault = Vault::new("trash-links");
    let bikeshed = vault.bikeshed();
    vault.stage("a.md", "# A");
    vault.stage("b.md", "see [[a]]");
    let entry = delete_at(&bikeshed, "a.md", at_day(100));

    assert_eq!(
        vault.read("b.md"),
        "see [[a]]",
        "the referencing note is not rewritten"
    );
    assert!(
        bikeshed.backlinks("a.md").is_err(),
        "the trashed target is unknown"
    );

    bikeshed.restore(&entry.id).expect("the note restores");
    let backlinks: Vec<String> = bikeshed
        .backlinks("a.md")
        .expect("the restored target answers")
        .value
        .into_iter()
        .map(|row| row.path)
        .collect();
    assert_eq!(backlinks, vec!["b.md".to_string()]);
}

#[test]
fn delete_refuses_a_path_inside_the_trash() {
    let vault = Vault::new("trash-delete-hidden");
    let bikeshed = vault.bikeshed();
    vault.stage("a.md", "keep");
    let entry = delete_at(&bikeshed, "a.md", at_day(100));

    let error = bikeshed.delete(&trashed(&entry)).unwrap_err();

    assert!(matches!(error, DomainError::InvalidName(_)));
    assert_eq!(
        vault.read(&trashed(&entry)),
        "keep",
        "the trash is untouched"
    );
}
