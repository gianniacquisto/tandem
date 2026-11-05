//! The human hold: a question assigned to the human closes via a comment plus
//! `set`, and the inbox is the assignee alone.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Edit, SetFields, Status};

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
fn set_clears_a_human_owner_freely() {
    let (_sandbox, board, id) = human_hold();
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
    assert_eq!(cleared.description.as_deref(), Some(BRIEF));
}

#[test]
fn set_hands_a_human_hold_to_another_owner_freely() {
    let (_sandbox, board, id) = human_hold();
    for next in [Some("builder"), Some(""), Some("   ")] {
        let moved = board
            .set(
                &id.to_string(),
                SetFields {
                    assignee: Edit::Set(next),
                    ..SetFields::default()
                },
            )
            .unwrap();
        let expected = match next {
            Some(raw) if !raw.trim().is_empty() => Some(raw.trim().to_string()),
            _ => None,
        };
        assert_eq!(moved.assignee, expected, "next owner {next:?}");
        board
            .set(
                &id.to_string(),
                SetFields {
                    assignee: Edit::Set(Some("human")),
                    ..SetFields::default()
                },
            )
            .unwrap();
    }
}

#[test]
fn the_hold_closes_via_a_comment_plus_a_set() {
    let (_sandbox, board, id) = human_hold();
    board
        .append_comment(&id.to_string(), "builder", "A: the first option.")
        .unwrap();
    let moved = board
        .set(
            &id.to_string(),
            SetFields {
                status: Edit::Set(Status::Ready),
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(moved.assignee, None, "the hold is closed");
    assert_eq!(moved.status, Status::Ready);
    let comments = board.comments(&id.to_string()).unwrap();
    assert!(
        comments.iter().any(|c| c.body.contains("first option")),
        "the decision lives in the comment log"
    );
}

#[test]
fn a_done_ticket_clears_its_owner() {
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
