//! Completion undo, ported from `CompletionUndoTests.swift`: completing erases the
//! stage a ticket came from, so the undo puts it back in backlog, idempotently, and
//! the offer runs out after six seconds.

mod support;

use support::Sandbox;
use tandem_core::domain::{
    AddTicket, CompletionUndo, DomainError, Edit, Filter, SetFields, Status,
};

#[test]
fn complete_then_undo_puts_the_ticket_back_in_backlog_with_no_completed_at() {
    let sandbox = Sandbox::new("undo-complete");
    let board = sandbox.board();
    let added = board.add("Buy milk", AddTicket::default()).unwrap();
    let offer = CompletionUndo::new(added.id, added.title.clone(), 1_000);

    let completed = board.complete(&offer.id.to_string()).unwrap();
    assert!(completed.done());
    assert!(completed.completed_at.is_some());

    let undone = board.undo(&offer).unwrap();
    assert_eq!(undone.id, added.id);
    assert_eq!(undone.status, Status::Backlog);
    assert!(!undone.done());
    assert!(undone.completed_at.is_none());

    // Persisted, not just returned: the ticket is open again, so it is back in the
    // list (and in the board's backlog column).
    let persisted = support::all(&board)
        .into_iter()
        .find(|ticket| ticket.id == added.id)
        .unwrap();
    assert_eq!(persisted.status, Status::Backlog);
    assert!(persisted.completed_at.is_none());
    assert_eq!(
        board
            .list_filter(Filter::Open)
            .unwrap()
            .into_iter()
            .map(|ticket| ticket.id)
            .collect::<Vec<_>>(),
        [added.id]
    );
    assert!(board.list_filter(Filter::Done).unwrap().is_empty());
}

#[test]
fn undo_lands_in_backlog_whatever_stage_the_ticket_came_from() {
    let sandbox = Sandbox::new("undo-stage");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::Ready),
                ..SetFields::default()
            },
        )
        .unwrap();
    let offer = CompletionUndo::new(added.id, added.title.clone(), 1_000);

    board.complete(&offer.id.to_string()).unwrap();
    assert_eq!(board.undo(&offer).unwrap().status, Status::Backlog);
}

#[test]
fn undo_is_idempotent_and_does_nothing_to_a_ticket_that_is_not_done() {
    let sandbox = Sandbox::new("undo-idempotent");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    let offer = CompletionUndo::new(added.id, added.title.clone(), 1_000);

    // Never completed: the undo has nothing to take back.
    let untouched = board.undo(&offer).unwrap();
    assert_eq!(untouched.status, Status::Backlog);
    assert!(untouched.completed_at.is_none());

    board.complete(&offer.id.to_string()).unwrap();
    assert_eq!(board.undo(&offer).unwrap().status, Status::Backlog);

    // A second undo changes nothing.
    let again = board.undo(&offer).unwrap();
    assert_eq!(again.status, Status::Backlog);
    assert!(again.completed_at.is_none());
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);
}

#[test]
fn undo_with_an_unknown_id_is_the_unknown_id_failure() {
    let sandbox = Sandbox::new("undo-unknown");
    let board = sandbox.board();
    let offer = CompletionUndo::new(uuid::Uuid::new_v4(), "Ghost", 1_000);
    match board.undo(&offer).unwrap_err() {
        DomainError::UnknownId { id, .. } => assert_eq!(id, offer.id.to_string()),
        other => panic!("expected unknownID, got {other:?}"),
    }
}

#[test]
fn the_offer_runs_out_after_six_seconds() {
    let start = 1_000;
    let offer = CompletionUndo::new(uuid::Uuid::new_v4(), "Task", start);

    assert_eq!(tandem_core::domain::undo::WINDOW, 6);
    assert_eq!(offer.expires_at(), 1_006);
    assert!(offer.is_live(start));
    assert!(offer.is_live(1_005));
    assert!(!offer.is_live(1_006));
}
