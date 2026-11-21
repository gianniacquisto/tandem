//! COR-9: a list, a show and a board draw read the rows they answer with, not the
//! whole board.
//!
//! The probe is the store itself. Rows the read must not touch are poisoned: a
//! column that holds a BLOB cannot be read as text, so a read that selects a
//! poisoned title or body fails loudly. A read that succeeds over a poisoned
//! archive never asked for it.

mod support;

use support::Sandbox;
use tandem_core::domain::{
    AddTicket, Archived, AssigneeAxis, Board, LabelAxis, Status, TicketQuery,
};
use tandem_core::wire::TicketSummary;

const BIG: usize = 4096;

fn poison(sandbox: &Sandbox, sql: &str) {
    let connection = rusqlite::Connection::open(sandbox.database()).expect("the database opens");
    connection.execute_batch(sql).expect("the poison applies");
}

/// A board with a live ticket, a live ticket blocked by an archived one, and a
/// pile of archived tickets carrying large bodies.
fn seeded(sandbox: &Sandbox) -> Board {
    let board = sandbox.board();
    board.add_project("Probe", Some("PRB")).unwrap();
    board.add_label("Probe", Some("Probe")).unwrap();
    let in_probe = |description: Option<&'static str>| AddTicket {
        label: Some("Probe"),
        description,
        ..AddTicket::default()
    };
    board
        .add(
            "live one",
            in_probe(Some("h\u{e9}llo \u{1F600} w\u{f6}rld")),
        )
        .unwrap();
    board.add("live two", in_probe(None)).unwrap();
    board.add("old blocker", in_probe(None)).unwrap();
    board.block("PRB-2", "PRB-3").unwrap();
    board.archive("PRB-3").unwrap();
    let big = "x".repeat(BIG);
    for n in 0..20 {
        board
            .add(
                &format!("archived {n}"),
                AddTicket {
                    label: Some("Probe"),
                    status: Status::Done,
                    description: Some(&big),
                    ..AddTicket::default()
                },
            )
            .unwrap();
    }
    board.archive_done().unwrap();
    board
}

#[test]
fn a_default_list_and_a_board_draw_read_no_archived_row_and_no_body() {
    let sandbox = Sandbox::new("sql-list");
    let board = seeded(&sandbox);
    // Every body is unreadable as text, and every archived row but the blocker
    // the live ticket waits on is unreadable whole.
    poison(
        &sandbox,
        "UPDATE tickets SET description = x'FF' WHERE description IS NOT NULL",
    );
    poison(
        &sandbox,
        "UPDATE tickets SET title = x'FF' WHERE archived = 1 AND title LIKE 'archived %'",
    );

    // The board page and `ticket.list` both read through `list_snapshot`.
    let (tickets, _) = board.list_snapshot(&TicketQuery::default()).unwrap();
    let titles: Vec<&str> = tickets.iter().map(|ticket| ticket.title.as_str()).collect();
    assert_eq!(titles, ["live two", "live one"]);
    let blocked = tickets.iter().find(|t| t.title == "live two").unwrap();
    assert!(blocked.description.is_none(), "no body is read for a list");
    assert_eq!(
        blocked.blocked_by.len(),
        1,
        "the archived blocker is still named"
    );
    assert_eq!(blocked.blocked_by[0].title, "old blocker");
}

#[test]
fn a_show_reads_the_ticket_and_the_far_end_of_its_edges_only() {
    let sandbox = Sandbox::new("sql-show");
    let board = seeded(&sandbox);
    poison(
        &sandbox,
        "UPDATE tickets SET title = x'FF', description = x'FF' WHERE title LIKE 'archived %'",
    );
    poison(
        &sandbox,
        "UPDATE tickets SET description = x'FF' WHERE title = 'live one'",
    );

    let mut id = String::new();
    for handle in ["PRB-2", "prb-2"] {
        let (ticket, _) = board.ticket_snapshot(handle).unwrap();
        assert_eq!(ticket.title, "live two");
        assert_eq!(ticket.blocked_by[0].title, "old blocker");
        id = ticket.id.to_string();
    }
    let (ticket, _) = board.ticket_snapshot(&id).unwrap();
    assert_eq!(ticket.reference.as_deref(), Some("PRB-2"));
    assert!(board.ticket_snapshot("PRB-999").is_err());
}

#[test]
fn the_narrowed_reads_answer_exactly_what_the_whole_board_answers() {
    let sandbox = Sandbox::new("sql-same");
    let board = seeded(&sandbox);
    board
        .add(
            "loose note",
            AddTicket {
                assignee: Some("human"),
                ..support::ticket()
            },
        )
        .unwrap();
    let queries = [
        TicketQuery::default(),
        TicketQuery {
            archived: Archived::Included,
            ..TicketQuery::default()
        },
        TicketQuery {
            archived: Archived::Only,
            ..TicketQuery::default()
        },
        TicketQuery {
            label: LabelAxis::Unnamed,
            ..TicketQuery::default()
        },
        TicketQuery {
            label: LabelAxis::Named("Probe".into()),
            ..TicketQuery::default()
        },
        TicketQuery {
            assignee: AssigneeAxis::Named("human".into()),
            ..TicketQuery::default()
        },
        TicketQuery {
            assignee: AssigneeAxis::Unassigned,
            ..TicketQuery::default()
        },
        TicketQuery {
            text: Some("W\u{d6}RLD".into()),
            ..TicketQuery::default()
        },
        TicketQuery {
            blocked_only: true,
            ..TicketQuery::default()
        },
    ];
    for query in queries {
        let narrowed: Vec<TicketSummary> = board
            .list_snapshot(&query)
            .unwrap()
            .0
            .iter()
            .map(TicketSummary::new)
            .collect();
        let whole: Vec<TicketSummary> = board
            .list(&query)
            .unwrap()
            .iter()
            .map(TicketSummary::new)
            .collect();
        assert_eq!(narrowed, whole, "{query:?}");
    }
    // A non-ASCII body counts characters, as the wire always has.
    let (live, _) = board.list_snapshot(&TicketQuery::default()).unwrap();
    let one = live
        .iter()
        .find(|ticket| ticket.title == "live one")
        .unwrap();
    assert_eq!(TicketSummary::new(one).description_length, 13);
    let (shown, _) = board.ticket_snapshot("PRB-1").unwrap();
    assert_eq!(shown, board.ticket("PRB-1").unwrap());
}
