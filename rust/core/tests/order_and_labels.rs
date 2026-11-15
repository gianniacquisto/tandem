//! Stored order and labels, ported from `OrderAndLabelsTests.swift`: the vector
//! order is the order (never recomputed from `created_at`), `add` lands on top,
//! labels group the list, and `move` covers both reorder and move-between.
//!
//! Left behind with the JSON store: the hand-written file cases (a dangling label
//! name, the grouped-order repair, the older file, the version-2 file, the renamed
//! keys read beside the v1 ones). They pinned the reader the port does not have;
//! the behaviour they pinned that still has a home is a label a ticket names but the
//! label list does not carry, ported through `rename_label` and `delete_label`.

mod support;

use std::time::SystemTime;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Filter, LabelDeletion, Status};

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
fn add_label_appends_and_empty_and_duplicate_names_are_rejected() {
    let sandbox = Sandbox::new("label-add");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    board.add_label("  Home  ", None, None).unwrap();
    assert_eq!(board.labels().unwrap(), ["Work", "Home"]);
    // Each verb returns the label list after its write.
    assert_eq!(
        board.add_label("Play", None, None).unwrap(),
        ["Work", "Home", "Play"]
    );
    assert_eq!(
        board.add_label("   ", None, None),
        Err(DomainError::EmptyLabelName)
    );
    assert_eq!(
        board.add_label("Work", None, None),
        Err(DomainError::DuplicateLabel("Work".to_string()))
    );
}

#[test]
fn add_files_on_top_of_that_label_and_an_unknown_label_is_rejected() {
    let sandbox = Sandbox::new("label-add-ticket");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                label: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "w2",
            AddTicket {
                label: Some("Work"),
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
            .map(|ticket| ticket.label.clone())
            .collect::<Vec<_>>(),
        [None, Some("Work".to_string()), Some("Work".to_string())]
    );

    assert_eq!(
        board.add(
            "x",
            AddTicket {
                label: Some("Nope"),
                ..AddTicket::default()
            }
        ),
        Err(DomainError::UnknownLabel("Nope".to_string()))
    );
}

#[test]
fn rename_label_rewrites_the_label_list_and_every_ticket_that_belongs_to_it() {
    let sandbox = Sandbox::new("label-rename");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                label: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board.rename_label("Work", "Job", None, None).unwrap();
    // Renaming to itself is a no-op that still reports the list.
    assert_eq!(
        board.rename_label("Job", "Job", None, None).unwrap(),
        ["Job"]
    );

    assert_eq!(board.labels().unwrap(), ["Job"]);
    let tickets = support::all(&board);
    assert_eq!(
        tickets
            .iter()
            .find(|ticket| ticket.title == "w1")
            .and_then(|ticket| ticket.label.clone())
            .as_deref(),
        Some("Job")
    );
    assert_eq!(
        tickets
            .iter()
            .find(|ticket| ticket.title == "d1")
            .and_then(|ticket| ticket.label.clone()),
        None
    );

    assert_eq!(
        board.rename_label("Ghost", "X", None, None),
        Err(DomainError::UnknownLabel("Ghost".to_string()))
    );
    assert_eq!(
        board.rename_label("Job", " ", None, None),
        Err(DomainError::EmptyLabelName)
    );
    board.add_label("Home", None, None).unwrap();
    assert_eq!(
        board.rename_label("Job", "Home", None, None),
        Err(DomainError::DuplicateLabel("Home".to_string()))
    );
}

#[test]
fn a_rename_to_itself_writes_nothing() {
    let sandbox = Sandbox::new("label-rename-noop");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    let before = modified(&sandbox.database());
    board.rename_label("Work", "Work", None, None).unwrap();
    assert_eq!(
        modified(&sandbox.database()),
        before,
        "a no-op write writes nothing"
    );
}

#[test]
fn delete_label_moves_its_tickets_to_the_default_area_and_never_deletes_them() {
    let sandbox = Sandbox::new("label-delete");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                label: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "w2",
            AddTicket {
                label: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    let moved: LabelDeletion = board.delete_label("Work").unwrap();
    assert_eq!(moved.moved, 2);
    assert!(moved.labels.is_empty());
    assert!(board.labels().unwrap().is_empty());

    let tickets = support::all(&board);
    assert_eq!(tickets.len(), 3, "nothing deleted");
    assert!(tickets.iter().all(|ticket| ticket.label.is_none()));
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["d1", "w2", "w1"],
        "order preserved"
    );

    assert_eq!(
        board.delete_label("Work"),
        Err(DomainError::UnknownLabel("Work".to_string()))
    );
}

