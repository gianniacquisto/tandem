//! Ticket dependencies (TAN-232), ported from `TicketDependencyTests.swift`: an
//! edge in `ticket_dependencies` holds the UUID of the ticket a ticket waits on, the
//! derived `blocked`/`blocked_by`/`blocks` triple reads it from both ends, and
//! `block`/`unblock` are the only writes.

mod support;

use std::collections::HashMap;
use std::time::SystemTime;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, DescriptionBody, DomainError, Filter, Ticket};

/// A board with one lane (`Tandem`, so every ticket carries a ref) and the tickets
/// `titles` in that lane, in add order. Returned by title.
fn store(titles: &[&str]) -> (Sandbox, Board, HashMap<String, Ticket>) {
    let sandbox = Sandbox::new("deps");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    let mut made = HashMap::new();
    // Added in the order given, so `titles[0]` carries `TAN-1`: the number follows
    // the write, not the row's place on the board.
    for title in titles {
        let ticket = board
            .add(
                title,
                AddTicket {
                    lane: Some("Tandem"),
                    ..AddTicket::default()
                },
            )
            .unwrap();
        made.insert((*title).to_string(), ticket);
    }
    (sandbox, board, made)
}

fn reference(tickets: &HashMap<String, Ticket>, title: &str) -> String {
    tickets[title]
        .reference
        .clone()
        .expect("the ticket carries a ref")
}

fn modified(sandbox: &Sandbox) -> SystemTime {
    std::fs::metadata(sandbox.database())
        .and_then(|meta| meta.modified())
        .expect("the database answers")
}

#[test]
fn block_writes_one_edge_and_it_survives_a_re_read() {
    let (sandbox, board, made) = store(&["waits", "finishes"]);
    let result = board
        .block(&reference(&made, "waits"), &reference(&made, "finishes"))
        .unwrap();
    assert!(result.blocked());
    assert_eq!(
        result
            .blocked_by
            .iter()
            .map(|edge| edge.reference.clone())
            .collect::<Vec<_>>(),
        [Some("TAN-2".to_string())]
    );
    assert_eq!(
        result
            .blocked_by
            .iter()
            .map(|edge| edge.title.as_str())
            .collect::<Vec<_>>(),
        ["finishes"]
    );
    assert_eq!(
        result
            .blocked_by
            .iter()
            .map(|edge| edge.status)
            .collect::<Vec<_>>(),
        [tandem_core::domain::Status::Backlog]
    );

    // A fresh board reads the edge from the database, not from the result.
    let fresh = sandbox.board();
    let reread = fresh.ticket(&reference(&made, "waits")).unwrap();
    assert!(reread.blocked());
    assert_eq!(reread.blocked_by.len(), 1);
    assert_eq!(reread.blocked_by[0].id, made["finishes"].id);
}

#[test]
fn block_twice_writes_one_edge_and_is_a_no_op() {
    let (sandbox, board, made) = store(&["waits", "finishes"]);
    let waits = reference(&made, "waits");
    let finishes = reference(&made, "finishes");

    board.block(&waits, &finishes).unwrap();
    let before = modified(&sandbox);

    let again = board.block(&waits, &finishes).unwrap();
    assert_eq!(again.blocked_by.len(), 1, "one edge, not two");
    assert_eq!(modified(&sandbox), before, "a no-op write writes nothing");
}

#[test]
fn a_ticket_may_block_on_two_and_blocked_by_keeps_the_order_they_were_set_in() {
    let (sandbox, board, made) = store(&["waits", "first", "second"]);
    let waits = reference(&made, "waits");

    board.block(&waits, &reference(&made, "first")).unwrap();
    let two = board.block(&waits, &reference(&made, "second")).unwrap();
    assert_eq!(
        two.blocked_by
            .iter()
            .map(|edge| edge.reference.clone())
            .collect::<Vec<_>>(),
        [Some("TAN-2".to_string()), Some("TAN-3".to_string())]
    );

    // The order is the write's, and a re-read agrees.
    let reread = sandbox.board().ticket(&waits).unwrap();
    assert_eq!(
        reread
            .blocked_by
            .iter()
            .map(|edge| edge.reference.clone())
            .collect::<Vec<_>>(),
        [Some("TAN-2".to_string()), Some("TAN-3".to_string())]
    );
}

