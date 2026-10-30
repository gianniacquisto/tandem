//! The board: the core's half of it, ported from `BoardTests.swift`. Two seams:
//! [`TicketBoard::rows`] (pure grouping, so the board and the list cannot disagree
//! about what a lane holds) and [`Board::place`] (the drop, one write for lane and
//! status together, proved against the real store).

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, Status, Ticket, TicketBoard, TicketQuery};

fn ticket(title: &str, status: Status, lane: Option<&str>) -> Ticket {
    Ticket {
        status,
        lane: lane.map(str::to_string),
        ..Ticket::new(title)
    }
}

#[test]
fn every_declared_lane_draws_a_row_even_with_no_tickets_in_it() {
    let rows = TicketBoard::rows(&[], &["Board".to_string(), "Memory".to_string()]);
    assert_eq!(
        rows.iter().map(|row| row.key.clone()).collect::<Vec<_>>(),
        [None, Some("Board".to_string()), Some("Memory".to_string())]
    );
    assert!(rows.iter().all(|row| row.count() == 0));
}

#[test]
fn a_row_is_its_lane_split_by_status_in_stored_order_inside_each_column() {
    let rows = TicketBoard::rows(
        &[
            ticket("a", Status::Backlog, None),
            ticket("b", Status::Backlog, None),
            ticket("c", Status::Ready, None),
        ],
        &[],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .tickets(Status::Backlog)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        rows[0]
            .tickets(Status::Ready)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );
    assert!(rows[0].tickets(Status::Done).is_empty());
}

/// Every status is a drawn column, so a row's cell and the lane header's count
/// cannot disagree.
#[test]
fn a_row_draws_every_ticket_in_its_own_column_and_the_header_count_matches_the_cells() {
    let rows = TicketBoard::rows(
        &[
            ticket("parked on purpose", Status::OnHold, Some("Tandem")),
            ticket("briefed", Status::Ready, Some("Tandem")),
        ],
        &["Tandem".to_string()],
    );
    let row = &rows[1];
    assert_eq!(
        row.tickets(Status::OnHold)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["parked on purpose"]
    );
    assert_eq!(
        row.tickets(Status::Ready)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["briefed"]
    );
    let drawn: usize = Status::WORKFLOW
        .iter()
        .map(|status| row.tickets(*status).len())
        .sum();
    assert_eq!(
        drawn,
        row.count(),
        "the lane header counts exactly what the cells draw"
    );
}

#[test]
fn the_default_area_is_the_first_row_then_declared_lanes_then_a_lane_only_a_ticket_names() {
    let rows = TicketBoard::rows(
        &[
            ticket("loose", Status::Backlog, None),
            ticket("filed", Status::Backlog, Some("Board")),
            ticket("hand edited", Status::Backlog, Some("Stray")),
        ],
        &["Board".to_string(), "Memory".to_string()],
    );
    assert_eq!(
        rows.iter().map(|row| row.key.clone()).collect::<Vec<_>>(),
        [
            None,
            Some("Board".to_string()),
            Some("Memory".to_string()),
            Some("Stray".to_string()),
        ]
    );
    assert_eq!(
        rows[3]
            .tickets(Status::Backlog)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["hand edited"]
    );
}

#[test]
fn archived_tickets_are_the_callers_decision_not_the_boards() {
    let open = ticket("open", Status::Backlog, None);
    let archived = Ticket {
        archived: true,
        ..ticket("archived", Status::Done, None)
    };
    let all = TicketBoard::rows(&[open.clone(), archived.clone()], &[]);
    assert_eq!(all[0].count(), 2);

    let visible = TicketBoard::rows(&TicketQuery::default().apply(&[open, archived]), &[]);
    assert!(visible[0].tickets(Status::Done).is_empty());
    assert_eq!(visible[0].count(), 1);
}

