//! The comments log (ADR 0017's 2026-10-20 amendment, re-backed on the database by
//! ADR 0031), ported from `CommentsTests.swift` and its two app-path suites: `append`
//! and `list` over the `comments` table, the app's read and write seams, and the
//! author field the minutes rule names.
//!
//! The log is append-only: no edit, no delete, and a deleted ticket's log stays.
//!
//! Left behind with the legacy sidecar: the `comments/<id>.jsonl` cases (the damaged
//! line, the one-time import, the read fallback) and the CLI byte-identity case. The
//! port has one log, the table; the one-off import is PRT-18's.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, CommentRead, DomainError, Edit, SetFields};

fn board_with_ticket() -> (Sandbox, Board, uuid::Uuid) {
    let sandbox = Sandbox::new("comments");
    let board = sandbox.board();
    let ticket = board.add("A ticket", AddTicket::default()).unwrap();
    (sandbox, board, ticket.id)
}

#[test]
fn the_first_append_records_the_comment_in_the_store_and_list_reads_it_back() {
    let (_sandbox, board, id) = board_with_ticket();
    let result = board
        .append_comment(&id.to_string(), "builder", "did you mean X?")
        .unwrap();
    assert_eq!(result.author, "builder");
    assert_eq!(result.body, "did you mean X?");
    assert!(result.at > 0, "the time is recorded");

    let listed = board.comments(&id.to_string()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], result);
}

#[test]
fn comments_read_back_in_append_order() {
    let (_sandbox, board, id) = board_with_ticket();
    for body in ["first", "second", "third"] {
        board
            .append_comment(&id.to_string(), "builder", body)
            .unwrap();
    }
    let listed = board.comments(&id.to_string()).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
}

#[test]
fn an_empty_or_whitespace_only_body_is_rejected_and_nothing_is_written() {
    let (_sandbox, board, id) = board_with_ticket();
    for body in ["", "   ", "\n  \n"] {
        assert_eq!(
            board.append_comment(&id.to_string(), "builder", body),
            Err(DomainError::EmptyCommentBody),
            "body {body:?}"
        );
    }
    assert!(board.comments(&id.to_string()).unwrap().is_empty());
}

#[test]
fn an_unknown_id_on_append_fails_with_the_open_list_like_complete() {
    let (_sandbox, board, id) = board_with_ticket();
    let unknown = "00000000-0000-0000-0000-000000000000";
    match board
        .append_comment(unknown, "builder", "hello")
        .unwrap_err()
    {
        DomainError::UnknownId { id: named, open } => {
            assert_eq!(named, unknown);
            assert_eq!(open.len(), 1);
            assert_eq!(open[0].id, id);
        }
        other => panic!("expected unknownID, got {other:?}"),
    }
    // A non-UUID id is unknown too.
    assert!(matches!(
        board.append_comment("not a uuid", "builder", "hello"),
        Err(DomainError::UnknownId { .. })
    ));
}

#[test]
fn a_ticket_with_no_comments_answers_an_empty_list() {
    let (_sandbox, board, id) = board_with_ticket();
    assert!(board.comments(&id.to_string()).unwrap().is_empty());
}

#[test]
fn an_unknown_id_on_list_is_the_domain_failure_not_an_empty_log() {
    let sandbox = Sandbox::new("comments-unknown");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();

    let unknown = "00000000-0000-0000-0000-000000000000";
    match board.comments(unknown).unwrap_err() {
        DomainError::UnknownId { id: named, open } => {
            assert_eq!(named, unknown);
            assert_eq!(open.len(), 1);
            assert_eq!(open[0].id, ticket.id);
        }
        other => panic!("expected unknownID, got {other:?}"),
    }
    match board.comments("not a uuid").unwrap_err() {
        DomainError::UnknownId { id: named, .. } => assert_eq!(named, "not a uuid"),
        other => panic!("expected unknownID, got {other:?}"),
    }
    // A ref that names nothing gets the ref-specific failure, like every other verb.
    match board.comments("PRB-99").unwrap_err() {
        DomainError::UnknownRef { reference, hint } => {
            assert_eq!(reference, "PRB-99");
            assert!(hint.contains("PRB-"));
        }
        other => panic!("expected unknownRef, got {other:?}"),
    }
}

