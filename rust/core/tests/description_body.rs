//! The stdin description body and the two sections the board reads out of a
//! description, ported from `DescriptionBodyTests.swift`.
//!
//! The store's rule is unchanged: an empty description is a legitimate value, and
//! `--description ""` clears it deliberately. The refusal belongs to the stdin path
//! alone.

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
fn a_line_that_starts_an_answer_heading_is_an_answer_any_case_any_dated_spelling() {
    for description in [
        "## Answer\nB: the chosen option",
        "## answer \u{2014} 2026-10-26\nA",
        "## ANSWER\nsettled",
        "## Answer (2026-10-21)\nA, approved by merging PR #59",
        "## Why\n\nwhy this is asked\n\n## Answer \u{2014} 2026-10-23\nA",
        "  ## Answer \u{2014} 2026-10-26\nindented, still a heading",
        "the brief\r\n\r\n## Answer \u{2014} 2026-10-26\r\nA",
    ] {
        assert!(
            DescriptionBody::answered(Some(description)),
            "not answered: {description}"
        );
    }
}

#[test]
fn a_ticket_with_no_answer_block_is_not_answered() {
    assert!(!DescriptionBody::answered(None));
    assert!(!DescriptionBody::answered(Some("")));
    assert!(!DescriptionBody::answered(Some("## Why\n\nno answer yet")));
    assert!(
        !DescriptionBody::answered(Some("text ## Answer, not a heading")),
        "the heading is line-anchored"
    );
    assert!(
        !DescriptionBody::answered(Some("The `## Answer` is recorded on `F1CEFA57`")),
        "a mention inline is not an answer"
    );
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