#[test]
fn a_drop_sets_the_lane_and_the_status_in_one_write() {
    let sandbox = Sandbox::new("board-place");
    let board = sandbox.board();
    board.add_lane("Board", None).unwrap();
    let ticket = board.add("carry it", AddTicket::default()).unwrap();

    let placed = board
        .place(
            &ticket.id.to_string(),
            Some("Board"),
            Status::InProgress,
            usize::MAX,
        )
        .unwrap();
    assert_eq!(placed.lane.as_deref(), Some("Board"));
    assert_eq!(placed.status, Status::InProgress);
    assert!(!placed.done());

    // The store agrees, after one write (read it back through a fresh Board).
    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    let tickets = fresh.list_filter(tandem_core::domain::Filter::All).unwrap();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].status, Status::InProgress);
    assert_eq!(tickets[0].lane.as_deref(), Some("Board"));
}

#[test]
fn a_drop_into_in_review_persists_and_draws_in_the_in_review_column() {
    let sandbox = Sandbox::new("board-review");
    let board = sandbox.board();
    let ticket = board
        .add("waiting for a reader", AddTicket::default())
        .unwrap();

    let placed = board
        .place(&ticket.id.to_string(), None, Status::InReview, usize::MAX)
        .unwrap();
    assert_eq!(placed.status, Status::InReview);
    assert!(!placed.done());

    let fresh = Board::with_actor(sandbox.store(), tandem_core::store::Actor::Human);
    let reread = fresh
        .list_filter(tandem_core::domain::Filter::All)
        .unwrap()
        .remove(0);
    let rows = TicketBoard::rows(&[reread], &[]);
    assert_eq!(
        rows[0]
            .tickets(Status::InReview)
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["waiting for a reader"]
    );
    assert!(rows[0].tickets(Status::InProgress).is_empty());
    assert!(rows[0].tickets(Status::Done).is_empty());
}

