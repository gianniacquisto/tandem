//! The drag arithmetic, ported from `OrderingTests.swift`: the index a drop names
//! must make the real `move` land the row where the human dropped it. Each case
//! asserts the resulting order after an actual move, so the math is pinned to the
//! operation it feeds rather than to itself.

mod support;

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use support::{Sandbox, all, titles};
use tandem_core::domain::{AddTicket, Board, Filter, Ticket, TicketOrdering};
use uuid::Uuid;

/// Drop `moving` on the `anchor` row and perform it.
fn drop_on(board: &Board, moving: &str, anchor: &str, after: bool) -> bool {
    let tickets = all(board);
    let (Some(moving_id), Some(anchor_id)) = (
        tickets
            .iter()
            .find(|ticket| ticket.title == moving)
            .map(|t| t.id),
        tickets
            .iter()
            .find(|ticket| ticket.title == anchor)
            .map(|t| t.id),
    ) else {
        return false;
    };
    let Some(index) = TicketOrdering::drop_index(&tickets, moving_id, anchor_id, after) else {
        return false;
    };
    let lane = tickets
        .iter()
        .find(|ticket| ticket.id == anchor_id)
        .and_then(|ticket| ticket.lane.clone());
    board
        .move_ticket(&moving_id.to_string(), lane.as_deref(), index)
        .expect("the move lands");
    true
}

#[test]
fn dropping_on_a_rows_lower_half_moves_the_ticket_just_past_it() {
    let sandbox = Sandbox::new("order-down");
    let board = sandbox.board();
    board.add("c", AddTicket::default()).unwrap();
    board.add("b", AddTicket::default()).unwrap();
    board.add("a", AddTicket::default()).unwrap();
    assert_eq!(titles(&board, Filter::All), ["a", "b", "c"]);

    drop_on(&board, "a", "c", true);
    assert_eq!(titles(&board, Filter::All), ["b", "c", "a"]);
}

#[test]
fn dropping_on_a_rows_upper_half_moves_the_ticket_just_above_it() {
    let sandbox = Sandbox::new("order-up");
    let board = sandbox.board();
    board.add("c", AddTicket::default()).unwrap();
    board.add("b", AddTicket::default()).unwrap();
    board.add("a", AddTicket::default()).unwrap();

    drop_on(&board, "a", "c", false);
    assert_eq!(titles(&board, Filter::All), ["b", "a", "c"]);
}

#[test]
fn a_row_dropped_on_itself_is_a_no_op() {
    let sandbox = Sandbox::new("order-self");
    let board = sandbox.board();
    board.add("b", AddTicket::default()).unwrap();
    board.add("a", AddTicket::default()).unwrap();

    let tickets = all(&board);
    let id = tickets[0].id;
    assert_eq!(TicketOrdering::drop_index(&tickets, id, id, true), None);
    assert_eq!(titles(&board, Filter::All), ["a", "b"]);
}

#[test]
fn dropping_a_default_ticket_onto_a_lane_row_files_it_into_that_lane() {
    let sandbox = Sandbox::new("order-into");
    let board = sandbox.board();
    board.add_lane("Work", None).unwrap();
    board.add("d1", AddTicket::default()).unwrap();
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
    // Stored: default [d1], Work [w2, w1].

    drop_on(&board, "d1", "w1", false);
    let tickets = all(&board);
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["w2", "d1", "w1"]
    );
    assert!(
        tickets
            .iter()
            .all(|ticket| ticket.lane.as_deref() == Some("Work"))
    );
}

#[test]
fn dropping_a_lane_ticket_onto_a_default_row_returns_it_to_the_default_area() {
    let sandbox = Sandbox::new("order-out");
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
    // Stored: default [d2, d1], Work [w1].

    drop_on(&board, "w1", "d1", true);
    let tickets = all(&board);
    assert_eq!(
        tickets
            .iter()
            .map(|ticket| ticket.title.as_str())
            .collect::<Vec<_>>(),
        ["d2", "d1", "w1"]
    );
    assert!(tickets.iter().all(|ticket| ticket.lane.is_none()));
}

#[test]
fn a_done_ticket_hidden_by_the_open_filter_still_anchors_the_drop_correctly() {
    let sandbox = Sandbox::new("order-hidden");
    let board = sandbox.board();
    board.add("c", AddTicket::default()).unwrap();
    let done = board.add("b", AddTicket::default()).unwrap();
    board.add("a", AddTicket::default()).unwrap();
    board.complete(&done.id.to_string()).unwrap();
    // Stored (all): [a, b(done), c]; visible under Open: [a, c].

    // The human sees [a, c] and drops a onto c's lower half.
    drop_on(&board, "a", "c", true);
    // a lands past c in the *stored* list, so the hidden b keeps its place above.
    assert_eq!(titles(&board, Filter::All), ["b", "c", "a"]);
    assert_eq!(titles(&board, Filter::Open), ["c", "a"]);
}

/// The drop-index arithmetic, in a golden both implementations run:
/// `Fixtures/port/ordering.json` is the case list, this suite reads it, and
/// `app/tests/board.test.ts` reads the same file through the renderer's port.
/// One rule, two languages, one set of answers (ADR 0008).
#[derive(Deserialize)]
struct Golden {
    cases: Vec<GoldenCase>,
}

#[derive(Deserialize)]
struct GoldenCase {
    name: String,
    tickets: Vec<GoldenTicket>,
    moving: String,
    anchor: String,
    after: bool,
    index: Option<usize>,
}

#[derive(Deserialize)]
struct GoldenTicket {
    id: String,
    lane: Option<String>,
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("the core crate sits two levels below the repo root")
        .join("Fixtures")
        .join("port")
        .join("ordering.json")
}

#[test]
fn the_drop_index_golden_is_the_rule_both_implementations_answer() {
    let path = golden_path();
    let golden: Golden = serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display())),
    )
    .expect("the ordering golden parses");
    assert!(!golden.cases.is_empty(), "the golden holds cases");

    for case in golden.cases {
        let tickets: Vec<Ticket> = case
            .tickets
            .iter()
            .map(|entry| {
                let mut ticket = Ticket::new("a ticket");
                ticket.id = Uuid::parse_str(&entry.id).expect("a golden id is a uuid");
                ticket.lane = entry.lane.clone();
                ticket
            })
            .collect();
        let moving = Uuid::parse_str(&case.moving).expect("a golden id is a uuid");
        let anchor = Uuid::parse_str(&case.anchor).expect("a golden id is a uuid");

        assert_eq!(
            TicketOrdering::drop_index(&tickets, moving, anchor, case.after),
            case.index,
            "{}",
            case.name
        );
    }
}
