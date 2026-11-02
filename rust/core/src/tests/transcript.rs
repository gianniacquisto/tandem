//! A session's harness transcript: the thinking and tool calls the pass produced.
//!
//! The usage a record stamps is a roll-up; the transcript is the rows behind
//! it. The database transport reads the harness's own `session_message` rows
//! in `seq` order, and the event-stream transport replays the forwarded
//! `message.updated` and `message.part.updated` events in first-seen order.
//! Both page by cursor, oldest first, so a builder session's long transcript
//! loads in pieces and the renderer never holds the whole run at once.

use std::fs;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::domain::harness::{transcript, transcript_events};

use super::support::Sandbox;

/// A harness database with one session of three messages: a user prompt, an
/// assistant turn with thinking, text and a tool call whose output is two
/// lines, and a closing assistant text.
fn harness(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.path().join("opencode.db");
    let connection = Connection::open(&path).expect("the harness database opens");
    connection
        .execute_batch(
            "CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                cost REAL,
                tokens_input INTEGER,
                tokens_output INTEGER,
                tokens_reasoning INTEGER,
                tokens_cache_read INTEGER,
                tokens_cache_write INTEGER,
                model TEXT
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                type TEXT NOT NULL,
                seq INTEGER NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL
            );",
        )
        .expect("the harness schema is written");
    connection
        .execute(
            "INSERT INTO session_v2
                (id, cost, tokens_input, tokens_output, tokens_reasoning,
                 tokens_cache_read, tokens_cache_write, model)
             VALUES ('ses_1', 0.1, 10, 5, 0, 0, 0, 'a-model')",
            [],
        )
        .expect("the harness row is written");
    for (id, kind, seq, data) in [
        (
            "msg_1",
            "user",
            4,
            r#"{"time":{"created":100},"text":"Do the thing."}"#,
        ),
        (
            "msg_2",
            "assistant",
            5,
            r#"{"time":{"created":101},"agent":"builder","model":{"id":"a-model"},"content":[{"type":"reasoning","text":"Let me think."},{"type":"text","text":"On it."},{"type":"tool","id":"call_1","name":"shell","state":{"status":"completed","input":{"command":"ls"},"content":[{"type":"text","text":"one\ntwo"}]}}]}"#,
        ),
        (
            "msg_3",
            "assistant",
            9,
            r#"{"time":{"created":102},"agent":"builder","content":[{"type":"text","text":"Done."}]}"#,
        ),
    ] {
        connection
            .execute(
                "INSERT INTO session_message
                    (id, session_id, type, seq, time_created, time_updated, data)
                 VALUES (?1, 'ses_1', ?2, ?3, ?4, ?4, ?5)",
                params![id, kind, seq, seq * 1000, data],
            )
            .expect("a harness message is written");
    }
    // Another session's message, which the transcript must not carry.
    connection
        .execute(
            "INSERT INTO session_message
                (id, session_id, type, seq, time_created, time_updated, data)
             VALUES ('msg_9', 'ses_other', 'user', 1, 1, 1, '{\"text\":\"Elsewhere.\"}')",
            [],
        )
        .expect("the other session's message is written");
    path
}

#[test]
fn the_database_transcript_reads_oldest_first_with_thinking_tools_and_outputs() {
    let sandbox = Sandbox::new("transcript-db");
    let path = harness(&sandbox);

    let page = transcript(&path, "ses_1", None, 50).expect("the transcript reads");

    assert_eq!(page.entries.len(), 3, "the other session's row stays out");
    assert_eq!(page.next_cursor, None, "a short page names no cursor");
    let user = &page.entries[0];
    assert_eq!(user.role, "user");
    assert_eq!(user.id, "msg_1");
    assert_eq!(user.seq, 4);
    assert_eq!(user.blocks.len(), 1);
    assert!(user.blocks[0].text().unwrap().contains("Do the thing."));

    let turn = &page.entries[1];
    assert_eq!(turn.role, "assistant");
    assert_eq!(turn.agent.as_deref(), Some("builder"));
    assert_eq!(turn.model.as_deref(), Some("a-model"));
    assert_eq!(turn.blocks.len(), 3, "thinking, text, and the tool call");
    assert!(
        turn.blocks[0]
            .reasoning()
            .unwrap()
            .contains("Let me think.")
    );
    let tool = turn.blocks[2]
        .tool()
        .expect("the third block is the tool call");
    assert_eq!(tool.name, "shell");
    assert_eq!(tool.status, "completed");
    assert_eq!(
        tool.input
            .get("command")
            .and_then(|command| command.as_str()),
        Some("ls"),
        "the call's inputs ride along, unredacted"
    );
    assert_eq!(
        tool.output,
        Some("one\ntwo"),
        "the call's output rides along too"
    );
}

