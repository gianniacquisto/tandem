//! The answer verb is gone: a hold closes via a comment plus `set`, and the
//! inbox is the assignee alone.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Edit, SetFields, Status};

const BRIEF: &str = "## Why\n\nWhich option?\n\n## What\n\nPick one.";

fn human_hold() -> (Sandbox, tandem_core::domain::Board, uuid::Uuid) {
    let sandbox = Sandbox::new("answer-absence");
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
fn set_moves_a_human_owner_freely_with_no_answer_write() {
    let (_sandbox, board, id) = human_hold();
    let moved = board
        .set(
            &id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .expect("set moves a human owner freely: no answer write exists");
    assert_eq!(moved.assignee, None);
    assert_eq!(moved.description.as_deref(), Some(BRIEF));
}

#[test]
fn closing_a_hold_is_a_comment_plus_a_set() {
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
    assert_eq!(moved.status, Status::Ready);
    assert_eq!(moved.assignee, None);
    let comments = board.comments(&id.to_string()).unwrap();
    assert!(
        comments.iter().any(|c| c.body.contains("first option")),
        "the decision lives in the comment log"
    );
}
