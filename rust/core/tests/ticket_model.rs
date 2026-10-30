//! The ticket model (ADR 0017, origin ADR 0020), ported from
//! `TicketModelTests.swift`: status, archived, description, assignee, origin and
//! the derived `done`, proved at the board's observable behaviour, never at
//! internal mechanics.
//!
//! Left behind with the JSON store (the port reads no `tickets.json`): the
//! legacy-file cases (an older file, the `done` mirror, the unknown status a later
//! build wrote, the one-time import's `.pre-sqlite-*` copies) and the two wire
//! cases (`StoreEncoding`), which belong to PRT-11. The behaviour they pinned that
//! still has a home is ported at its new edge: a stored word this build does not
//! know reads as `backlog`, a retired word reads as `on-hold`.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, Edit, Filter, SetFields, Status, Ticket};

fn temp_board() -> (Sandbox, Board) {
    let sandbox = Sandbox::new("ticket");
    let board = sandbox.board();
    (sandbox, board)
}

#[test]
fn a_ticket_round_trips_status_archived_description_assignee_and_lane() {
    let (sandbox, board) = temp_board();
    board.add_lane("Core", None).unwrap();
    let added = board
        .add(
            "Ship the model",
            AddTicket {
                lane: Some("Core"),
                assignee: Some("UI"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let set = board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::Ready),
                description: Edit::Set(Some("Three axes, two new fields.")),
                ..SetFields::default()
            },
        )
        .unwrap();

    assert_eq!(set.status, Status::Ready);
    assert!(!set.done());
    assert!(!set.archived);
    assert_eq!(set.assignee.as_deref(), Some("UI"));
    assert_eq!(
        set.description.as_deref(),
        Some("Three axes, two new fields.")
    );
    assert_eq!(set.lane.as_deref(), Some("Core"));

    // A fresh Board over the same home reads the same ticket.
    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    let read = fresh.list_filter(Filter::All).unwrap().remove(0);
    assert_eq!(read.status, Status::Ready);
    assert_eq!(read.assignee.as_deref(), Some("UI"));
    assert_eq!(
        read.description.as_deref(),
        Some("Three axes, two new fields.")
    );
    assert_eq!(read.lane.as_deref(), Some("Core"));
    assert!(!read.done());
}