#[test]
fn the_database_transcript_pages_by_sequence() {
    let sandbox = Sandbox::new("transcript-db-pages");
    let path = harness(&sandbox);

    let first = transcript(&path, "ses_1", None, 2).expect("the first page reads");
    assert_eq!(first.entries.len(), 2);
    assert_eq!(
        first.next_cursor,
        Some(5),
        "the cursor is the page's last sequence"
    );

    let second = transcript(&path, "ses_1", first.next_cursor, 2).expect("the next page reads");
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.entries[0].id, "msg_3");
    assert_eq!(second.next_cursor, None, "the last page names no cursor");

    let empty = transcript(&path, "ses_1", Some(9), 2).expect("a cursor past the end reads");
    assert!(empty.entries.is_empty());
    assert_eq!(empty.next_cursor, None);
}

/// The forwarded event stream: `message.updated` carries each message's shell,
/// `message.part.updated` its tool parts, and a foreign session's events ride
/// the same file.
fn events(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.path().join("events.jsonl");
    fs::write(
        &path,
        [
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_1","sessionID":"ses_1","role":"user"}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_0","sessionID":"ses_1","messageID":"msg_1","type":"text","text":"Do the thing."}}}"#,
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_2","sessionID":"ses_1","role":"assistant","providerID":"opencode-go","modelID":"a-model"}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_1","sessionID":"ses_1","messageID":"msg_2","type":"reasoning","text":"Let me think."}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_2","sessionID":"ses_1","messageID":"msg_2","type":"tool","tool":"shell","state":{"status":"completed","input":{"command":"ls"},"content":[{"type":"text","text":"one\ntwo"}]}}}}"#,
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_9","sessionID":"ses_other","role":"user"}}}"#,
            "not json at all",
        ]
        .join("\n"),
    )
    .expect("the event stream is written");
    path
}

#[test]
fn the_event_transcript_replays_messages_with_their_parts_in_order() {
    let sandbox = Sandbox::new("transcript-events");
    let path = events(&sandbox);

    let page = transcript_events(&path, "ses_1", None, 50).expect("the transcript reads");

    assert_eq!(page.entries.len(), 2, "the foreign session stays out");
    assert_eq!(page.next_cursor, None);
    let user = &page.entries[0];
    assert_eq!(user.role, "user");
    assert_eq!(user.blocks.len(), 1);
    assert!(user.blocks[0].text().unwrap().contains("Do the thing."));

    let turn = &page.entries[1];
    assert_eq!(turn.role, "assistant");
    assert_eq!(turn.model.as_deref(), Some("a-model"));
    assert_eq!(turn.blocks.len(), 2, "thinking and the tool call");
    assert!(
        turn.blocks[0]
            .reasoning()
            .unwrap()
            .contains("Let me think.")
    );
    let tool = turn.blocks[1]
        .tool()
        .expect("the second block is the tool call");
    assert_eq!(tool.name, "shell");
    assert_eq!(tool.output, Some("one\ntwo"));
}

#[test]
fn the_event_transcript_pages_by_position() {
    let sandbox = Sandbox::new("transcript-events-pages");
    let path = events(&sandbox);

    let first = transcript_events(&path, "ses_1", None, 1).expect("the first page reads");
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.next_cursor, Some(0));

    let second =
        transcript_events(&path, "ses_1", first.next_cursor, 1).expect("the next page reads");
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.entries[0].role, "assistant");
    assert_eq!(second.next_cursor, Some(1), "a full page names its cursor");

    let last =
        transcript_events(&path, "ses_1", second.next_cursor, 1).expect("the last page reads");
    assert!(last.entries.is_empty());
    assert_eq!(last.next_cursor, None);
}
