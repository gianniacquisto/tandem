//! Stored order and lanes, ported from `OrderAndLanesTests.swift`: the vector
//! order is the order (never recomputed from `created_at`), `add` lands on top,
//! lanes group the list, and `move` covers both reorder and move-between.
//!
//! Left behind with the JSON store: the hand-written file cases (a dangling lane
//! name, the grouped-order repair, the older file, the version-2 file, the renamed
//! keys read beside the v1 ones). They pinned the reader the port does not have;
//! the behaviour they pinned that still has a home is a lane a ticket names but the
//! lane list does not carry, ported through `rename_lane` and `delete_lane`.

mod support;

use std::time::SystemTime;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Filter, LaneDeletion, Status};

fn modified(path: &std::path::Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .expect("the database answers")
}

#[test]
fn a_manual_order_survives_a_fresh_board_it_comes_off_disk_not_from_created_at() {
    let sandbox = Sandbox::new("order-stored");
    let board = sandbox.board();
    let c = board.add("c", AddTicket::default()).unwrap().id;
    board.add("b", AddTicket::default()).unwrap();
    board.add("a", AddTicket::default()).unwrap();
    assert_eq!(
        support::titles(&board, Filter::All),
        ["a", "b", "c"],
        "newest on top"
    );

    // Move the oldest to the top.
    board.move_ticket(&c.to_string(), None, 0).unwrap();
    assert_eq!(support::titles(&board, Filter::All), ["c", "a", "b"]);

    // A fresh Board over the same home sees the same order.
    let fresh = sandbox.board();
    assert_eq!(support::titles(&fresh, Filter::All), ["c", "a", "b"]);
}

#[test]
fn move_within_the_default_area_reorders_and_the_index_clamps_to_the_ends() {
    let sandbox = Sandbox::new("order-default");
    let board = sandbox.board();
    board.add("c", AddTicket::default()).unwrap();
    board.add("b", AddTicket::default()).unwrap();
    let a = board.add("a", AddTicket::default()).unwrap().id;
    assert_eq!(support::titles(&board, Filter::All), ["a", "b", "c"]);

    board.move_ticket(&a.to_string(), None, 0).unwrap();
    assert_eq!(
        support::titles(&board, Filter::All),
        ["a", "b", "c"],
        "already first"
    );

    board.move_ticket(&a.to_string(), None, 99).unwrap();
    assert_eq!(
        support::titles(&board, Filter::All),
        ["b", "c", "a"],
        "clamped to the end"
    );

    board.move_ticket(&a.to_string(), None, 0).unwrap();
    assert_eq!(
        support::titles(&board, Filter::All),
        ["a", "b", "c"],
        "clamped to the front"
    );
}

#[test]
fn a_filter_preserves_the_stored_order() {
    let sandbox = Sandbox::new("order-filter");
    let board = sandbox.board();
    let a = board.add("a", AddTicket::default()).unwrap().id;
    let b = board.add("b", AddTicket::default()).unwrap().id;
    let c = board.add("c", AddTicket::default()).unwrap().id;
    board.complete(&c.to_string()).unwrap(); // stored: c(done), b, a

    let ids = |filter| {
        board
            .list_filter(filter)
            .unwrap()
            .into_iter()
            .map(|ticket| ticket.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(Filter::All), [c, b, a]);
    assert_eq!(ids(Filter::Open), [b, a]);
    assert_eq!(ids(Filter::Done), [c]);
    let _ = (a, b);
}

#[test]
fn add_lane_appends_and_empty_and_duplicate_names_are_rejected() {
    let sandbox = Sandbox::new("lane-add");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board.add_lane("  Home  ", None).unwrap();
    assert_eq!(board.lanes().unwrap(), ["Work", "Home"]);
    // Each verb returns the lane list after its write.
    assert_eq!(
        board.add_lane("Play", None).unwrap(),
        ["Work", "Home", "Play"]
    );
    assert_eq!(board.add_lane("   ", None), Err(DomainError::EmptyLaneName));
    assert_eq!(
        board.add_lane("Work", None),
        Err(DomainError::DuplicateLane("Work".to_string()))
    );
}

#[test]
fn add_files_on_top_of_that_lane_and_an_unknown_lane_is_rejected() {
    let sandbox = Sandbox::new("lane-add-ticket");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "w2",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    // Groups in order: default first, then Work; newest on top within a group.
    let tickets = support::all(&board);
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["d1", "w2", "w1"]
    );
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.lane.clone())
            .collect::<Vec<_>>(),
        [None, Some("Work".to_string()), Some("Work".to_string())]
    );

    assert_eq!(
        board.add(
            "x",
            AddTicket {
                lane: Some("Nope"),
                ..AddTicket::default()
            }
        ),
        Err(DomainError::UnknownLane("Nope".to_string()))
    );
}

