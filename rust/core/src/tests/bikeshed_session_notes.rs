//! Ported from `BikeshedSessionNotesTests`: the notes a session wrote are the
//! captures filed under its folder, pending and archived alike, found by scanning
//! the files.

use std::fs;

use crate::bikeshed::CaptureKind;

use super::bikeshed_support::Vault;

fn archive(vault: &Vault, capture: &str, session: &str) {
    let source = vault.dir().join(capture);
    let done = vault
        .dir()
        .join(format!("projects/tandem/.raw/.done/{session}"));
    fs::create_dir_all(&done).unwrap();
    fs::rename(&source, done.join(source.file_name().unwrap())).unwrap();
}

#[test]
fn a_sessions_notes_are_the_captures_under_its_folder() {
    let vault = Vault::new("session-notes");
    let bikeshed = vault.bikeshed();
    let mine = bikeshed
        .capture(
            "tandem",
            CaptureKind::Fact,
            "# Mine\n",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap()
        .value;
    let theirs = bikeshed
        .capture(
            "tandem",
            CaptureKind::Fact,
            "# Theirs\n",
            "ses_2",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap()
        .value;
    bikeshed
        .capture(
            "fleet",
            CaptureKind::Gotcha,
            "# Elsewhere\n",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap();

    let notes = bikeshed.session_notes("ses_1").unwrap().value;
    assert!(notes.contains(&mine));
    assert!(!notes.contains(&theirs));
    assert_eq!(
        notes.len(),
        2,
        "one per project, both under the session folder"
    );
}

#[test]
fn a_consolidated_capture_is_still_the_sessions_note_at_its_archived_path() {
    let vault = Vault::new("session-notes-archive");
    let bikeshed = vault.bikeshed();
    let pending = bikeshed
        .capture(
            "tandem",
            CaptureKind::Fact,
            "# Kept\n",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap()
        .value;
    let consumed = bikeshed
        .capture(
            "tandem",
            CaptureKind::Decision,
            "# Folded\n",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap()
        .value;
    archive(&vault, &consumed, "ses_1");

    let archived = format!(
        "projects/tandem/.raw/.done/ses_1/{}",
        consumed.rsplit('/').next().unwrap()
    );
    let notes = bikeshed.session_notes("ses_1").unwrap().value;
    assert!(notes.contains(&pending));
    assert!(notes.contains(&archived));
    assert!(!notes.contains(&consumed));
    assert_eq!(notes.len(), 2);
}

#[test]
fn a_session_that_wrote_nothing_has_no_notes() {
    let vault = Vault::new("session-notes-empty");
    let bikeshed = vault.bikeshed();
    assert!(bikeshed.session_notes("ses_1").unwrap().value.is_empty());
    bikeshed
        .capture(
            "tandem",
            CaptureKind::Fact,
            "# Mine\n",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap();
    assert!(
        bikeshed
            .session_notes("ses_other")
            .unwrap()
            .value
            .is_empty()
    );
}

#[test]
fn a_missing_bikeshed_is_an_empty_list() {
    let vault = Vault::new("session-notes-missing");
    assert!(
        vault
            .bikeshed()
            .session_notes("ses_1")
            .unwrap()
            .value
            .is_empty()
    );
}
