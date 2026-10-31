//! The harness read, and the wrapper's whole shape: a resumed pass lands its
//! usage on a record of its own and keeps the board it carried (the read
//! `session end --usage-from-harness` makes, `docs/port.md` Where the old code goes).
//!
//! The harness database here is opencode's shape: one `session_v2` row per
//! session and one `session_message` row per message, the skill loads sitting in
//! the message's `content` as `skill` tool parts.

use std::fs;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::domain::error::DomainError;
use crate::domain::harness::{HarnessUsage, read, read_events};
use crate::domain::session::{
    Ending, HarnessSession, HarnessTransport, SessionOutcome, SessionStatus, Sessions, SkillCounts,
};

use super::support::{Sandbox, TICKET_ONE, TICKET_TWO, at};

/// A harness database with one session: `ses_1` spent the numbers below and
/// loaded `opencode` twice and `tdd` once.
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
                session_id TEXT NOT NULL,
                data TEXT NOT NULL
            );",
        )
        .expect("the harness schema is written");
    // A row the harness never attributed: the model column holds JSON `null`.
    connection
        .execute(
            "INSERT INTO session_v2
                (id, cost, tokens_input, tokens_output, tokens_reasoning,
                 tokens_cache_read, tokens_cache_write, model)
             VALUES ('ses_unattributed', 0, 0, 0, 0, 0, 0, 'null')",
            [],
        )
        .expect("the unattributed row is written");
    connection
        .execute(
            "INSERT INTO session_v2
                (id, cost, tokens_input, tokens_output, tokens_reasoning,
                 tokens_cache_read, tokens_cache_write, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "ses_1",
                0.42,
                25_765,
                3_100,
                900,
                12_000,
                150,
                "{\"id\":\"deepseek-v4.1-flash\",\"providerID\":\"opencode-go\"}"
            ],
        )
        .expect("the harness row is written");
    for data in [
        r#"{"content":[{"type":"reasoning","text":"thinking"}]}"#,
        r#"{"content":[{"type":"tool","name":"skill","state":{"input":{"id":"opencode"}}}]}"#,
        r#"{"content":[{"type":"tool","name":"skill","state":{"input":{"id":"opencode"}}},{"type":"tool","name":"skill","state":{"input":{"id":" tdd "}}}]}"#,
        "not json at all",
    ] {
        connection
            .execute(
                "INSERT INTO session_message (session_id, data) VALUES (?1, ?2)",
                params!["ses_1", data],
            )
            .expect("a harness message is written");
    }
    path
}

#[test]
fn the_harness_row_becomes_the_usage_triple_and_the_skill_counts() {
    let sandbox = Sandbox::new("harness-read");
    let path = harness(&sandbox);

    let usage = read(&path, "ses_1").expect("the row is there");

    assert_eq!(usage.tokens.input, Some(25_765));
    assert_eq!(usage.tokens.output, Some(3_100));
    assert_eq!(usage.tokens.reasoning, Some(900));
    assert_eq!(usage.tokens.cache_read, Some(12_000));
    assert_eq!(usage.tokens.cache_write, Some(150));
    assert_eq!(usage.cost, Some(0.42));
    assert_eq!(
        usage.model.as_deref(),
        Some("deepseek-v4.1-flash"),
        "the model is the id inside the harness's JSON, not the JSON"
    );
    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([
            ("opencode".to_string(), 2),
            ("tdd".to_string(), 1),
        ])),
        "the skill loads are counted by name, and a name is trimmed"
    );
}

#[test]
fn a_harness_row_with_a_null_model_reports_no_model() {
    let sandbox = Sandbox::new("harness-null-model");
    let path = harness(&sandbox);

    let usage = read(&path, "ses_unattributed").expect("the row is there");
    assert_eq!(
        usage.model, None,
        "JSON null is no model, not a model called \"null\""
    );
}

#[test]
fn a_missing_harness_database_is_a_domain_failure_not_a_panic() {
    let sandbox = Sandbox::new("harness-missing");
    let path = sandbox.path().join("nothing-here.db");

    match read(&path, "ses_1") {
        Err(DomainError::NoHarnessDatabase { path: named }) => {
            assert!(
                named.ends_with("nothing-here.db"),
                "the message names the path"
            );
        }
        other => panic!("expected a missing harness database, got {other:?}"),
    }
}

