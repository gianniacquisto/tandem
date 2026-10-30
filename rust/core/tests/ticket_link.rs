//! `set_link` and the additive `link` field, ported from `TicketLinkTests.swift`: a
//! ticket may point at a bikeshed note by bikeshed-root-relative path, GUI-only
//! (ADR 0010).

mod support;

use std::time::SystemTime;

use support::Sandbox;
use tandem_core::domain::{AddTicket, DomainError, Filter};

fn modified(sandbox: &Sandbox) -> SystemTime {
    std::fs::metadata(sandbox.database())
        .and_then(|meta| meta.modified())
        .expect("the database answers")
}

#[test]
fn set_link_stores_a_trimmed_bikeshed_relative_path_and_it_persists() {
    let sandbox = Sandbox::new("link-set");
    let board = sandbox.board();
    let added = board
        .add("Ship the bikeshed", AddTicket::default())
        .unwrap();
    let linked = board
        .set_link(&added.id.to_string(), Some("  projects/App.md  \n"))
        .unwrap();
    assert_eq!(linked.id, added.id);
    assert_eq!(linked.link.as_deref(), Some("projects/App.md"));

    // Re-read: the link is on disk, not just in the returned value.
    let persisted = support::all(&board)
        .into_iter()
        .find(|ticket| ticket.id == added.id)
        .unwrap();
    assert_eq!(persisted.link.as_deref(), Some("projects/App.md"));
    assert_eq!(persisted.title, "Ship the bikeshed");
}

#[test]
fn an_empty_or_whitespace_only_link_unlinks_the_ticket() {
    let sandbox = Sandbox::new("link-clear");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set_link(&added.id.to_string(), Some("Ideas.md"))
        .unwrap();
    assert_eq!(
        support::all(&board)
            .first()
            .and_then(|ticket| ticket.link.clone())
            .as_deref(),
        Some("Ideas.md")
    );

    let cleared = board
        .set_link(&added.id.to_string(), Some("   \n"))
        .unwrap();
    assert_eq!(cleared.link, None);
    assert_eq!(board.list_filter(Filter::All).unwrap()[0].link, None);
}

#[test]
fn set_link_to_none_clears_an_existing_link() {
    let sandbox = Sandbox::new("link-none");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set_link(&added.id.to_string(), Some("Ideas.md"))
        .unwrap();

    let cleared = board.set_link(&added.id.to_string(), None).unwrap();
    assert_eq!(cleared.link, None);
    assert_eq!(board.list_filter(Filter::All).unwrap()[0].link, None);
}

#[test]
fn setting_the_same_link_is_a_no_op_that_writes_nothing() {
    let sandbox = Sandbox::new("link-noop");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set_link(&added.id.to_string(), Some("Ideas.md"))
        .unwrap();
    let before = modified(&sandbox);

    let again = board
        .set_link(&added.id.to_string(), Some("Ideas.md"))
        .unwrap();
    assert_eq!(again.link.as_deref(), Some("Ideas.md"));
    assert_eq!(modified(&sandbox), before, "a no-op write writes nothing");

    // An unchanged none to none is also a no-op.
    board.set_link(&added.id.to_string(), None).unwrap();
    let before = modified(&sandbox);
    let unlinked = board.set_link(&added.id.to_string(), None).unwrap();
    assert_eq!(unlinked.link, None);
    assert_eq!(modified(&sandbox), before);
}

#[test]
fn set_link_with_an_unknown_or_non_uuid_id_is_the_unknown_id_failure() {
    let sandbox = Sandbox::new("link-unknown");
    let board = sandbox.board();
    board.add("A", AddTicket::default()).unwrap();

    for id in [uuid::Uuid::new_v4().to_string(), "not-a-uuid".to_string()] {
        assert!(matches!(
            board.set_link(&id, Some("Ideas.md")),
            Err(DomainError::UnknownId { .. })
        ));
    }
}

#[test]
fn the_link_round_trips_through_the_store_present_and_cleared() {
    let sandbox = Sandbox::new("link-roundtrip");
    let board = sandbox.board();
    let added = board.add("Task", AddTicket::default()).unwrap();
    board
        .set_link(&added.id.to_string(), Some("Ideas.md"))
        .unwrap();

    // Present: a real string, read back by a fresh Board.
    assert_eq!(
        sandbox.board().list_filter(Filter::All).unwrap()[0]
            .link
            .as_deref(),
        Some("Ideas.md")
    );

    // Cleared: none, and the clear persists.
    board.set_link(&added.id.to_string(), None).unwrap();
    assert_eq!(
        sandbox.board().list_filter(Filter::All).unwrap()[0].link,
        None
    );
}
