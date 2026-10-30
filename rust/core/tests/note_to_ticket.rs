//! `add_from_note` (NoteToTicket.swift), ported from `NoteToTicketTests.swift`: a
//! selection in a note becomes a ticket, and the ticket points back at the note.
//!
//! The rules under test: the title is the selection's first line (trimmed, and
//! without a checkbox's syntax), the description is the selection, the link is the
//! note's bikeshed-relative path, an empty selection is rejected, and the note
//! itself is never touched.

mod support;

use std::fs;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, DomainError, Filter};

fn add(
    board: &Board,
    selection: &str,
    path: &str,
) -> Result<tandem_core::domain::Ticket, DomainError> {
    board.add_from_note(selection, path, AddTicket::default())
}

#[test]
fn a_selection_becomes_a_ticket_the_first_line_is_the_title_the_selection_the_description() {
    let sandbox = Sandbox::new("note-first");
    let board = sandbox.board();
    let added = add(
        &board,
        "Ship the bikeshed\n\nIt is late, but it is the last thing.",
        "projects/App.md",
    )
    .unwrap();

    assert_eq!(added.title, "Ship the bikeshed");
    assert_eq!(
        added.description.as_deref(),
        Some("Ship the bikeshed\n\nIt is late, but it is the last thing.")
    );
    assert_eq!(added.link.as_deref(), Some("projects/App.md"));
    // Born in the default lane, in backlog, filed by the human.
    assert_eq!(added.lane, None);
    assert_eq!(added.status, tandem_core::domain::Status::Backlog);
    assert_eq!(added.origin, "human");
}

#[test]
fn the_title_is_trimmed() {
    let sandbox = Sandbox::new("note-trim");
    let board = sandbox.board();
    let added = add(
        &board,
        "   Fix the flaky test   \nmore detail",
        "notes/Ideas.md",
    )
    .unwrap();
    assert_eq!(added.title, "Fix the flaky test");
}

#[test]
fn a_selection_that_starts_with_a_blank_line_is_titled_after_its_first_real_line() {
    let sandbox = Sandbox::new("note-blank");
    let board = sandbox.board();
    let added = add(&board, "\n\nWrite the ADR", "notes/Ideas.md").unwrap();
    assert_eq!(added.title, "Write the ADR");
}

#[test]
fn a_checkbox_line_is_titled_with_its_text_not_its_syntax() {
    let sandbox = Sandbox::new("note-checkbox");
    let board = sandbox.board();
    for marker in ["- [ ] ", "- [x] ", "* [ ] ", "+ [ ] ", "1. [ ] "] {
        let selection = format!("{marker}Fix the flaky test");
        let added = add(&board, &selection, "notes/Ideas.md").unwrap();
        assert_eq!(added.title, "Fix the flaky test", "marker: {marker}");
        // The description keeps the line as written: the note's syntax stays in
        // the note, and the ticket shows what was sent.
        assert_eq!(added.description.as_deref(), Some(selection.as_str()));
    }
}

#[test]
fn a_line_that_is_not_a_task_item_keeps_its_text() {
    let sandbox = Sandbox::new("note-plain");
    let board = sandbox.board();
    let dash = add(&board, "- not a task", "notes/Ideas.md").unwrap();
    assert_eq!(dash.title, "- not a task");
    let bracket = add(&board, "[draft] not a task", "notes/Ideas.md").unwrap();
    assert_eq!(bracket.title, "[draft] not a task");
}

#[test]
fn an_empty_or_whitespace_only_selection_is_rejected() {
    let sandbox = Sandbox::new("note-empty");
    let board = sandbox.board();
    for selection in ["", "   ", "\n\n", " \t "] {
        assert!(
            matches!(
                add(&board, selection, "notes/Ideas.md"),
                Err(DomainError::EmptySelection)
            ),
            "selection {selection:?}"
        );
    }
    // Nothing was filed.
    assert!(board.list_filter(Filter::All).unwrap().is_empty());
}

#[test]
fn a_note_path_that_is_not_bikeshed_relative_is_rejected() {
    let sandbox = Sandbox::new("note-path");
    let board = sandbox.board();
    for path in [
        "",
        "  ",
        "/notes/Ideas.md",
        "../escape.md",
        "notes//Ideas.md",
    ] {
        assert!(
            matches!(
                add(&board, "A title", path),
                Err(DomainError::InvalidNotePath(_))
            ),
            "path {path:?}"
        );
    }
    assert!(board.list_filter(Filter::All).unwrap().is_empty());
}

#[test]
fn the_link_is_the_bikeshed_relative_path_and_it_persists() {
    let sandbox = Sandbox::new("note-link");
    let board = sandbox.board();
    let added = add(&board, "Ship it", "projects/tandem/App.md").unwrap();
    assert_eq!(added.link.as_deref(), Some("projects/tandem/App.md"));

    // Re-read: the link is on disk, not just in the returned value.
    let persisted = support::all(&board)
        .into_iter()
        .find(|ticket| ticket.id == added.id)
        .unwrap();
    assert_eq!(persisted.link.as_deref(), Some("projects/tandem/App.md"));
    assert_eq!(persisted.description.as_deref(), Some("Ship it"));
}

#[test]
fn the_ticket_lands_on_top_of_the_default_lane_in_backlog() {
    let sandbox = Sandbox::new("note-top");
    let board = sandbox.board();
    board.add("An older ticket", AddTicket::default()).unwrap();
    let added = add(&board, "A newer one", "notes/Ideas.md").unwrap();

    let tickets = support::all(&board);
    assert_eq!(tickets.first().unwrap().id, added.id);
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["A newer one", "An older ticket"]
    );
}

#[test]
fn the_ticket_files_into_the_lane_it_is_given_and_an_unknown_lane_is_rejected() {
    let sandbox = Sandbox::new("note-lane");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    let added = board
        .add_from_note(
            "A lane ticket",
            "notes/Ideas.md",
            AddTicket {
                lane: Some("Tandem"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(added.lane.as_deref(), Some("Tandem"));

    assert!(matches!(
        board.add_from_note(
            "A lane ticket",
            "notes/Ideas.md",
            AddTicket {
                lane: Some("Nope"),
                ..AddTicket::default()
            }
        ),
        Err(DomainError::UnknownLane(_))
    ));
}

#[test]
fn the_note_is_not_modified_the_files_bytes_are_untouched() {
    let sandbox = Sandbox::new("note-untouched");
    let board = sandbox.board();
    let note = sandbox
        .root()
        .join("bikeshed")
        .join("projects")
        .join("App.md");
    fs::create_dir_all(note.parent().unwrap()).unwrap();
    let text = "- [ ] Ship the bikeshed\n\nSome prose.\n";
    fs::write(&note, text).unwrap();

    add(&board, "Ship the bikeshed", "projects/App.md").unwrap();

    assert_eq!(fs::read_to_string(&note).unwrap(), text);
    assert_eq!(
        fs::read(&note).unwrap(),
        text.as_bytes(),
        "the bytes, not only the text"
    );
}