#[test]
fn a_session_the_harness_never_recorded_names_the_id() {
    let sandbox = Sandbox::new("harness-unknown");
    let path = harness(&sandbox);

    assert_eq!(
        read(&path, "ses_9").unwrap_err(),
        DomainError::UnknownHarnessSession {
            id: "ses_9".to_string(),
        }
    );
}

#[test]
fn a_message_that_is_not_the_json_this_build_reads_is_skipped() {
    let sandbox = Sandbox::new("harness-skip");
    let path = harness(&sandbox);

    // The suite's database carries one message that is not JSON at all: the read
    // still answers the usage the pass spent, and the counts it could read.
    let usage = read(&path, "ses_1").expect("the usage is still read");
    assert!(usage.tokens.reported());
    assert!(usage.skills.is_some(), "the readable messages still count");
}

/// An event stream with one session's events: `ses_1` spent the numbers below
/// across two messages and loaded `opencode` and `tdd` once each.
///
/// The envelope is opencode's own: `message.updated` carries the assistant
/// message's `info` (tokens, cost, model), `message.part.updated` the tool parts
/// a skill load rides in. A stream repeats a message as it changes, so the last
/// update of a message is the one that counts; an event for another session and
/// a line that is not JSON are neither this session's usage nor a failure.
fn stream(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.path().join("events.jsonl");
    fs::write(
        &path,
        [
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_1","sessionID":"ses_1","role":"assistant","cost":0.25,"tokens":{"input":1000,"output":200,"reasoning":50,"cache":{"read":100,"write":10}},"providerID":"opencode-go","modelID":"deepseek-v4.1-flash"}}}"#,
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_1","sessionID":"ses_1","role":"assistant","cost":0.25,"tokens":{"input":1000,"output":200,"reasoning":50,"cache":{"read":100,"write":10}},"providerID":"opencode-go","modelID":"deepseek-v4.1-flash"}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_1","sessionID":"ses_1","messageID":"msg_1","type":"tool","tool":"skill","state":{"input":{"id":"opencode"}}}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_2","sessionID":"ses_1","messageID":"msg_1","type":"tool","tool":"skill","state":{"input":{"id":" tdd "}}}}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"id":"prt_2","sessionID":"ses_1","messageID":"msg_1","type":"tool","tool":"skill","state":{"input":{"id":" tdd "}}}}}"#,
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_2","sessionID":"ses_1","role":"assistant","cost":0.25,"tokens":{"input":500,"output":100,"reasoning":0,"cache":{"read":0,"write":0}},"providerID":"opencode-go","modelID":"claude-sonnet-5"}}}"#,
            "not json at all",
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_9","sessionID":"ses_other","role":"assistant","cost":9,"tokens":{"input":9999}}}}"#,
        ]
        .join("\n"),
    )
    .expect("the event stream is written");
    path
}

#[test]
fn the_event_stream_becomes_the_usage_triple_and_the_skill_counts() {
    let sandbox = Sandbox::new("harness-stream");
    let path = stream(&sandbox);

    let usage = read_events(&path, "ses_1").expect("the stream is there");

    assert_eq!(
        usage.tokens.input,
        Some(1_500),
        "two messages, summed once each"
    );
    assert_eq!(usage.tokens.output, Some(300));
    assert_eq!(usage.tokens.reasoning, Some(50));
    assert_eq!(usage.tokens.cache_read, Some(100));
    assert_eq!(usage.tokens.cache_write, Some(10));
    assert_eq!(
        usage.cost,
        Some(0.5),
        "a repeat of a message is not billed twice"
    );
    assert_eq!(
        usage.model.as_deref(),
        Some("claude-sonnet-5"),
        "the model is the last assistant message's, not the first's"
    );
    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([
            ("opencode".to_string(), 1),
            ("tdd".to_string(), 1),
        ])),
        "one count per part id, and a name is trimmed"
    );
}