#[test]
fn block_accepts_a_ref_and_a_uuid_for_the_same_blocker() {
    let (sandbox, board, made) = store(&["waits", "finishes"]);
    let blocker = made["finishes"].clone();

    board
        .block(&reference(&made, "waits"), &blocker.id.to_string())
        .unwrap();
    assert_eq!(
        board
            .ticket(&made["waits"].id.to_string())
            .unwrap()
            .blocked_by
            .len(),
        1
    );

    // The other spelling names the same edge, so the second write is the no-op.
    let same = board
        .block(&reference(&made, "waits"), &reference(&made, "finishes"))
        .unwrap();
    assert_eq!(
        same.blocked_by
            .iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>(),
        [blocker.id]
    );
    drop(sandbox);
}

#[test]
fn a_ticket_cannot_block_on_itself() {
    let (_sandbox, board, made) = store(&["alone"]);
    let reference = reference(&made, "alone");

    assert_eq!(
        board.block(&reference, &reference),
        Err(DomainError::TicketBlocksItself {
            reference: "TAN-1".to_string()
        })
    );
    assert_eq!(
        board.block(&reference, &reference).unwrap_err().message(),
        "TAN-1 cannot block on itself. Name the ticket that has to finish first."
    );
    assert!(
        board.ticket(&reference).unwrap().blocked_by.is_empty(),
        "nothing was written"
    );
}

#[test]
fn an_edge_that_would_close_a_cycle_is_refused_and_names_the_path() {
    let (_sandbox, board, made) = store(&["a", "b", "c"]);
    let a = reference(&made, "a");
    let b = reference(&made, "b");
    let c = reference(&made, "c");

    board.block(&a, &b).unwrap();
    board.block(&b, &c).unwrap();

    let error = board.block(&c, &a).unwrap_err();
    assert_eq!(
        error,
        DomainError::TicketCycle {
            path: vec![
                "TAN-3".to_string(),
                "TAN-1".to_string(),
                "TAN-2".to_string(),
                "TAN-3".to_string(),
            ]
        }
    );
    assert_eq!(
        error.message(),
        "That edge would close a cycle: TAN-3 -> TAN-1 -> TAN-2 -> TAN-3. \
         Two tickets cannot wait on each other; unblock one of them first."
    );
    assert!(
        board.ticket(&c).unwrap().blocked_by.is_empty(),
        "nothing was written"
    );
}

#[test]
fn a_done_blocker_is_refused() {
    let (_sandbox, board, made) = store(&["waits", "finished"]);
    board.complete(&reference(&made, "finished")).unwrap();

    let error = board
        .block(&reference(&made, "waits"), &reference(&made, "finished"))
        .unwrap_err();
    assert_eq!(
        error.message(),
        "TAN-2 is done, so the block would lift the moment it is written. \
         Block on the ticket that is still open."
    );
    assert!(
        board
            .ticket(&reference(&made, "waits"))
            .unwrap()
            .blocked_by
            .is_empty()
    );
}

#[test]
fn an_archived_blocker_is_refused() {
    let (_sandbox, board, made) = store(&["waits", "filed away"]);
    board.archive(&reference(&made, "filed away")).unwrap();

    let error = board
        .block(&reference(&made, "waits"), &reference(&made, "filed away"))
        .unwrap_err();
    assert_eq!(
        error.message(),
        "TAN-2 is archived, so the block could never be seen to lift. \
         Bring it back to the board first."
    );
    assert!(
        board
            .ticket(&reference(&made, "waits"))
            .unwrap()
            .blocked_by
            .is_empty()
    );
}

#[test]
fn an_unknown_handle_is_refused_with_the_boards_own_ref_message() {
    let (_sandbox, board, made) = store(&["waits", "finishes"]);
    let waits = reference(&made, "waits");

    let text = board.block(&waits, "TAN-99").unwrap_err().message();
    assert!(text.starts_with("Unknown ticket ref \"TAN-99\"."));
    assert!(
        text.contains("has issued TAN-1 to TAN-2"),
        "the lane's real range, not a new one"
    );

    // An empty handle is the unknown id the board already answers for.
    let empty = board.block(&waits, "").unwrap_err().message();
    assert!(empty.starts_with("Unknown ticket id \"\"."));
}