#[test]
fn move_places_a_ticket_at_a_zero_based_index_within_another_label() {
    let sandbox = Sandbox::new("label-move-between");
    let board = sandbox.board();
    board.add_label("Work", None, None).unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board.add("d2", AddTicket::default()).unwrap();
    board
        .add(
            "w1",
            AddTicket {
                label: Some("Work"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "w2",
            AddTicket {
                label: Some("Work"),
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
            .map(|ticket| ticket.label.clone())
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
fn move_rejects_an_unknown_id_and_an_unknown_label() {
    let sandbox = Sandbox::new("label-move-rejects");
    let board = sandbox.board();
    let a = board.add("a", AddTicket::default()).unwrap().id;
    assert_eq!(
        board.move_ticket(&a.to_string(), Some("Nope"), 0),
        Err(DomainError::UnknownLabel("Nope".to_string()))
    );
    assert!(matches!(
        board.move_ticket(&uuid::Uuid::new_v4().to_string(), None, 0),
        Err(DomainError::UnknownId { .. })
    ));
}

#[test]
fn move_label_reorders_the_label_list_and_the_index_clamps_to_the_ends() {
    let sandbox = Sandbox::new("label-order");
    let board = sandbox.board();
    board.add_label("A", None, None).unwrap();
    board.add_label("B", None, None).unwrap();
    board.add_label("C", None, None).unwrap();
    assert_eq!(board.labels().unwrap(), ["A", "B", "C"]);

    assert_eq!(board.move_label("B", 2).unwrap(), ["A", "C", "B"]);
    assert_eq!(board.move_label("B", 0).unwrap(), ["B", "A", "C"]);
    assert_eq!(board.move_label("B", 99).unwrap(), ["A", "C", "B"]);

    // It is stored order, not a UI sort, so it survives a re-read.
    assert_eq!(sandbox.board().labels().unwrap(), ["A", "C", "B"]);
}

#[test]
fn move_label_rejects_an_unknown_name() {
    let sandbox = Sandbox::new("label-order-rejects");
    let board = sandbox.board();
    assert_eq!(
        board.move_label("Nope", 0),
        Err(DomainError::UnknownLabel("Nope".to_string()))
    );
}

#[test]
fn moving_a_label_carries_its_tickets_and_the_default_area_stays_first() {
    let sandbox = Sandbox::new("label-order-tickets");
    let board = sandbox.board();
    board.add_label("A", None, None).unwrap();
    board.add_label("B", None, None).unwrap();
    board.add("d1", AddTicket::default()).unwrap();
    board
        .add(
            "a1",
            AddTicket {
                label: Some("A"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "b1",
            AddTicket {
                label: Some("B"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    // Stored: default [d1], A [a1], B [b1].
    assert_eq!(support::titles(&board, Filter::All), ["d1", "a1", "b1"]);

    board.move_label("B", 0).unwrap();
    // B leads the labels now, but the default area is still the top group.
    assert_eq!(board.labels().unwrap(), ["B", "A"]);
    assert_eq!(support::titles(&board, Filter::All), ["d1", "b1", "a1"]);
    assert_eq!(
        support::all(&board)
            .iter()
            .map(|ticket| ticket.label.clone())
            .collect::<Vec<_>>(),
        [None, Some("B".to_string()), Some("A".to_string())]
    );
}

#[test]
fn a_ticket_filed_in_a_label_keeps_its_status_through_a_label_reorder() {
    let sandbox = Sandbox::new("label-status");
    let board = sandbox.board();
    board.add_label("A", None, None).unwrap();
    board
        .add(
            "a1",
            AddTicket {
                label: Some("A"),
                status: Status::Ready,
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "a2",
            AddTicket {
                label: Some("A"),
                status: Status::OnHold,
                ..AddTicket::default()
            },
        )
        .unwrap();
    board.add_label("B", None, None).unwrap();
    board.move_label("A", 1).unwrap();
    assert_eq!(
        support::all(&board)
            .iter()
            .map(|ticket| ticket.status)
            .collect::<Vec<_>>(),
        [Status::OnHold, Status::Ready],
        "newest on top, and the reorder left every status alone"
    );
}
