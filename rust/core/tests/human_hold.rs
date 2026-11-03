//! The human hold (TAN-279): `set` cannot move the owner away from `human`
//! while the ticket asks an unanswered question.
//!
//! The hold closes only through `ticket answer`, which writes the `## Answer`
//! block and the new owner in one write. A same-call description that adds the
//! block does not exempt the write: the guard reads the stored ticket. A done
//! ticket is exempt: finishing the work already settled the question.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Edit, SetFields, Status};

const BRIEF: &str = "## Why\n\nWhich option?\n\n## What\n\nPick one.";

fn human_hold() -> (Sandbox, tandem_core::domain::Board, uuid::Uuid) {
    let sandbox = Sandbox::new("human-hold");
    let board = sandbox.board();
    let id = board.add("Which option?", AddTicket::default()).unwrap().id;
    board
        .set(
            &id.to_string(),
            SetFields {
                status: Edit::Set(Status::OnHold),
                assignee: Edit::Set(Some("human")),
                description: Edit::Set(Some(BRIEF)),
                ..SetFields::default()
            },
        )
        .unwrap();
    (sandbox, board, id)
}

#[test]
fn set_cannot_clear_a_human_owner_without_an_answer() {
    let (_sandbox, board, id) = human_hold();
    let error = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap_err();
    assert!(
        error.message().contains("ticket answer"),
        "the refusal names the one write that closes the hold: {}",
        error.message()
    );

    let shown = board.ticket(&id.to_string()).unwrap();
    assert_eq!(
        shown.assignee.as_deref(),
        Some("human"),
        "a refused clear writes nothing"
    );
    assert_eq!(shown.description.as_deref(), Some(BRIEF));
}

#[test]
fn set_cannot_hand_a_human_hold_to_another_owner_without_an_answer() {
    let (_sandbox, board, id) = human_hold();
    for next in [Some("builder"), Some(""), Some("   ")] {
        let error = board
            .set(
                &id.to_string(),
                SetFields {
                    assignee: Edit::Set(next),
                    ..SetFields::default()
                },
            )
            .unwrap_err();
        assert!(
            error.message().contains("ticket answer"),
            "next owner {next:?}: {}",
            error.message()
        );
    }
    assert_eq!(
        board.ticket(&id.to_string()).unwrap().assignee.as_deref(),
        Some("human")
    );
}

#[test]
fn a_same_call_description_adding_an_answer_does_not_exempt_the_write() {
    let (_sandbox, board, id) = human_hold();
    let error = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                description: Edit::Set(Some("## Why\n\nWhich?\n\n## Answer\n\nA.")),
                ..SetFields::default()
            },
        )
        .unwrap_err();
    assert!(
        error.message().contains("ticket answer"),
        "the guard reads the stored ticket, not the same-call description: {}",
        error.message()
    );
    assert_eq!(
        board.ticket(&id.to_string()).unwrap().assignee.as_deref(),
        Some("human")
    );
}

#[test]
fn an_answered_ticket_may_clear_its_owner() {
    let (_sandbox, board, id) = human_hold();
    board
        .answer(
            &id.to_string(),
            Status::OnHold,
            Some("human"),
            "A.",
            1_762_000_000,
        )
        .unwrap();

    let cleared = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.assignee, None);
}

#[test]
fn a_done_ticket_may_clear_its_owner_without_an_answer() {
    let (_sandbox, board, id) = human_hold();
    board
        .set(
            &id.to_string(),
            SetFields {
                status: Edit::Set(Status::Done),
                ..SetFields::default()
            },
        )
        .unwrap();

    let cleared = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.assignee, None);
    assert_eq!(cleared.status, Status::Done);
}

#[test]
fn answer_closes_the_hold_in_one_write() {
    let (_sandbox, board, id) = human_hold();
    let answered = board
        .answer(
            &id.to_string(),
            Status::Ready,
            None,
            "A: the first option.",
            1_762_000_000,
        )
        .unwrap();
    assert_eq!(answered.assignee, None, "the hold is closed");
    assert_eq!(answered.status, Status::Ready);
    assert!(
        tandem_core::domain::DescriptionBody::answered(answered.description.as_deref()),
        "the block the guard reads is the one the answer wrote"
    );
    assert!(!matches!(
        board.ticket(&id.to_string()).unwrap().assignee.as_deref(),
        Some("human")
    ));
}

#[test]
fn the_refusal_is_a_domain_failure_not_an_unknown_id() {
    let (_sandbox, board, id) = human_hold();
    let error = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap_err();
    assert!(
        !matches!(error, DomainError::UnknownId { .. }),
        "the ticket exists: {error:?}"
    );
}
