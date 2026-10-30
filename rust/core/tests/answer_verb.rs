//! `ticket answer` (D2522EC3), ported from `AnswerVerbTests.swift`: the human's
//! answer recorded and the hold closed in **one write**: the dated `## Answer`
//! block appended to the description, the status the answer opens, and the next
//! assignee (omitted clears).
//!
//! The Swift suite drove the binary. The argv half (`--body -` on stdin,
//! `--assignee self`, the exit code) is PRT-9's; the write it drives is here, at the
//! board seam, with the same assertions about what lands and what is refused.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DescriptionBody, DomainError, Edit, SetFields, Status};

const BRIEF: &str = "## Why\n\nWhich option?\n\n## What\n\nPick one.";

fn answered_board() -> (Sandbox, tandem_core::domain::Board, uuid::Uuid) {
    let sandbox = Sandbox::new("answer");
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
fn answer_appends_the_dated_block_and_moves_the_status_and_assignee_together() {
    let (_sandbox, board, id) = answered_board();
    let answered = board
        .answer(
            &id.to_string(),
            Status::Ready,
            None,
            "A: the first option.",
            1_762_000_000,
        )
        .unwrap();

    assert_eq!(answered.id, id);
    assert_eq!(answered.status, Status::Ready);
    assert_eq!(
        answered.assignee, None,
        "the question is settled: the hold is closed"
    );

    let description = answered.description.clone().unwrap();
    assert!(
        description.starts_with(&format!("{BRIEF}\n\n## Answer \u{2014} ")),
        "the answer is appended below the brief: {description}"
    );
    assert!(description.contains("\n\nA: the first option."));
    let heading = description
        .lines()
        .find(|line| line.starts_with("## Answer \u{2014} "))
        .expect("a dated heading");
    let date = &heading["## Answer \u{2014} ".len()..];
    assert_eq!(date.len(), 10, "a dated heading, got {date:?}");
    assert!(
        date.split('-').count() == 3 && date.chars().all(|c| c.is_ascii_digit() || c == '-'),
        "a dated heading, got {date:?}"
    );
}

#[test]
fn an_omitted_assignee_clears_the_owner_and_an_empty_one_is_the_same() {
    for assignee in [None, Some(""), Some("   ")] {
        let (_sandbox, board, id) = answered_board();
        let answered = board
            .answer(
                &id.to_string(),
                Status::Ready,
                assignee,
                "B: settled.",
                1_762_000_000,
            )
            .unwrap();
        assert_eq!(
            answered.assignee, None,
            "an omitted or empty assignee clears the hold"
        );
    }
}

#[test]
fn the_next_owner_can_be_named_in_the_same_write() {
    let (_sandbox, board, id) = answered_board();
    let answered = board
        .answer(
            &id.to_string(),
            Status::Ready,
            Some("builder"),
            "A.",
            1_762_000_000,
        )
        .unwrap();
    assert_eq!(answered.assignee.as_deref(), Some("builder"));
}

#[test]
fn an_empty_body_is_refused_and_the_ticket_is_left_alone() {
    for body in ["", "   ", " \n\t "] {
        let (_sandbox, board, id) = answered_board();
        assert_eq!(
            board.answer(&id.to_string(), Status::Ready, None, body, 1_762_000_000),
            Err(DomainError::EmptyDescriptionBody),
            "body {body:?}"
        );
        let shown = board.ticket(&id.to_string()).unwrap();
        assert_eq!(
            shown.description.as_deref(),
            Some(BRIEF),
            "a refused answer writes nothing"
        );
        assert_eq!(shown.status, Status::OnHold);
        assert_eq!(shown.assignee.as_deref(), Some("human"));
    }
}

#[test]
fn an_unknown_id_is_a_domain_failure_carrying_the_open_board() {
    let (_sandbox, board, _) = answered_board();
    match board
        .answer("not-an-id", Status::Ready, None, "A", 1_762_000_000)
        .unwrap_err()
    {
        DomainError::UnknownId { id, open } => {
            assert_eq!(id, "not-an-id");
            assert_eq!(open.len(), 1, "the error carries the open board");
            assert_eq!(open[0].title, "Which option?");
        }
        other => panic!("expected unknownID, got {other:?}"),
    }
}

#[test]
fn the_body_lands_with_the_block_and_the_status_the_answer_opens() {
    let sandbox = Sandbox::new("answer-stored");
    let board = sandbox.board();
    let id = board.add("Question", AddTicket::default()).unwrap().id;
    board
        .answer(
            &id.to_string(),
            Status::Backlog,
            None,
            "B: needs re-refining.",
            1_762_000_000,
        )
        .unwrap();

    let shown = board.ticket(&id.to_string()).unwrap();
    assert_eq!(shown.status, Status::Backlog);
    assert_eq!(shown.assignee, None);
    let description = shown.description.unwrap();
    assert!(
        description.starts_with("## Answer \u{2014} "),
        "a ticket with no brief still gets the block"
    );
    assert!(description.contains("B: needs re-refining."));
    assert!(
        DescriptionBody::answered(Some(&description)),
        "the board now reads it as answered"
    );
}

#[test]
fn a_second_answer_appends_below_the_first_and_never_rewrites_it() {
    let (_sandbox, board, id) = answered_board();
    let first = board
        .answer(&id.to_string(), Status::OnHold, None, "A.", 1_762_000_000)
        .unwrap()
        .description
        .unwrap();
    let second = board
        .answer(
            &id.to_string(),
            Status::Ready,
            None,
            "B.",
            1_762_000_000 + 86_400,
        )
        .unwrap()
        .description
        .unwrap();
    assert!(
        second.starts_with(&first),
        "the first answer stays where it was"
    );
    assert!(second.ends_with("\n\nB."));
}
