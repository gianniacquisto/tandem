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
use tandem_core::domain::{AddTicket, DomainError, Filter};

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
    board.add_label("Tandem", Some(support::PROJECT)).unwrap();
    board.add_label("Retro", Some(support::PROJECT)).unwrap();
    // A ticket in one label must not change what `list` reports: the labels come
    // from the store's label list, not from ticket rows.
    let ticket = board.add("In a label", support::ticket()).unwrap();
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
    board.add_label("Tandem", Some(support::PROJECT)).unwrap();
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
    board.rename_label("Tandem", "Tandem Two", None).unwrap();
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
    assert_eq!(
        board.add_label("Tandem", Some(support::PROJECT)).unwrap(),
        ["Tandem"]
    );
    assert_eq!(
        board.rename_label("Tandem", "Board", None).unwrap(),
        ["Board"]
    );
    assert_eq!(board.move_label("Board", 0).unwrap(), ["Board"]);
    assert!(board.delete_label("Board").unwrap().labels.is_empty());
}

#[test]
fn unknown_label_names_the_label_and_the_fix() {
    let sandbox = Sandbox::new("label-verb-unknown");
    let board = sandbox.board();
    board.add_label("Tandem", Some(support::PROJECT)).unwrap();
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
    epic_board
        .add_label("Tandem", Some(support::PROJECT))
        .unwrap();
    epic_board.add_label("ui", Some(support::PROJECT)).unwrap();
    epic_board
        .add_label("core", Some(support::PROJECT))
        .unwrap();
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
    let ticket = epic_board.add("epic work", support::ticket()).unwrap();
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

#[test]
fn label_delete_unlabels_its_tickets_and_keeps_their_refs() {
    let sandbox = Sandbox::new("label-verb-delete-refs");
    let board = sandbox.board();
    board.add_label("Epic", Some(support::PROJECT)).unwrap();
    let ticket = board
        .add(
            "in the epic",
            AddTicket {
                label: Some("Epic"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let reference = ticket.reference.clone().expect("a ref");

    let deletion = board.delete_label("Epic").unwrap();
    assert_eq!(deletion.moved, 1);
    let kept = board.ticket(&reference).unwrap();
    assert_eq!(kept.label, None, "the label is gone");
    assert_eq!(kept.project, support::PROJECT, "the project stays");
    assert_eq!(kept.reference.as_deref(), Some(reference.as_str()));
}

#[test]
fn label_rename_with_a_new_project_is_refused_while_the_label_holds_tickets_and_allowed_when_empty()
{
    let sandbox = Sandbox::new("label-verb-reparent");
    let board = sandbox.board();
    board.add_project("Other", None).unwrap();
    board.add_label("Full", Some(support::PROJECT)).unwrap();
    board.add_label("Empty", Some(support::PROJECT)).unwrap();
    board
        .add(
            "holds",
            AddTicket {
                label: Some("Full"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    match board
        .rename_label("Full", "Full", Some("Other"))
        .unwrap_err()
    {
        error @ DomainError::LabelHoldsTickets { .. } => {
            let message = error.message();
            assert!(
                message.contains("1 tickets")
                    && message.contains("Main")
                    && message.contains("Other"),
                "the refusal names the count and the two projects: {message}"
            );
        }
        other => panic!("expected labelHoldsTickets, got {other:?}"),
    }
    board
        .rename_label("Empty", "Empty", Some("Other"))
        .expect("an empty label may move");
    let projects = board.projects().unwrap();
    let other = projects.iter().find(|p| p.name == "Other").unwrap();
    assert_eq!(other.labels, ["Empty"]);
}

#[test]
fn label_add_without_a_project_is_refused_even_with_one_project_on_the_home() {
    let sandbox = Sandbox::new("label-verb-no-project");
    let board = sandbox.board();
    match board.add_label("Loose", None).unwrap_err() {
        error @ DomainError::MissingProject { .. } => {
            assert!(error.message().contains("Main"), "{}", error.message());
        }
        other => panic!("expected missingProject, got {other:?}"),
    }
}

#[test]
fn project_delete_is_refused_while_it_holds_a_ticket() {
    let sandbox = Sandbox::new("label-verb-project-delete");
    let board = sandbox.board();
    board.add("holds", support::ticket()).unwrap();
    match board.delete_project(support::PROJECT).unwrap_err() {
        error @ DomainError::ProjectNotEmpty { .. } => {
            assert!(error.message().contains("1 tickets"), "{}", error.message());
        }
        other => panic!("expected projectNotEmpty, got {other:?}"),
    }
}
