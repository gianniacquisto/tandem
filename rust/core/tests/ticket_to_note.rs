//! Promoting a ticket to a bikeshed note ([`TicketToNote`]), ported from
//! `TicketToNoteTests.swift`: the note is made from the ticket's title and
//! description, the ticket's `link` points at it, and nothing else about the ticket
//! changes. The note is created, never overwritten; a ticket that already links to a
//! note is refused.
//!
//! The bikeshed lands behind [`Notes`] (PRT-7), so this suite stages notes in an
//! in-memory writer with the same rule the bikeshed's `writeNew` keeps (`-2`, `-3`,
//! ... beside a taken name). The collision arithmetic itself is PRT-7's; what is
//! proved here is that a promotion asks for a new note and takes the path it is
//! given.

mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;

use support::Sandbox;
use tandem_core::domain::{
    AddTicket, Board, DomainError, Edit, Filter, Notes, SetFields, Status, TicketToNote,
};

/// The note writer the promotion talks to: the same create-never-overwrite rule the
/// bikeshed keeps, without a filesystem.
#[derive(Default)]
struct MemNotes {
    cells: RefCell<BTreeMap<String, String>>,
}

impl MemNotes {
    fn stage(&self, path: &str, text: &str) {
        self.cells
            .borrow_mut()
            .insert(path.to_string(), text.to_string());
    }

    fn read(&self, path: &str) -> String {
        self.cells
            .borrow()
            .get(path)
            .unwrap_or_else(|| panic!("no note at {path}"))
            .clone()
    }

    fn paths(&self) -> Vec<String> {
        self.cells.borrow().keys().cloned().collect()
    }
}

impl Notes for MemNotes {
    fn write_new(&self, path: &str, text: &str) -> Result<String, String> {
        let mut cells = self.cells.borrow_mut();
        let mut candidate = path.to_string();
        let mut suffix = 2;
        while cells.contains_key(&candidate) {
            candidate = suffixed(path, suffix);
            suffix += 1;
        }
        cells.insert(candidate.clone(), text.to_string());
        Ok(candidate)
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        self.cells.borrow_mut().remove(path);
        Ok(())
    }
}

/// `projects/tandem/foo.md` becomes `projects/tandem/foo-2.md`.
fn suffixed(path: &str, n: usize) -> String {
    match path.strip_suffix(".md") {
        Some(stem) => format!("{stem}-{n}.md"),
        None => format!("{path}-{n}"),
    }
}

fn promote(
    board: &Board,
    notes: &MemNotes,
    id: &str,
) -> Result<tandem_core::domain::PromotedNote, DomainError> {
    TicketToNote::new(board, notes).promote(id)
}