#[test]
fn rename_lane_rewrites_the_lane_list_and_every_ticket_that_belongs_to_it() {
    let sandbox = Sandbox::new("lane-rename");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board.rename_lane("Work", "Job", None).unwrap();
    // Renaming to itself is a no-op that still reports the list.
    assert_eq!(board.rename_lane("Job", "Job", None).unwrap(), ["Job"]);

    assert_eq!(board.lanes().unwrap(), ["Job"]);
    let tickets = support::all(&board);
    assert_eq!(
        tickets
            .iter()
            .find(|ticket| ticket.title == "w1")
            .and_then(|ticket| ticket.lane.clone())
            .as_deref(),
        Some("Job")
    );
    assert_eq!(
        tickets
            .iter()
            .find(|ticket| ticket.title == "d1")
            .and_then(|ticket| ticket.lane.clone()),
        None
    );

    assert_eq!(
        board.rename_lane("Ghost", "X", None),
        Err(DomainError::UnknownLane("Ghost".to_string()))
    );
    assert_eq!(
        board.rename_lane("Job", " ", None),
        Err(DomainError::EmptyLaneName)
    );
    board.add_lane("Home", None).unwrap();
    assert_eq!(
        board.rename_lane("Job", "Home", None),
        Err(DomainError::DuplicateLane("Home".to_string()))
    );
}

#[test]
fn a_rename_to_itself_writes_nothing() {
    let sandbox = Sandbox::new("lane-rename-noop");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    let before = modified(&sandbox.database());
    board.rename_lane("Work", "Work", None).unwrap();
    assert_eq!(
        modified(&sandbox.database()),
        before,
        "a no-op write writes nothing"
    );
}

#[test]
fn delete_lane_moves_its_tickets_to_the_default_area_and_never_deletes_them() {
    let sandbox = Sandbox::new("lane-delete");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "w2",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    let moved: LaneDeletion = board.delete_lane("Work").unwrap();
    assert_eq!(moved.moved, 2);
    assert!(moved.lanes.is_empty());
    assert!(board.lanes().unwrap().is_empty());

    let tickets = support::all(&board);
    assert_eq!(tickets.len(), 3, "nothing deleted");
    assert!(tickets.iter().all(|ticket| ticket.lane.is_none()));
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["d1", "w2", "w1"],
        "order preserved"
    );

    assert_eq!(
        board.delete_lane("Work"),
        Err(DomainError::UnknownLane("Work".to_string()))
    );
}