#[test]
fn a_drop_appends_to_its_lane_by_default_and_takes_an_index_when_given_one() {
    let sandbox = Sandbox::new("board-index");
    let board = sandbox.board();
    board.add_lane("Board", None).unwrap();
    board
        .add(
            "one",
            AddTicket {
                lane: Some("Board"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    board
        .add(
            "two",
            AddTicket {
                lane: Some("Board"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let moving = board
        .add(
            "three",
            AddTicket {
                lane: Some("Board"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    // No index: the drop appends to the end of the lane.
    board
        .place(
            &moving.id.to_string(),
            Some("Board"),
            Status::Ready,
            usize::MAX,
        )
        .unwrap();
    assert_eq!(
        support::titles(&board, tandem_core::domain::Filter::All),
        ["two", "one", "three"]
    );

    // An index: the drop lands at that place in the lane.
    board
        .place(&moving.id.to_string(), Some("Board"), Status::Backlog, 0)
        .unwrap();
    assert_eq!(
        support::titles(&board, tandem_core::domain::Filter::All),
        ["three", "two", "one"]
    );
}

#[test]
fn dropping_into_done_stamps_completed_at_and_dropping_out_of_it_clears_it() {
    let sandbox = Sandbox::new("board-done");
    let board = sandbox.board();
    let ticket = board.add("finish it", AddTicket::default()).unwrap();

    let done = board
        .place(&ticket.id.to_string(), None, Status::Done, usize::MAX)
        .unwrap();
    assert!(done.done());
    assert!(done.completed_at.is_some());

    let back = board
        .place(&ticket.id.to_string(), None, Status::Ready, usize::MAX)
        .unwrap();
    assert!(!back.done());
    assert!(back.completed_at.is_none());
}

#[test]
fn a_drop_that_lands_the_ticket_where_it_already_was_changes_nothing() {
    let sandbox = Sandbox::new("board-noop");
    let board = sandbox.board();
    board.add("a", AddTicket::default()).unwrap();
    let moving = board.add("b", AddTicket::default()).unwrap();

    let same = board
        .place(&moving.id.to_string(), None, Status::Backlog, 0)
        .unwrap();
    assert_eq!(same.id, moving.id);
    assert_eq!(
        support::titles(&board, tandem_core::domain::Filter::All),
        ["b", "a"]
    );
}

#[test]
fn an_unknown_lane_or_id_is_rejected_and_the_store_is_left_alone() {
    let sandbox = Sandbox::new("board-rejects");
    let board = sandbox.board();
    let ticket = board.add("a", AddTicket::default()).unwrap();
    let before = board.list_filter(tandem_core::domain::Filter::All).unwrap();

    assert!(
        board
            .place(
                &ticket.id.to_string(),
                Some("Nope"),
                Status::Ready,
                usize::MAX
            )
            .is_err()
    );
    assert!(
        board
            .place(
                &uuid::Uuid::new_v4().to_string(),
                None,
                Status::Ready,
                usize::MAX
            )
            .is_err()
    );
    assert_eq!(
        board.list_filter(tandem_core::domain::Filter::All).unwrap(),
        before
    );
}

// MARK: - One transaction (the Swift `mutate`, `Store.swift:130-175`)

/// The read-modify-write is one transaction: the write lock is held before the
/// rows are read, so two writers over one home cannot hand out the same lane number
/// or write a stale row back over the other's.
#[test]
fn two_writers_over_one_home_keep_every_row_and_issue_every_ref_once() {
    let sandbox = Sandbox::new("board-writers");
    let board = sandbox.board();
    board.add_lane("Tandem", Some("TAN")).unwrap();
    let store = sandbox.store();

    std::thread::scope(|scope| {
        for group in ["a", "b"] {
            let store = store.clone();
            scope.spawn(move || {
                let board =
                    Board::with_actor(store, tandem_core::store::Actor::Agent(group.to_string()));
                for index in 0..20 {
                    board
                        .add(
                            &format!("{group}-{index}"),
                            AddTicket {
                                lane: Some("Tandem"),
                                ..AddTicket::default()
                            },
                        )
                        .unwrap_or_else(|error| panic!("{group}-{index} answered {error}"));
                }
            });
        }
    });

    let all = sandbox
        .board()
        .list_filter(tandem_core::domain::Filter::All)
        .unwrap();
    assert_eq!(all.len(), 40, "no writer lost a row");
    let mut refs: Vec<String> = all
        .iter()
        .filter_map(|ticket| ticket.reference.clone())
        .collect();
    assert_eq!(refs.len(), 40, "every row carries a ref");
    refs.sort();
    let mut expected: Vec<String> = (1..=40).map(|number| format!("TAN-{number}")).collect();
    expected.sort();
    assert_eq!(refs, expected, "every number is issued exactly once");
}

/// A writer's snapshot cannot be stale: adding a ticket shifts the stored order of
/// every ticket after it, and the write that carries the new order writes those
/// rows back. Read outside the transaction, that write reverts an edit another
/// writer made in between.
#[test]
fn a_concurrent_edit_is_not_reverted_by_another_writers_snapshot() {
    let sandbox = Sandbox::new("board-revert");
    let board = sandbox.board();
    board.add_lane("Tandem", Some("TAN")).unwrap();
    let edited = board
        .add(
            "original",
            AddTicket {
                lane: Some("Tandem"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let store = sandbox.store();
    let id = edited.id.to_string();

    std::thread::scope(|scope| {
        let editor = store.clone();
        let edited_id = id.clone();
        scope.spawn(move || {
            let board = Board::with_actor(
                editor,
                tandem_core::store::Actor::Agent("editor".to_string()),
            );
            for index in 0..20 {
                board
                    .set(
                        &edited_id,
                        tandem_core::domain::SetFields {
                            title: tandem_core::domain::Edit::Set(&format!("edited {index}")),
                            ..tandem_core::domain::SetFields::default()
                        },
                    )
                    .unwrap();
            }
        });
        let adder = store.clone();
        scope.spawn(move || {
            let board =
                Board::with_actor(adder, tandem_core::store::Actor::Agent("adder".to_string()));
            for index in 0..20 {
                board
                    .add(
                        &format!("added {index}"),
                        AddTicket {
                            lane: Some("Tandem"),
                            ..AddTicket::default()
                        },
                    )
                    .unwrap();
            }
        });
    });

    let reread = sandbox.board().ticket(&id).unwrap();
    assert!(
        reread.title.starts_with("edited "),
        "the concurrent edit was written back over: {}",
        reread.title
    );
}
