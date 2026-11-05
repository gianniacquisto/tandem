//! The stdin description body and the section the board reads out of a
//! description.
//!
//! The store's rule is unchanged: an empty description is a legitimate value, and
//! `--description ""` clears it deliberately. The refusal belongs to the stdin path
//! alone. There is no answer marker: a hold closes via a comment plus `set`, and
//! the inbox is the assignee alone.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DescriptionBody, DomainError, Edit, Filter, SetFields};

#[test]
fn an_empty_body_is_refused() {
    assert_eq!(
        DescriptionBody::required(""),
        Err(DomainError::EmptyDescriptionBody)
    );
}

#[test]
fn a_whitespace_only_body_is_refused() {
    for body in ["   ", "\t", "\n", " \t\n "] {
        assert_eq!(
            DescriptionBody::required(body),
            Err(DomainError::EmptyDescriptionBody),
            "body: {body:?}"
        );
    }
}

#[test]
fn a_body_with_text_is_kept_trimmed_at_its_edges() {
    assert_eq!(
        DescriptionBody::required("  the brief  ").unwrap(),
        "the brief"
    );
    assert_eq!(
        DescriptionBody::required("line one\nline two").unwrap(),
        "line one\nline two"
    );
}

#[test]
fn the_refusal_is_a_domain_failure_with_actionable_text() {
    let text = DomainError::EmptyDescriptionBody.message();
    assert!(text.contains("empty"));
    assert!(text.contains("--description"));
}

#[test]
fn the_store_still_takes_an_empty_description_as_a_deliberate_clear() {
    let sandbox = Sandbox::new("description-body");
    let board = sandbox.board();
    let ticket = board.add("A ticket", AddTicket::default()).unwrap();
    board
        .set(
            &ticket.id.to_string(),
            SetFields {
                description: Edit::Set(Some("the brief")),
                ..SetFields::default()
            },
        )
        .unwrap();
    let cleared = board
        .set(
            &ticket.id.to_string(),
            SetFields {
                description: Edit::Set(Some("")),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.description, None);
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);
}