#[test]
fn move_places_a_ticket_at_a_zero_based_index_within_another_lane() {
    let sandbox = Sandbox::new("lane-move-between");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board.add("d2", AddTicket::default()).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "w2",
            AddTicket {
                lane: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    // Stored: default [d2, d1], then Work [w2, w1].

    let d1 = support::all(&board)
        .into_iter()
        .find(|ticket| ticket.title == "d1")
        .unwrap()
        .id;
    // Move d1 into Work at index 1: between w2 and w1.
    board.move_ticket(&d1.to_string(), Some("Work"), 1).unwrap();
    let moved = support::all(&board);
    assert_eq!(
        moved
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["d2", "w2", "d1", "w1"]
    );
    assert_eq!(
        moved
            .iter()
            .map(|ticket| ticket.lane.clone())
            .collect::<Vec<_>>(),
        [
            None,
            Some("Work".to_string()),
            Some("Work".to_string()),
            Some("Work".to_string())
        ]
    );
}

#[test]
fn move_rejects_an_unknown_id_and_an_unknown_lane() {
    let sandbox = Sandbox::new("lane-move-rejects");
    let board = sandbox.board();
    let a = board.add("a", AddTicket::default()).unwrap().id;
    assert_eq!(
        board.move_ticket(&a.to_string(), Some("Nope"), 0),
        Err(DomainError::UnknownLane("Nope".to_string()))
    );
    assert!(matches!(
        board.move_ticket(&uuid::Uuid::new_v4().to_string(), None, 0),
        Err(DomainError::UnknownId { .. })
    ));
}

#[test]
fn move_lane_reorders_the_lane_list_and_the_index_clamps_to_the_ends() {
    let sandbox = Sandbox::new("lane-order");
    let board = sandbox.board();
    board.add_lane("A", None).unwrap();
    board.add_lane("B", None).unwrap();
    board.add_lane("C", None).unwrap();
    assert_eq!(board.lanes().unwrap(), ["A", "B", "C"]);

    assert_eq!(board.move_lane("B", 2).unwrap(), ["A", "C", "B"]);
    assert_eq!(board.move_lane("B", 0).unwrap(), ["B", "A", "C"]);
    assert_eq!(board.move_lane("B", 99).unwrap(), ["A", "C", "B"]);

    // It is stored order, not a UI sort, so it survives a re-read.
    assert_eq!(sandbox.board().lanes().unwrap(), ["A", "C", "B"]);
}

#[test]
fn move_lane_rejects_an_unknown_name() {
    let sandbox = Sandbox::new("lane-order-rejects");
    let board = sandbox.board();
    assert_eq!(
        board.move_lane("Nope", 0),
        Err(DomainError::UnknownLane("Nope".to_string()))
    );
}

#[test]
fn moving_a_lane_carries_its_tickets_and_the_default_area_stays_first() {
    let sandbox = Sandbox::new("lane-order-tickets");
    let board = sandbox.board();
    board.add_lane("A", None).unwrap();
    board.add_lane("B", None).unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "a1",
            AddTicket {
                lane: Some("A"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "b1",
            AddTicket {
                lane: Some("B"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    // Stored: default [d1], A [a1], B [b1].
    assert_eq!(support::titles(&board, Filter::All), ["d1", "a1", "b1"]);

    board.move_lane("B", 0).unwrap();
    // B leads the lanes now, but the default area is still the top group.
    assert_eq!(board.lanes().unwrap(), ["B", "A"]);
    assert_eq!(support::titles(&board, Filter::All), ["d1", "b1", "a1"]);
    assert_eq!(
        support::all(&board)
            .iter()
            .map(|ticket| ticket.lane.clone())
            .collect::<Vec<_>>(),
        [None, Some("B".to_string()), Some("A".to_string())]
    );
}

#[test]
fn a_ticket_filed_in_a_lane_keeps_its_status_through_a_lane_reorder() {
    let sandbox = Sandbox::new("lane-status");
    let board = sandbox.board();
    board.add_lane("A", None).unwrap();
    board
        .add(
            "a1",
            AddTicket {
                lane: Some("A"),
                status: Status::Ready,
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "a2",
            AddTicket {
                lane: Some("A"),
                status: Status::OnHold,
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add_lane("B", None).unwrap();
    board.move_lane("A", 1).unwrap();
    assert_eq!(
        support::all(&board)
            .iter()
            .map(|ticket| ticket.status)
            .collect::<Vec<_>>(),
        [Status::OnHold, Status::Ready],
        "newest on top, and the reorder left every status alone"
    );
}