#[test]
fn promotion_writes_the_note_links_the_ticket_and_changes_nothing_else() {
    let sandbox = Sandbox::new("promote");
    let board = sandbox.board();
    let notes = MemNotes::default();
    board.add_lane("Cycle", None).unwrap();
    let added = board
        .add(
            "Ship the bikeshed",
            AddTicket {
                lane: Some("Cycle"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = added.id.to_string();
    board
        .set(
            &id,
            SetFields {
                description: Edit::Set(Some("The note body.")),
                status: Edit::Set(Status::InProgress),
                ..SetFields::default()
            },
        )
        .unwrap();

    let promoted = promote(&board, &notes, &id).unwrap();

    // The note lands under the lane, named after the title.
    assert_eq!(promoted.path, "projects/cycle/ship-the-bikeshed.md");

    // Its text: the title as the heading, the description as the body. The ticket's
    // own state is not written into the note.
    assert_eq!(
        notes.read(&promoted.path),
        "# Ship the bikeshed\n\nThe note body.\n"
    );

    // The ticket, as the board now holds it: linked, and otherwise untouched.
    let stored = board
        .list_filter(Filter::All)
        .unwrap()
        .into_iter()
        .find(|ticket| ticket.id == added.id)
        .unwrap();
    assert_eq!(
        stored.link.as_deref(),
        Some("projects/cycle/ship-the-bikeshed.md")
    );
    assert_eq!(stored.title, "Ship the bikeshed");
    assert_eq!(stored.description.as_deref(), Some("The note body."));
    assert_eq!(stored.status, Status::InProgress);
    assert_eq!(stored.lane.as_deref(), Some("Cycle"));
}

#[test]
fn a_ticket_with_no_lane_lands_in_projects_uncategorized() {
    let sandbox = Sandbox::new("promote-default");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let added = board.add("No lane here", AddTicket::default()).unwrap();

    let promoted = promote(&board, &notes, &added.id.to_string()).unwrap();

    assert_eq!(promoted.path, "projects/uncategorized/no-lane-here.md");
    assert_eq!(notes.read(&promoted.path), "# No lane here\n");
}

#[test]
fn the_notes_name_is_a_slug_of_the_title() {
    let sandbox = Sandbox::new("promote-slug");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let added = board
        .add("Fix the / thing: v2!", AddTicket::default())
        .unwrap();

    let promoted = promote(&board, &notes, &added.id.to_string()).unwrap();

    assert_eq!(promoted.path, "projects/uncategorized/fix-the-thing-v2.md");
}

#[test]
fn a_taken_name_gets_a_minus_two_suffix_and_the_first_note_is_untouched() {
    let sandbox = Sandbox::new("promote-collision");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let first = board.add("Same title", AddTicket::default()).unwrap();
    let second = board.add("Same title", AddTicket::default()).unwrap();

    let one = promote(&board, &notes, &first.id.to_string()).unwrap();
    let two = promote(&board, &notes, &second.id.to_string()).unwrap();

    assert_eq!(one.path, "projects/uncategorized/same-title.md");
    assert_eq!(two.path, "projects/uncategorized/same-title-2.md");
    assert_eq!(notes.read(&one.path), "# Same title\n");
    assert_eq!(notes.read(&two.path), "# Same title\n");
    assert_eq!(notes.paths().len(), 2);
}

#[test]
fn a_note_the_human_wrote_by_hand_is_never_overwritten() {
    let sandbox = Sandbox::new("promote-hand");
    let board = sandbox.board();
    let notes = MemNotes::default();
    notes.stage(
        "projects/uncategorized/ship-it.md",
        "# Ship it\n\nMy own prose.\n",
    );
    let added = board.add("Ship it", AddTicket::default()).unwrap();

    let promoted = promote(&board, &notes, &added.id.to_string()).unwrap();

    // The promotion filed itself beside the note, not over it.
    assert_eq!(promoted.path, "projects/uncategorized/ship-it-2.md");
    assert_eq!(
        notes.read("projects/uncategorized/ship-it.md"),
        "# Ship it\n\nMy own prose.\n"
    );
    assert_eq!(notes.read(&promoted.path), "# Ship it\n");
}

#[test]
fn promoting_a_linked_ticket_again_is_refused_and_the_note_is_left_alone() {
    let sandbox = Sandbox::new("promote-refuse");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let added = board.add("Write the ADR", AddTicket::default()).unwrap();
    let id = added.id.to_string();
    let first = promote(&board, &notes, &id).unwrap();
    let before = notes.read(&first.path);

    assert_eq!(
        promote(&board, &notes, &id),
        Err(DomainError::AlreadyLinked {
            path: first.path.clone()
        })
    );

    // Nothing was written: the note is the same, and no second note appeared.
    assert_eq!(notes.read(&first.path), before);
    assert_eq!(notes.paths().len(), 1);
    assert_eq!(
        board.list_filter(Filter::All).unwrap()[0].link.as_deref(),
        Some(first.path.as_str())
    );
}

#[test]
fn the_refusal_names_the_note_the_ticket_already_points_at() {
    let sandbox = Sandbox::new("promote-message");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let added = board.add("Write the ADR", AddTicket::default()).unwrap();
    let id = added.id.to_string();
    let first = promote(&board, &notes, &id).unwrap();

    let error = promote(&board, &notes, &id).unwrap_err();
    assert!(error.message().contains(&first.path));
}

#[test]
fn the_tickets_link_reads_the_note_back() {
    let sandbox = Sandbox::new("promote-roundtrip");
    let board = sandbox.board();
    let notes = MemNotes::default();
    let added = board.add("Round trip", AddTicket::default()).unwrap();
    let id = added.id.to_string();
    board
        .set(
            &id,
            SetFields {
                description: Edit::Set(Some("Body.")),
                ..SetFields::default()
            },
        )
        .unwrap();
    promote(&board, &notes, &id).unwrap();

    let link = board
        .list_filter(Filter::All)
        .unwrap()
        .into_iter()
        .find(|ticket| ticket.id == added.id)
        .and_then(|ticket| ticket.link)
        .expect("the ticket is linked");
    assert_eq!(notes.read(&link), "# Round trip\n\nBody.\n");
}

#[test]
fn promoting_an_unknown_id_is_the_unknown_id_failure() {
    let sandbox = Sandbox::new("promote-unknown");
    let board = sandbox.board();
    let notes = MemNotes::default();
    board.add("A ticket", AddTicket::default()).unwrap();

    for id in [uuid::Uuid::new_v4().to_string(), "not-a-uuid".to_string()] {
        assert!(matches!(
            promote(&board, &notes, &id),
            Err(DomainError::UnknownId { .. })
        ));
    }
    assert!(notes.paths().is_empty());
}

#[test]
fn the_path_and_text_rules_are_the_documents_own() {
    assert_eq!(
        TicketToNote::path(Some("Cycle"), "Ship the bikeshed"),
        "projects/cycle/ship-the-bikeshed.md"
    );
    assert_eq!(
        TicketToNote::path(None, "No lane here"),
        "projects/uncategorized/no-lane-here.md"
    );
    assert_eq!(
        TicketToNote::path(Some("!!!"), "!!!"),
        "projects/uncategorized/untitled.md",
        "a name of only punctuation has no slug"
    );
    assert_eq!(
        TicketToNote::slug(&"a".repeat(80)).unwrap().len(),
        60,
        "a slug is capped at 60 characters"
    );
    assert_eq!(TicketToNote::text("Title", None), "# Title\n");
    assert_eq!(
        TicketToNote::text("Title\nsecond line", Some("Body.")),
        "# Title second line\n\nBody.\n"
    );
}
