//! The board's whole lifecycle against the committed fixture home
//! (`Fixtures/port/Home`), which is what the ticket's "done when" names.
//!
//! The fixture is content, not scratch: no run writes into it, so this case copies
//! the whole tree into a sandbox and points the board at the copy. The copy proves
//! two things the temp-home suites do not: the board stands up beside a real
//! bikeshed, and every verb the ticket lists works on one board, in order.

use std::fs;
use std::path::{Path, PathBuf};

use tandem_core::domain::{
    AddTicket, Board, CommentRead, CompletionUndo, DescriptionBody, DomainError, Edit, Filter,
    SetFields, Status, TicketBoard,
};
use tandem_core::store::{Actor, Home, Store};

fn fixture_home() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // <repo>/rust/core
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("{} sits two levels below the repo root", manifest.display()));
    let path = root.join("Fixtures").join("port").join("Home");
    assert!(
        path.join("bikeshed").is_dir(),
        "the fixture home is committed at Fixtures/port/Home ({})",
        path.display()
    );
    path
}

/// A copy of `from` at `to`, files and directories, nothing else.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("the copy's directory is creatable");
    for entry in fs::read_dir(from).expect("the fixture reads") {
        let entry = entry.expect("an entry reads");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("the type reads").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("a fixture file copies");
        }
    }
}

fn sandbox_copy(name: &str) -> (PathBuf, Board) {
    let root = std::env::temp_dir().join(format!("tandem-fixture-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    copy_tree(&fixture_home(), &home);
    let home = Home::resolve(Some(&home), None, &root.join("user"));
    let board = Board::with_actor(Store::new(home), Actor::Human);
    (root, board)
}

#[test]
fn the_board_lives_a_whole_life_against_the_fixture_home() {
    let (root, board) = sandbox_copy("lifecycle");
    let fixture = fixture_home();

    // Created, in a lane that issues a ref.
    board.add_lane("Port", Some("PRT")).unwrap();
    let ticket = board
        .add(
            "Port the board",
            AddTicket {
                lane: Some("Port"),
                status: Status::Ready,
                assignee: Some("builder"),
                description: Some("## Why\n\nBecause the board is the work."),
                origin: "human",
            },
        )
        .unwrap();
    assert_eq!(ticket.reference.as_deref(), Some("PRT-1"));

    // Re-referenced: the ref resolves wherever an id is accepted.
    assert_eq!(board.ticket("prt-1").unwrap().id, ticket.id);

    // Completed and un-completed, through the undo a completion raises.
    let done = board.complete("PRT-1").unwrap();
    assert!(done.done());
    assert!(done.completed_at.is_some());
    let undone = board
        .undo(&CompletionUndo::new(ticket.id, ticket.title.clone(), 1_000))
        .unwrap();
    assert_eq!(undone.status, Status::Backlog);
    assert!(undone.completed_at.is_none());

    // Blocked and unblocked, against a second ticket in the same lane.
    let blocker = board
        .add(
            "Land the store",
            AddTicket {
                lane: Some("Port"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(blocker.reference.as_deref(), Some("PRT-2"));
    let blocked = board.block("PRT-1", "PRT-2").unwrap();
    assert!(blocked.blocked());
    assert_eq!(blocked.blocked_by[0].reference.as_deref(), Some("PRT-2"));
    let unblocked = board.unblock("PRT-1", None).unwrap();
    assert!(!unblocked.blocked());

    // Ordered within a lane, and moved across lanes and back.
    board.add_lane("Memory", Some("MEM")).unwrap();
    board.move_ticket("PRT-2", Some("Port"), 0).unwrap();
    let moved = board
        .move_ticket("PRT-2", Some("Memory"), usize::MAX)
        .unwrap();
    assert_eq!(moved.lane.as_deref(), Some("Memory"));
    assert_eq!(
        moved.reference.as_deref(),
        Some("MEM-1"),
        "the target lane's number"
    );
    // The ref moved with the lane, so the old handle names nothing: the ticket is
    // resolved by its id, and it takes the target lane's next number.
    assert!(matches!(
        board.ticket("PRT-2"),
        Err(DomainError::UnknownRef { .. })
    ));
    let back = board
        .move_ticket(&blocker.id.to_string(), Some("Port"), usize::MAX)
        .unwrap();
    assert_eq!(back.reference.as_deref(), Some("PRT-3"));

    // Commented on, and read back through the app's own read path.
    let comment = board.comment_write("PRT-1", "the first note").unwrap();
    assert_eq!(comment.author, "human");
    match board.comment_read("PRT-1") {
        CommentRead::List(listed) => {
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].body, "the first note");
        }
        other => panic!("expected the log, got {other:?}"),
    }

    // Answered: the block lands, the pair moves, and the inbox stops asking.
    board
        .set(
            "PRT-1",
            SetFields {
                status: Edit::Set(Status::OnHold),
                assignee: Edit::Set(Some("human")),
                ..SetFields::default()
            },
        )
        .unwrap();
    assert_eq!(
        tandem_core::domain::TicketQuery::inbox()
            .apply(&board.list(&Filter::All.query()).unwrap())
            .len(),
        1
    );
    let answered = board
        .answer("PRT-1", Status::Ready, None, "A: port it.", 1_762_000_000)
        .unwrap();
    assert_eq!(answered.assignee, None);
    assert!(DescriptionBody::answered(answered.description.as_deref()));
    assert!(
        tandem_core::domain::TicketQuery::inbox()
            .apply(&board.list(&Filter::All.query()).unwrap())
            .is_empty(),
        "the hold is closed in the same write"
    );

    // Archived, and still on the board behind the archive.
    board.archive(&blocker.id.to_string()).unwrap();
    assert!(
        board
            .list_filter(Filter::Open)
            .unwrap()
            .iter()
            .all(|ticket| !ticket.archived)
    );
    assert!(
        board
            .list_filter(Filter::All)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == blocker.id && ticket.archived)
    );

    // The board draws what it holds: one row per lane, the default area first.
    let rows = TicketBoard::rows(
        &board.list_filter(Filter::All).unwrap(),
        &board.lanes().unwrap(),
    );
    assert_eq!(rows.first().and_then(|row| row.key.clone()), None);
    assert!(rows.iter().any(|row| row.key.as_deref() == Some("Port")));

    // The committed fixture is content, not scratch: not a byte of it moved.
    assert!(
        !fixture.join("tandem.db").exists(),
        "no run writes into the fixture home"
    );
    assert!(!fixture.join("tandem.db.bak").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unknown_ref_against_the_fixture_home_names_the_range_it_does_have() {
    let (root, board) = sandbox_copy("unknown-ref");
    board.add_lane("Port", Some("PRT")).unwrap();
    board
        .add(
            "one",
            AddTicket {
                lane: Some("Port"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    match board.ticket("PRT-2").unwrap_err() {
        DomainError::UnknownRef { hint, .. } => {
            assert!(hint.contains("PRT-1"), "the real range: {hint}");
        }
        other => panic!("expected unknownRef, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&root);
}