#[test]
fn in_review_round_trips_through_the_store() {
    let (sandbox, board) = temp_board();
    let added = board
        .add(
            "Waiting for review",
            AddTicket {
                status: Status::InReview,
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(added.status, Status::InReview);
    assert!(!added.done());

    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    let read = fresh.list_filter(Filter::All).unwrap().remove(0);
    assert_eq!(read.status, Status::InReview);
    assert!(!read.done());
}

#[test]
fn workflow_order_puts_in_review_between_in_progress_and_done() {
    assert_eq!(
        Status::WORKFLOW,
        [
            Status::Backlog,
            Status::OnHold,
            Status::Ready,
            Status::InProgress,
            Status::InReview,
            Status::Done,
        ]
    );
    assert_eq!(
        Status::WIRE_VALUES,
        [
            "backlog",
            "on-hold",
            "ready",
            "in-progress",
            "in-review",
            "done"
        ]
    );
    assert!(
        !Status::WIRE_VALUES.contains(&"refined"),
        "the retired word is not a wire value"
    );
}

#[test]
fn on_hold_round_trips_through_the_store() {
    let (sandbox, board) = temp_board();
    let added = board
        .add(
            "Parked on purpose",
            AddTicket {
                status: Status::OnHold,
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(added.status, Status::OnHold);
    assert!(!added.done());

    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    assert_eq!(
        fresh.list_filter(Filter::All).unwrap()[0].status,
        Status::OnHold
    );
}

/// The writer half of the on-hold gate: the retired word leaves the domain and the
/// wire, and a store that still spells it reads as the slot it named, never as
/// `backlog`.
#[test]
fn a_stored_refined_reads_as_on_hold_and_the_domain_no_longer_accepts_the_word() {
    assert_eq!(Status::stored("refined"), Status::OnHold);
    assert_eq!(Status::stored("a word from a later build"), Status::Backlog);
    assert_eq!(
        Status::parse("refined"),
        Err(tandem_core::domain::DomainError::InvalidStatus(
            "refined".to_string()
        ))
    );
}

#[test]
fn an_origin_round_trips_through_the_store() {
    let (sandbox, board) = temp_board();
    let added = board
        .add(
            "Filed by an agent",
            AddTicket {
                origin: "builder",
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(added.origin, "builder");

    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    assert_eq!(fresh.list_filter(Filter::All).unwrap()[0].origin, "builder");
}

#[test]
fn add_stamps_its_origin_defaulting_to_human_with_the_assignees_spelling_rules() {
    let (_sandbox, board) = temp_board();
    assert_eq!(
        board.add("default", AddTicket::default()).unwrap().origin,
        "human"
    );
    assert_eq!(
        board
            .add(
                "empty",
                AddTicket {
                    origin: "   ",
                    ..AddTicket::default()
                }
            )
            .unwrap()
            .origin,
        "human"
    );
    assert_eq!(
        board
            .add(
                "reserved",
                AddTicket {
                    origin: "  HUMAN ",
                    ..AddTicket::default()
                }
            )
            .unwrap()
            .origin,
        "human"
    );
    // An agent's name is stored as given, not lowercased.
    assert_eq!(
        board
            .add(
                "named",
                AddTicket {
                    origin: " Blue Agent ",
                    ..AddTicket::default()
                }
            )
            .unwrap()
            .origin,
        "Blue Agent"
    );
}

#[test]
fn origin_is_written_once_at_add_and_set_move_place_reopen_leave_it() {
    let (_sandbox, board) = temp_board();
    board.add_lane("Core", None).unwrap();
    let added = board
        .add(
            "Task",
            AddTicket {
                assignee: Some("core"),
                origin: "builder",
                ..AddTicket::default()
            },
        )
        .unwrap();

    let after = board
        .set(
            &added.id.to_string(),
            SetFields {
                title: Edit::Set("Renamed"),
                status: Edit::Set(Status::InProgress),
                assignee: Edit::Set(Some("human")),
                description: Edit::Set(Some("Body")),
                archived: Edit::Set(true),
            },
        )
        .unwrap();
    assert_eq!(after.origin, "builder", "set has no origin to change");

    let moved = board
        .move_ticket(&added.id.to_string(), Some("Core"), 0)
        .unwrap();
    assert_eq!(moved.origin, "builder");

    let placed = board
        .place(
            &added.id.to_string(),
            Some("Core"),
            Status::Done,
            usize::MAX,
        )
        .unwrap();
    assert_eq!(placed.origin, "builder");

    assert_eq!(
        board.reopen(&added.id.to_string()).unwrap().origin,
        "builder"
    );
}

#[test]
fn completing_writes_status_done_and_completed_at() {
    let (sandbox, board) = temp_board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board.complete(&added.id.to_string()).unwrap();

    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    let read = fresh.list_filter(Filter::All).unwrap().remove(0);
    assert_eq!(read.status, Status::Done);
    assert!(read.completed_at.is_some());
}

#[test]
fn reopening_after_done_lands_in_backlog() {
    let (_sandbox, board) = temp_board();
    let added = board
        .add(
            "Task",
            AddTicket {
                status: Status::Ready,
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(added.status, Status::Ready, "a ticket can be filed ready");

    board.complete(&added.id.to_string()).unwrap();
    let reopened = board.reopen(&added.id.to_string()).unwrap();
    assert_eq!(reopened.status, Status::Backlog);
    assert!(!reopened.done());
    assert!(reopened.completed_at.is_none());

    // Idempotent: a ticket that is not done is left alone.
    board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::OnHold),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(
        board.reopen(&added.id.to_string()).unwrap().status,
        Status::OnHold
    );
}

#[test]
fn set_changes_only_the_fields_it_names() {
    let (_sandbox, board) = temp_board();
    board.add_lane("Core", None).unwrap();
    let added = board
        .add(
            "Title",
            AddTicket {
                lane: Some("Core"),
                assignee: Some("UI"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .set(
            &added.id.to_string(),
            SetFields {
                description: Edit::Set(Some("Body")),
                ..SetFields::default()
            },
        )
        .unwrap();

    let after = board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::InProgress),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(after.title, "Title");
    assert_eq!(after.description.as_deref(), Some("Body"));
    assert_eq!(after.assignee.as_deref(), Some("UI"));
    assert_eq!(after.lane.as_deref(), Some("Core"));
    assert_eq!(after.status, Status::InProgress);
    assert!(!after.archived);
}

#[test]
fn set_clears_an_optional_field_with_set_none_and_leaves_it_with_unchanged() {
    let (_sandbox, board) = temp_board();
    let added = board
        .add(
            "kept",
            AddTicket {
                status: Status::Ready,
                assignee: Some("core"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .set(
            &added.id.to_string(),
            SetFields {
                description: Edit::Set(Some("Body")),
                ..SetFields::default()
            },
        )
        .unwrap();

    let cleared = board
        .set(
            &added.id.to_string(),
            SetFields {
                assignee: Edit::Set(None),
                description: Edit::Set(None),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.assignee, None);
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.title, "kept", "an unnamed field is untouched");
    assert_eq!(cleared.status, Status::Ready);
}

#[test]
fn set_trims_text_treats_an_empty_value_as_cleared_and_stores_human_lowercase() {
    let (_sandbox, board) = temp_board();
    let added = board.add("old", AddTicket::default()).unwrap();
    let set = board
        .set(
            &added.id.to_string(),
            SetFields {
                title: Edit::Set("  New  "),
                assignee: Edit::Set(Some("  HUMAN ")),
                description: Edit::Set(Some("  body\n")),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(set.title, "New");
    assert_eq!(set.description.as_deref(), Some("body"));
    assert_eq!(set.assignee.as_deref(), Some("human"));

    let cleared = board
        .set(
            &added.id.to_string(),
            SetFields {
                assignee: Edit::Set(Some("")),
                description: Edit::Set(Some("  \n ")),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.assignee, None);

    // An agent's name is stored as given, not lowercased.
    let named = board
        .set(
            &added.id.to_string(),
            SetFields {
                assignee: Edit::Set(Some("Core-Agent")),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(named.assignee.as_deref(), Some("Core-Agent"));
}

#[test]
fn set_rejects_an_empty_title_and_leaves_the_ticket_untouched() {
    let (_sandbox, board) = temp_board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    for title in ["", "   ", "\t\n  "] {
        assert!(matches!(
            board.set(
                &added.id.to_string(),
                SetFields {
                    title: Edit::Set(title),
                    ..SetFields::default()
                }
            ),
            Err(tandem_core::domain::DomainError::EmptyTitle)
        ));
    }
    assert_eq!(board.list_filter(Filter::All).unwrap()[0].title, "Task");
}

#[test]
fn set_with_an_unknown_id_is_the_unknown_id_failure_carrying_the_open_list() {
    let (sandbox, board) = temp_board();
    let open = board.add("A", AddTicket::default()).unwrap();
    let error = board
        .set(
            &uuid_string(),
            SetFields {
                status: Edit::Set(Status::Ready),
                ..SetFields::default()
            },
        )
        .unwrap_err();
    match error {
        tandem_core::domain::DomainError::UnknownId { open: listed, .. } => {
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].id, open.id);
        }
        other => panic!("expected unknownID, got {other:?}"),
    }
    drop(sandbox);
}

#[test]
fn setting_status_to_done_stamps_completed_at_and_leaving_done_clears_it() {
    let (_sandbox, board) = temp_board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    let done = board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::Done),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert!(done.done());
    assert!(done.completed_at.is_some());

    let ready = board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::Ready),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert!(!ready.done());
    assert!(ready.completed_at.is_none());

    // Free-form: backwards and forwards both work, without a transition table.
    let backlog = board
        .set(
            &added.id.to_string(),
            SetFields {
                status: Edit::Set(Status::Backlog),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(backlog.status, Status::Backlog);
}

#[test]
fn archiving_takes_a_ticket_out_of_the_default_views_and_never_deletes_it() {
    let (_sandbox, board) = temp_board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set(
            &added.id.to_string(),
            SetFields {
                archived: Edit::Set(true),
                ..SetFields::default()
            },
        )
        .unwrap();

    assert!(board.list_filter(Filter::Open).unwrap().is_empty());
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);

    let unarchived = board
        .set(
            &added.id.to_string(),
            SetFields {
                archived: Edit::Set(false),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert!(!unarchived.archived);
    assert_eq!(board.list_filter(Filter::Open).unwrap().len(), 1);
}

#[test]
fn add_files_a_ticket_in_a_status_and_with_an_owner_and_a_title_alone_stays_default() {
    let (_sandbox, board) = temp_board();
    let plain = board.add("plain", AddTicket::default()).unwrap();
    assert_eq!(plain.status, Status::Backlog);
    assert_eq!(plain.assignee, None);
    assert_eq!(plain.origin, "human");
    assert!(!plain.archived);
    assert!(!plain.done());

    let filed = board
        .add(
            "filed",
            AddTicket {
                status: Status::Ready,
                assignee: Some("core"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(filed.status, Status::Ready);
    assert_eq!(filed.assignee.as_deref(), Some("core"));
    assert!(!filed.done());
    assert!(filed.completed_at.is_none());
}

/// The board's own default: a ticket read back equals the one the write returned,
/// which is what every round-trip case above rests on.
#[test]
fn the_returned_ticket_is_the_one_a_read_returns() {
    let (_sandbox, board) = temp_board();
    let added = board.add("same", AddTicket::default()).unwrap();
    let read: Ticket = board.list_filter(Filter::All).unwrap().remove(0);
    assert_eq!(added, read);
}

fn uuid_string() -> String {
    uuid::Uuid::new_v4().to_string()
}