#[test]
fn list_resolves_a_ref_to_the_tickets_log_in_either_case() {
    let sandbox = Sandbox::new("comments-ref");
    let board = sandbox.board();
    board.add_lane("Probe", Some("PRB")).unwrap();
    let ticket = board
        .add(
            "A ticket",
            AddTicket {
                lane: Some("Probe"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(ticket.reference.as_deref(), Some("PRB-1"));
    board
        .append_comment(&ticket.id.to_string(), "builder", "did you mean X?")
        .unwrap();

    let by_uuid = board.comments(&ticket.id.to_string()).unwrap();
    assert_eq!(
        by_uuid
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["did you mean X?"]
    );
    for reference in ["PRB-1", "prb-1"] {
        assert_eq!(
            board.comments(reference).unwrap(),
            by_uuid,
            "{reference} answers the same log"
        );
    }
}

#[test]
fn a_comment_survives_an_edit_of_the_ticket() {
    let (_sandbox, board, id) = board_with_ticket();
    board
        .append_comment(&id.to_string(), "builder", "asked mid-build")
        .unwrap();

    board
        .set(
            &id.to_string(),
            SetFields {
                title: Edit::Set("A renamed ticket"),
                ..SetFields::default()
            },
        )
        .unwrap();
    board.complete(&id.to_string()).unwrap();

    let listed = board.comments(&id.to_string()).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["asked mid-build"]
    );
}

#[test]
fn the_author_is_trimmed_and_an_absent_author_is_the_empty_string() {
    let (_sandbox, board, id) = board_with_ticket();
    let trimmed = board
        .append_comment(&id.to_string(), "  builder  ", "signed")
        .unwrap();
    assert_eq!(trimmed.author, "builder");
    let unnamed = board
        .append_comment(&id.to_string(), "", "anonymous")
        .unwrap();
    assert_eq!(unnamed.author, "");
}

#[test]
fn the_body_is_trimmed_at_its_edges_and_kept_whole_inside() {
    let (_sandbox, board, id) = board_with_ticket();
    let comment = board
        .append_comment(&id.to_string(), "builder", "\n  did you mean X?\n\n")
        .unwrap();
    assert_eq!(comment.body, "did you mean X?");

    let inner = board
        .append_comment(&id.to_string(), "builder", "  one\n\ntwo  ")
        .unwrap();
    assert_eq!(inner.body, "one\n\ntwo");
}

#[test]
fn a_lowercase_and_an_uppercase_id_reach_the_same_log() {
    let (_sandbox, board, id) = board_with_ticket();
    board
        .append_comment(&id.to_string(), "builder", "hi")
        .unwrap();
    assert_eq!(
        board
            .comments(&id.to_string().to_lowercase())
            .unwrap()
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["hi"]
    );
    assert_eq!(
        board
            .comments(&id.to_string().to_uppercase())
            .unwrap()
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["hi"]
    );
}

#[test]
fn the_app_reads_the_log_in_append_order_with_author_time_and_body() {
    let (_sandbox, board, id) = board_with_ticket();
    for (author, body) in [("builder", "first"), ("human", "second"), ("", "third")] {
        board.append_comment(&id.to_string(), author, body).unwrap();
    }

    let read = board.comment_read(&id.to_string());
    let CommentRead::List(listed) = read else {
        panic!("expected the log, got {read:?}");
    };
    assert_eq!(
        listed
            .iter()
            .map(|comment| comment.body.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    assert_eq!(
        listed
            .iter()
            .map(|comment| comment.author.as_str())
            .collect::<Vec<_>>(),
        ["builder", "human", ""]
    );
    // The times never go backwards: the order the sheet draws is the order written.
    let times: Vec<i64> = listed.iter().map(|comment| comment.at).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
}

#[test]
fn a_ticket_with_no_log_reads_as_no_comments() {
    let (_sandbox, board, id) = board_with_ticket();
    match board.comment_read(&id.to_string()) {
        CommentRead::List(listed) => assert!(listed.is_empty()),
        other => panic!("expected an empty log, got {other:?}"),
    }
}

#[test]
fn a_database_held_exclusively_reads_as_busy_not_as_a_damaged_log() {
    let (sandbox, board, id) = board_with_ticket();
    board
        .append_comment(&id.to_string(), "builder", "first")
        .unwrap();

    // Another connection holds the database exclusively: a WAL snapshot reads past
    // a normal writer, but an exclusive holder is the contention the store's ~2 s
    // window ends in a timeout.
    let holder = rusqlite::Connection::open(sandbox.database()).expect("the database opens");
    holder
        .execute_batch("PRAGMA locking_mode=EXCLUSIVE; BEGIN IMMEDIATE;")
        .expect("the holder takes the write lock");

    let read = board.comment_read(&id.to_string());
    assert!(
        matches!(read, CommentRead::Busy(_)),
        "expected a busy read, got {read:?}"
    );

    let _ = holder.execute_batch("ROLLBACK");
}

#[test]
fn the_apps_write_path_appends_the_humans_comment_under_the_humans_name() {
    let (_sandbox, board, id) = board_with_ticket();
    let written = board
        .comment_write(&id.to_string(), "said in the sheet")
        .unwrap();
    assert_eq!(written.author, "human");
    assert_eq!(written.body, "said in the sheet");
    assert_eq!(board.comments(&id.to_string()).unwrap(), [written]);
}

#[test]
fn a_deleted_tickets_log_stays_where_it_was() {
    let (_sandbox, board, id) = board_with_ticket();
    board
        .append_comment(&id.to_string(), "builder", "kept")
        .unwrap();
    board.delete(&id.to_string()).unwrap();

    // The ticket is gone, so the log is unreachable through it: the row still sits
    // in the table, and the failure is the board's own.
    assert!(matches!(
        board.comments(&id.to_string()),
        Err(DomainError::UnknownId { .. })
    ));
}