#[test]
fn completing_the_blocker_lifts_the_block_with_no_write() {
    let (_sandbox, board, made) = store(&["waits", "finishes"]);
    let waits = made["waits"].clone();
    board
        .block(&reference(&made, "waits"), &reference(&made, "finishes"))
        .unwrap();

    board.complete(&reference(&made, "finishes")).unwrap();

    let dependent = board.ticket(&reference(&made, "waits")).unwrap();
    assert!(
        !dependent.blocked(),
        "the block lifts without a write to the dependent"
    );
    assert_eq!(
        dependent
            .blocked_by
            .iter()
            .map(|edge| edge.status)
            .collect::<Vec<_>>(),
        [tandem_core::domain::Status::Done],
        "the edge stays as history"
    );
    // The dependent's own row is untouched by the blocker's completion.
    assert_eq!(dependent.title, waits.title);
    assert_eq!(dependent.status, waits.status);
    assert_eq!(dependent.assignee, waits.assignee);
    assert_eq!(dependent.lane, waits.lane);
    assert_eq!(dependent.number, waits.number);
    assert_eq!(dependent.created_at, waits.created_at);
    assert_eq!(dependent.completed_at, waits.completed_at);
    assert_eq!(dependent.archived, waits.archived);
}

#[test]
fn blocks_names_the_dependents_on_the_far_side_in_stored_order() {
    let (_sandbox, board, made) = store(&["one", "two", "blocker"]);
    board
        .block(&reference(&made, "one"), &reference(&made, "blocker"))
        .unwrap();
    board
        .block(&reference(&made, "two"), &reference(&made, "blocker"))
        .unwrap();

    let far = board.ticket(&reference(&made, "blocker")).unwrap();
    assert!(!far.blocked(), "holding others up is not being blocked");
    // The far side has no order of its own to store, so it reads in the board's
    // order, the one order every read already follows.
    let stored: Vec<Option<String>> = board
        .list_filter(Filter::All)
        .unwrap()
        .into_iter()
        .filter(|ticket| {
            ticket
                .blocked_by
                .iter()
                .any(|edge| edge.id == made["blocker"].id)
        })
        .map(|ticket| ticket.reference)
        .collect();
    assert_eq!(
        far.blocks
            .iter()
            .map(|edge| edge.reference.clone())
            .collect::<Vec<_>>(),
        stored
    );
    let mut references: Vec<String> = far
        .blocks
        .iter()
        .filter_map(|edge| edge.reference.clone())
        .collect();
    references.sort();
    assert_eq!(references, ["TAN-1", "TAN-2"]);
    let mut titles: Vec<String> = far.blocks.iter().map(|edge| edge.title.clone()).collect();
    titles.sort();
    assert_eq!(titles, ["one", "two"]);
    assert!(
        far.blocks
            .iter()
            .all(|edge| edge.status == tandem_core::domain::Status::Backlog)
    );
}

#[test]
fn unblock_on_clears_one_edge_and_unblock_alone_clears_every_edge() {
    let (_sandbox, board, made) = store(&["waits", "first", "second"]);
    let waits = reference(&made, "waits");
    board.block(&waits, &reference(&made, "first")).unwrap();
    board.block(&waits, &reference(&made, "second")).unwrap();

    let one = board
        .unblock(&waits, Some(&reference(&made, "first")))
        .unwrap();
    assert_eq!(
        one.blocked_by
            .iter()
            .map(|edge| edge.reference.clone())
            .collect::<Vec<_>>(),
        [Some("TAN-3".to_string())]
    );

    let none = board.unblock(&waits, None).unwrap();
    assert!(none.blocked_by.is_empty());
    assert!(board.ticket(&waits).unwrap().blocked_by.is_empty());
    assert!(
        board
            .ticket(&reference(&made, "first"))
            .unwrap()
            .blocks
            .is_empty(),
        "the far side lost the edge with the near side"
    );
}

