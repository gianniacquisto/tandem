//! Ticket references (ticket 6598E091), ported from `TicketRefTests.swift`: a lane
//! key plus a per-lane number, rendered `KEY-N` (`TAN-42`) and accepted wherever an
//! id is.
//!
//! The Swift suite drove the CLI for the contract; this one drives the board, which
//! is where the contract lives. The argv and stdout half belongs to PRT-9, and the
//! version-2 migration cases belonged to a store the port replaces with a fresh
//! schema (PRT-4).

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Filter, Status, TicketRef};

#[test]
fn a_lanes_tickets_are_numbered_and_the_ref_is_on_add_and_list() {
    let sandbox = Sandbox::new("ref-numbers");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    let first = board
        .add(
            "first",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let second = board
        .add(
            "second",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    assert_eq!(first.number, Some(1));
    assert_eq!(first.reference.as_deref(), Some("PRB-1"));
    assert_eq!(second.number, Some(2));
    assert_eq!(second.reference.as_deref(), Some("PRB-2"));

    let refs: Vec<String> = board
        .list_filter(Filter::All)
        .unwrap()
        .into_iter()
        .filter_map(|ticket| ticket.reference)
        .collect();
    let mut sorted = refs.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        ["PRB-1", "PRB-2"],
        "the list rows carry the ref too"
    );

    // The default area carries no number and no ref.
    let loose = board.add("loose", AddTicket::default()).unwrap();
    assert_eq!(loose.number, None);
    assert_eq!(loose.reference, None);
}

#[test]
fn an_absent_key_is_the_first_three_of_the_name_separators_stripped() {
    let sandbox = Sandbox::new("ref-keys");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    board.add_lane("Retro", None).unwrap();
    board.add_lane("code-rank", None).unwrap();

    let reference = |lane: &str| {
        board
            .add(
                "t",
                AddTicket {
                    lane: Some(lane),
                    ..AddTicket::default()
                },
            )
            .unwrap()
            .reference
    };
    assert_eq!(reference("Tandem").as_deref(), Some("TAN-1"));
    assert_eq!(reference("Retro").as_deref(), Some("RET-1"));
    assert_eq!(reference("code-rank").as_deref(), Some("COD-1"));
}

#[test]
fn a_key_already_in_use_is_refused_case_insensitively() {
    let sandbox = Sandbox::new("ref-duplicate");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    assert_eq!(
        board.add_lane("Pilot", Some("prb")),
        Err(DomainError::DuplicateLaneKey("PRB".to_string()))
    );

    // The failure names the key it cannot use.
    assert!(
        DomainError::DuplicateLaneKey("PRB".to_string())
            .message()
            .contains("PRB")
    );
    assert_eq!(
        board.add_lane("Bad", Some("P-R")),
        Err(DomainError::InvalidLaneKey("P-R".to_string())),
        "a key is alphanumerics only"
    );
}

#[test]
fn show_resolves_a_ref_in_either_case_and_an_unissued_number_names_the_lanes_range() {
    let sandbox = Sandbox::new("ref-resolve");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    let first = board
        .add(
            "first",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let second = board
        .add(
            "second",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    for reference in ["PRB-2", "prb-2"] {
        assert_eq!(
            board.ticket(reference).unwrap().id,
            second.id,
            "{reference} must resolve"
        );
    }

    // The full UUID still resolves.
    assert_eq!(board.ticket(&second.id.to_string()).unwrap().id, second.id);

    match board.ticket("PRB-99").unwrap_err() {
        DomainError::UnknownRef { reference, hint } => {
            assert_eq!(reference, "PRB-99");
            assert!(hint.contains("PRB-1"));
            assert!(
                hint.contains("PRB-2"),
                "the failure names the lane's real range"
            );
        }
        other => panic!("expected unknownRef, got {other:?}"),
    }

    match board.ticket("ZZZ-1").unwrap_err() {
        DomainError::UnknownRef { hint, .. } => assert!(hint.contains("ZZZ")),
        other => panic!("expected unknownRef, got {other:?}"),
    }

    // The first ticket is what its ref names: numbering follows creation.
    assert_eq!(board.ticket("PRB-1").unwrap().id, first.id);
}

#[test]
fn a_rename_keeps_the_key_and_every_ref_still_resolves() {
    let sandbox = Sandbox::new("ref-rename");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    board
        .add(
            "first",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    board.rename_lane("Probe", "Probe Two", None).unwrap();
    assert_eq!(
        board.rename_lane("Probe Two", "Probe Two", None).unwrap(),
        ["Probe Two"]
    );

    let shown = board.ticket("PRB-1").unwrap();
    assert_eq!(
        shown.lane.as_deref(),
        Some("Probe Two"),
        "the key survives the rename"
    );
}

#[test]
fn a_key_on_a_lane_with_a_numbered_ticket_is_refused_naming_its_refs() {
    let sandbox = Sandbox::new("ref-frozen");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    board
        .add(
            "first",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    match board
        .rename_lane("Probe", "Probe Two", Some("PBX"))
        .unwrap_err()
    {
        DomainError::FrozenLaneKey { key, lane, refs } => {
            assert_eq!(key, "PRB");
            assert_eq!(lane, "Probe");
            assert_eq!(
                refs,
                ["PRB-1"],
                "the failure names the refs that would move"
            );
        }
        other => panic!("expected frozenLaneKey, got {other:?}"),
    }
    assert_eq!(
        board.ticket("PRB-1").unwrap().title,
        "first",
        "nothing changed"
    );

    // An empty lane's key is not frozen: it may move.
    board.add_lane("Empty", None).unwrap();
    board
        .rename_lane("Empty", "Empty Two", Some("EMP"))
        .unwrap();
    assert_eq!(board.lanes().unwrap(), ["Probe", "Empty Two"]);
}

#[test]
fn a_move_between_lanes_assigns_the_target_lanes_next_number_and_drops_the_old_one() {
    let sandbox = Sandbox::new("ref-move");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    board.add_lane("Retro", Some("RET")).unwrap();
    let ticket = board
        .add(
            "moving",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(ticket.reference.as_deref(), Some("PRB-1"));

    let moved = board
        .move_ticket(&ticket.id.to_string(), Some("Retro"), usize::MAX)
        .unwrap();
    assert_eq!(moved.reference.as_deref(), Some("RET-1"));
    assert_eq!(moved.number, Some(1));

    // A move to the default area carries neither number nor ref.
    let loose = board
        .move_ticket(&ticket.id.to_string(), None, usize::MAX)
        .unwrap();
    assert_eq!(loose.reference, None);

    // And back in a lane it gets that lane's next number, not the one it had.
    board
        .add(
            "another",
            AddTicket {
                lane: Some("Retro"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let back = board
        .move_ticket(&ticket.id.to_string(), Some("Retro"), usize::MAX)
        .unwrap();
    assert_eq!(
        back.reference.as_deref(),
        Some("RET-3"),
        "RET-1 was issued, RET-2 is the other ticket, so the next is RET-3"
    );
}

#[test]
fn the_ref_parser_is_case_insensitive_on_the_key_and_digit_only_on_the_number() {
    assert_eq!(TicketRef::parse("prb-2"), Some(("PRB".to_string(), 2)));
    assert_eq!(TicketRef::parse(" PRB-2 "), Some(("PRB".to_string(), 2)));
    assert_eq!(TicketRef::parse("PRB-0"), None);
    assert_eq!(TicketRef::parse("PRB-x"), None);
    assert_eq!(TicketRef::parse("PRB"), None);
    assert_eq!(TicketRef::parse("not-a-uuid"), None);
    assert_eq!(TicketRef::render("TAN", 42), "TAN-42");
}

#[test]
fn a_done_ticket_keeps_its_ref_and_its_number() {
    let sandbox = Sandbox::new("ref-done");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    let ticket = board
        .add(
            "done soon",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let done = board.complete(&ticket.id.to_string()).unwrap();
    assert_eq!(done.reference.as_deref(), Some("PRB-1"));
    assert_eq!(done.status, Status::Done);
}