#[test]
fn a_stream_that_is_not_there_is_a_domain_failure_not_a_panic() {
    let sandbox = Sandbox::new("harness-stream-missing");
    let path = sandbox.path().join("absent.jsonl");

    match read_events(&path, "ses_1") {
        Err(DomainError::NoHarnessEvents { path: named }) => {
            assert!(
                named.ends_with("absent.jsonl"),
                "the failure names the path: {named}"
            );
        }
        other => panic!("expected a missing event stream, got {other:?}"),
    }
}

#[test]
fn a_stream_with_no_message_for_the_session_reports_no_usage() {
    let sandbox = Sandbox::new("harness-stream-unknown");
    let path = stream(&sandbox);

    let usage = read_events(&path, "ses_unknown").expect("the stream is there");
    assert_eq!(
        usage,
        HarnessUsage::default(),
        "another session's events are not this session's usage"
    );
    assert_eq!(usage.tokens(), None, "absence, never an invented zero");
}

#[test]
fn a_second_pass_reads_the_harness_into_a_record_of_its_own() {
    let sandbox = Sandbox::new("harness-wrapper");
    let path = harness(&sandbox);
    let store = sandbox.store();
    let sessions = Sessions::new(&store);
    let actor = sandbox.actor();
    let harness = HarnessSession {
        id: "ses_1".to_string(),
        transport: HarnessTransport::Database,
    };

    // The first pass: recorded, linked to the board, stamped, already ended when
    // a second pass runs in the same harness session.
    sessions
        .start_with_harness(
            &actor,
            "ses_pass_1",
            "tandem",
            Some("builder"),
            "The first pass.",
            harness.clone(),
            Some(&at(0)),
        )
        .expect("the first record starts");
    sessions.link(&actor, "ses_pass_1", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_pass_1", TICKET_TWO).unwrap();
    sessions
        .end(
            &actor,
            "ses_pass_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("The first pass landed."),
                at: Some(&at(60)),
                ..Ending::default()
            },
        )
        .expect("the first record stamps");

    // The second pass writes its own record against the same harness session and
    // takes the harness's usage there. Nothing continues the stamped record.
    sessions
        .start_with_harness(
            &actor,
            "ses_pass_2",
            "tandem",
            Some("builder"),
            "A second pass in the same harness session.",
            harness,
            Some(&at(90)),
        )
        .expect("the second pass gets its own record");
    sessions.link(&actor, "ses_pass_2", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_pass_2", TICKET_TWO).unwrap();

    let usage = read(&path, "ses_1").expect("the harness row is there");
    let stamp = sessions
        .end(
            &actor,
            "ses_pass_2",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("What the second pass landed."),
                tokens: usage.tokens(),
                cost: usage.cost,
                model: usage.model.as_deref(),
                skills: usage.skills.clone(),
                at: Some(&at(150)),
            },
        )
        .expect("the second record stamps");

    assert!(stamp.stamped, "the usage comment posts once");
    assert_eq!(stamp.session.id, "ses_pass_2");
    assert_eq!(
        stamp.session.harness_session_id.as_deref(),
        Some("ses_1"),
        "the harness session is the pointer, not the handle"
    );
    assert_eq!(stamp.session.status, SessionStatus::Ended);
    assert_eq!(
        stamp.session.tickets,
        vec![TICKET_ONE.to_string(), TICKET_TWO.to_string()]
    );
    assert_eq!(stamp.session.cost, Some(0.42));
    assert_eq!(stamp.session.model.as_deref(), Some("deepseek-v4.1-flash"));
    assert_eq!(
        stamp.session.skills,
        Some(SkillCounts::from([
            ("opencode".to_string(), 2),
            ("tdd".to_string(), 1),
        ]))
    );
    assert_eq!(
        stamp
            .session
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.input),
        Some(25_765)
    );

    // And the first record still holds its own, usage-free fact.
    let first = sessions.show("ses_pass_1").unwrap();
    assert_eq!(first.cost, None);
    assert_eq!(first.status, SessionStatus::Ended);
    assert_eq!(
        first.harness_session_id.as_deref(),
        Some("ses_1"),
        "both records still name the harness session they ran in"
    );
    assert_eq!(sessions.list(None, None).unwrap().len(), 2);
}
