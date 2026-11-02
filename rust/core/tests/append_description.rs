//! `ticket set --append-description`: an append that cannot wipe a brief (TAN-278).
//!
//! The store's rule is replace by default: `--description` overwrites. The
//! append is a different verb: one transaction, a blank line and the new text
//! after the existing body, bare text onto an empty body. Empty and
//! whitespace-only additions are refused, and naming both verbs in one call
//! is refused, leaving the description as it was.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Edit, Filter, SetFields};

fn append(board: &tandem_core::domain::Board, id: &str, text: &str) -> Result<String, DomainError> {
    let ticket = board.set(
        id,
        SetFields {
            append_description: Edit::Set(text),
            ..SetFields::default()
        },
    )?;
    Ok(ticket.description.unwrap_or_default())
}

#[test]
fn append_onto_an_empty_body_writes_bare_text() {
    let sandbox = Sandbox::new("append-empty");
    let board = sandbox.board();
    let ticket = board.add("A ticket", AddTicket::default()).unwrap();
    assert_eq!(ticket.description, None);

    let body = append(&board, &ticket.id.to_string(), "the addition").unwrap();
    assert_eq!(body, "the addition");
}

#[test]
fn append_onto_a_body_adds_a_blank_line_after_it() {
    let sandbox = Sandbox::new("append-blank-line");
    let board = sandbox.board();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    let body = append(&board, &ticket.id.to_string(), "the addition").unwrap();
    assert_eq!(body, "the brief\n\nthe addition");
}

#[test]
fn append_and_replace_differ() {
    let sandbox = Sandbox::new("append-vs-replace");
    let board = sandbox.board();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = ticket.id.to_string();

    let appended = append(&board, &id, "the addition").unwrap();
    assert!(
        appended.contains("the brief"),
        "an append keeps the brief: {appended}"
    );
    assert!(
        appended.contains("the addition"),
        "an append carries the addition: {appended}"
    );

    let replaced = board
        .set(
            &id,
            SetFields {
                description: Edit::Set(Some("only this")),
                ..SetFields::default()
            },
        )
        .unwrap();
    let replaced = replaced.description.unwrap_or_default();
    assert_eq!(
        replaced, "only this",
        "a replace overwrites where an append preserves"
    );
    assert!(
        !replaced.contains("the brief"),
        "a replace must fail this test if it ever preserves: {replaced}"
    );
}

#[test]
fn two_appends_leave_both_paragraphs() {
    let sandbox = Sandbox::new("append-twice");
    let board = sandbox.board();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = ticket.id.to_string();

    append(&board, &id, "first addition").unwrap();
    let body = append(&board, &id, "second addition").unwrap();

    assert!(
        body.contains("the brief"),
        "the brief survives two appends: {body}"
    );
    assert!(
        body.contains("first addition"),
        "the first addition survives the second: {body}"
    );
    assert!(
        body.contains("second addition"),
        "the second addition lands: {body}"
    );
    assert_eq!(body, "the brief\n\nfirst addition\n\nsecond addition");
}

#[test]
fn two_appends_from_two_boards_leave_both_paragraphs() {
    let sandbox = Sandbox::new("append-two-processes");
    let first = sandbox.board();
    let ticket = first
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = ticket.id.to_string();

    // Two processes are two stores over one home: the second reads after the
    // first commits, inside its own transaction, so no update is lost.
    let second = sandbox.board();
    append(&first, &id, "first addition").unwrap();
    let body = append(&second, &id, "second addition").unwrap();

    assert!(
        body.contains("first addition") && body.contains("second addition"),
        "two processes leave both paragraphs: {body}"
    );
}

#[test]
fn an_empty_or_whitespace_only_addition_is_refused_and_leaves_the_body() {
    let sandbox = Sandbox::new("append-empty-refused");
    let board = sandbox.board();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = ticket.id.to_string();

    for empty in ["", "   ", "\t", "\n", " \t\n "] {
        assert_eq!(
            board.set(
                &id,
                SetFields {
                    append_description: Edit::Set(empty),
                    ..SetFields::default()
                },
            ),
            Err(DomainError::EmptyDescriptionBody),
            "addition: {empty:?}"
        );
    }

    let reread = board.ticket(&id).unwrap();
    assert_eq!(
        reread.description.as_deref(),
        Some("the brief"),
        "a refused append writes nothing"
    );
}

#[test]
fn naming_replace_and_append_together_is_refused_and_leaves_the_body() {
    let sandbox = Sandbox::new("append-collision");
    let board = sandbox.board();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                description: Some("the brief"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let id = ticket.id.to_string();

    let refused = board.set(
        &id,
        SetFields {
            description: Edit::Set(Some("a replace")),
            append_description: Edit::Set("an addition"),
            ..SetFields::default()
        },
    );
    assert!(refused.is_err(), "naming both verbs in one call must fail");

    let reread = board.ticket(&id).unwrap();
    assert_eq!(
        reread.description.as_deref(),
        Some("the brief"),
        "a refused combination writes nothing"
    );
}

#[test]
fn the_append_trims_its_edges_like_a_body() {
    let sandbox = Sandbox::new("append-trim");
    let board = sandbox.board();
    let ticket = board.add("A ticket", AddTicket::default()).unwrap();

    let body = append(&board, &ticket.id.to_string(), "  the addition  \n").unwrap();
    assert_eq!(body, "the addition");
}

#[test]
fn the_list_still_reads_after_appends() {
    let sandbox = Sandbox::new("append-list");
    let board = sandbox.board();
    let ticket = board.add("A ticket", AddTicket::default()).unwrap();
    append(&board, &ticket.id.to_string(), "first").unwrap();
    append(&board, &ticket.id.to_string(), "second").unwrap();
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);
}
