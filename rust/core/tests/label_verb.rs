//! The `label` verb group's read, ported from `LabelVerbTests.swift`: `label list` is
//! the label discovery the board answers without pulling ticket rows. The write
//! verbs (`add`, `rename`, `delete`) each answer the list their write produced;
//! `list` answers the store's declared labels, empty ones included.
//!
//! The Swift suite drove the binary and asserted its JSON, stderr and exit code.
//! That half is PRT-9's (the tandem grammar and the agent verbs); what is ported
//! here is the contract the verb answers with, at the board seam it reads.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Filter};

#[test]
fn label_list_on_an_empty_store_is_an_empty_label_list() {
    let sandbox = Sandbox::new("label-verb-empty");
    let board = sandbox.board();
    assert_eq!(board.labels().unwrap(), Vec::<String>::new());
}

#[test]
fn label_list_prints_the_declared_labels_empty_labels_included_with_no_ticket_rows() {
    let sandbox = Sandbox::new("label-verb-list");
    let board = sandbox.board();
    board.add_label("Tandem", None, None).unwrap();
    board.add_label("Retro", None, None).unwrap();
    // A ticket in one label must not change what `list` reports: the labels come
    // from the store's label list, not from ticket rows.
    let ticket = board.add("In a label", AddTicket::default()).unwrap();
    board
        .move_ticket(&ticket.id.to_string(), Some("Tandem"), usize::MAX)
        .unwrap();

    assert_eq!(
        board.labels().unwrap(),
        ["Tandem", "Retro"],
        "render order, the empty label included"
    );
    // The read answers labels, not tickets: one row per label, and nothing else.
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);
}

#[test]
fn label_list_reports_a_label_a_hand_edit_leaves_on_a_ticket() {
    let sandbox = Sandbox::new("label-verb-dangling");
    let board = sandbox.board();
    board.add_label("Tandem", None, None).unwrap();
    let ticket = board
        .add(
            "in Tandem",
            AddTicket {
                label: Some("Tandem"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    // A rename is the only way a label name changes in the port; a name a ticket
    // carries that the list has dropped is still drawn, because otherwise a row
    // could be hidden by deleting a name.
    board
        .rename_label("Tandem", "Tandem Two", None, None)
        .unwrap();
    assert_eq!(board.labels().unwrap(), ["Tandem Two"]);
    assert_eq!(
        board
            .ticket(&ticket.id.to_string())
            .unwrap()
            .label
            .as_deref(),
        Some("Tandem Two")
    );
}

#[test]
fn every_label_write_answers_the_list_after_its_write() {
    let sandbox = Sandbox::new("label-verb-writes");
    let board = sandbox.board();
    assert_eq!(board.add_label("Tandem", None, None).unwrap(), ["Tandem"]);
    assert_eq!(
        board.rename_label("Tandem", "Board", None, None).unwrap(),
        ["Board"]
    );
    assert_eq!(board.move_label("Board", 0).unwrap(), ["Board"]);
    assert!(board.delete_label("Board").unwrap().labels.is_empty());
}

#[test]
fn unknown_label_names_the_label_and_the_fix() {
    let sandbox = Sandbox::new("label-verb-unknown");
    let board = sandbox.board();
    board.add_label("Tandem", None, None).unwrap();
    let refused = board.move_label("Nope", 0).unwrap_err();
    let message = refused.message();
    assert!(
        message.contains("\"Nope\""),
        "the refusal names the label: {message}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "the refusal names the fix: {message}"
    );

    // The epic labels use the same refusal: an unknown epic name names the
    // label and the fix at the domain seam.
    let epic = Sandbox::new("label-verb-unknown-epic");
    let epic_board = epic.board();
    epic_board.add_label("Tandem", None, None).unwrap();
    epic_board.add_label("ui", None, None).unwrap();
    epic_board.add_label("core", None, None).unwrap();
    let refused = epic_board.move_label("docs", 0).unwrap_err();
    let message = refused.message();
    assert!(
        message.contains("\"docs\""),
        "the refusal names the epic label: {message}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "the refusal names the fix: {message}"
    );
    let ticket = epic_board.add("epic work", AddTicket::default()).unwrap();
    let refused = epic_board
        .move_ticket(&ticket.id.to_string(), Some("docs"), usize::MAX)
        .unwrap_err();
    let message = refused.message();
    assert!(
        message.contains("\"docs\""),
        "filing into an unknown epic label names the label: {message}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "filing names the fix: {message}"
    );
}