#[test]
fn unblock_on_an_unblocked_ticket_is_a_no_op() {
    let (_sandbox, board, made) = store(&["waits", "finishes"]);
    let waits = reference(&made, "waits");

    let result = board.unblock(&waits, None).unwrap();
    assert!(result.blocked_by.is_empty());
    assert!(!result.blocked());

    // A named edge that is not there is a no-op too.
    let named = board
        .unblock(&waits, Some(&reference(&made, "finishes")))
        .unwrap();
    assert!(named.blocked_by.is_empty());
}

#[test]
fn deleting_a_ticket_removes_its_edges_on_both_ends() {
    let (_sandbox, board, made) = store(&["one", "two", "blocker"]);
    board
        .block(&reference(&made, "one"), &reference(&made, "blocker"))
        .unwrap();
    board
        .block(&reference(&made, "two"), &reference(&made, "blocker"))
        .unwrap();

    board.delete(&reference(&made, "blocker")).unwrap();

    assert!(
        board
            .ticket(&reference(&made, "one"))
            .unwrap()
            .blocked_by
            .is_empty(),
        "the near side dropped the edge"
    );
    assert!(
        board
            .ticket(&reference(&made, "two"))
            .unwrap()
            .blocked_by
            .is_empty()
    );
}

#[test]
fn a_ticket_moved_to_another_lane_keeps_its_edge_and_the_ref_follows_the_lane_key() {
    let (_sandbox, board, made) = store(&["waits", "finishes"]);
    board.add_lane("Fleet", None).unwrap();
    board
        .block(&reference(&made, "waits"), &reference(&made, "finishes"))
        .unwrap();

    board
        .move_ticket(&reference(&made, "finishes"), Some("Fleet"), usize::MAX)
        .unwrap();

    let dependent = board.ticket(&reference(&made, "waits")).unwrap();
    assert_eq!(
        dependent.blocked_by.len(),
        1,
        "the UUID is the key, so the edge survived"
    );
    assert_eq!(dependent.blocked_by[0].id, made["finishes"].id);
    assert_eq!(
        dependent.blocked_by[0].reference.as_deref(),
        Some("FLE-1"),
        "the ref is the display, and it moved"
    );
    assert!(dependent.blocked());
}

#[test]
fn blocked_reason_reads_the_blocked_on_section_and_is_none_without_one() {
    let body = "The real state of this ticket.\n\n\
                ## Blocked on\n\n\
                TAN-210 has to land first.\n\n\
                ## Done when\n\n\
                The block lifts.";
    assert_eq!(
        DescriptionBody::blocked_reason(Some(body)).as_deref(),
        Some("TAN-210 has to land first.")
    );
    assert_eq!(
        DescriptionBody::blocked_reason(Some("No section here.")),
        None
    );
    assert_eq!(DescriptionBody::blocked_reason(None), None);
    assert_eq!(
        DescriptionBody::blocked_reason(Some("## Blocked on\n\n\n## Next\n\ntext")),
        None,
        "an empty section is no reason"
    );
}

#[test]
fn a_body_that_mentions_the_heading_inline_is_not_a_reason() {
    let body = "This ticket's `## Blocked on` line must become data, not stay prose.";
    assert_eq!(DescriptionBody::blocked_reason(Some(body)), None);
}

#[test]
fn the_tickets_blocked_reason_is_the_bodys_section_read_not_copied() {
    let sandbox = Sandbox::new("deps-reason");
    let board = sandbox.board();
    board.add_lane("Tandem", None).unwrap();
    board
        .add(
            "waits",
            AddTicket {
                lane: Some("Tandem"),
                description: Some("## Blocked on\n\nTAN-210 lands first."),
                ..AddTicket::default()
            },
        )
        .unwrap();
    let finished = board
        .add(
            "finishes",
            AddTicket {
                lane: Some("Tandem"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    board
        .block("TAN-1", &finished.reference.clone().unwrap())
        .unwrap();
    assert_eq!(
        board.ticket("TAN-1").unwrap().blocked_reason().as_deref(),
        Some("TAN-210 lands first.")
    );
}
