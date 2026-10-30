//! The `lane` verb group's read, ported from `LaneVerbTests.swift`: `lane list` is
//! the lane discovery the board answers without pulling ticket rows. The write
//! verbs (`add`, `rename`, `delete`) each answer the list their write produced;
//! `list` answers the store's declared lanes, empty ones included.
//!
//! The Swift suite drove the binary and asserted its JSON, stderr and exit code.
//! That half is PRT-9's (the tandem grammar and the agent verbs); what is ported
//! here is the contract the verb answers with, at the board seam it reads.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Filter};

#[test]
fn lane_list_on_an_empty_store_is_an_empty_lane_list() {
    let sandbox = Sandbox::new("lane-verb-empty");
    let board = sandbox.board();
    assert_eq!(board.lanes().unwrap(), Vec::<String>::new());
}

#[test]
fn lane_list_prints_the_declared_lanes_empty_lanes_included_with_no_ticket_rows() {
    let sandbox = Sandbox::new("lane-verb-list");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    board.add_lane("Retro", None).unwrap();
    // A ticket in one lane must not change what `list` reports: the lanes come
    // from the store's lane list, not from ticket rows.
    let ticket = board.add("In a lane", AddTicket::default()).unwrap();
    board
        .move_ticket(&ticket.id.to_string(), Some("Tandem"), usize::MAX)
        .unwrap();

    assert_eq!(
        board.lanes().unwrap(),
        ["Tandem", "Retro"],
        "render order, the empty lane included"
    );
    // The read answers lanes, not tickets: one row per lane, and nothing else.
    assert_eq!(board.list_filter(Filter::All).unwrap().len(), 1);
}

#[test]
fn lane_list_reports_a_lane_a_hand_edit_leaves_on_a_ticket() {
    let sandbox = Sandbox::new("lane-verb-dangling");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    let ticket = board
        .add(
            "in Tandem",
            AddTicket {
                lane: Some("Tandem"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    // A rename is the only way a lane name changes in the port; a name a ticket
    // carries that the list has dropped is still drawn, because otherwise a row
    // could be hidden by deleting a name.
    board.rename_lane("Tandem", "Tandem Two", None).unwrap();
    assert_eq!(board.lanes().unwrap(), ["Tandem Two"]);
    assert_eq!(
        board
            .ticket(&ticket.id.to_string())
            .unwrap()
            .lane
            .as_deref(),
        Some("Tandem Two")
    );
}

#[test]
fn every_lane_write_answers_the_list_after_its_write() {
    let sandbox = Sandbox::new("lane-verb-writes");
    let board = sandbox.board();
    assert_eq!(board.add_lane("Tandem", None).unwrap(), ["Tandem"]);
    assert_eq!(
        board.rename_lane("Tandem", "Board", None).unwrap(),
        ["Board"]
    );
    assert_eq!(board.move_lane("Board", 0).unwrap(), ["Board"]);
    assert!(board.delete_lane("Board").unwrap().lanes.is_empty());
}
