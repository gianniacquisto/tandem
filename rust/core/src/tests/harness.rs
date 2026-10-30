//! The harness read, and the wrapper's whole shape: a resumed pass lands its
//! usage on a record of its own and keeps the board it carried
//! (`Scripts/usage-report.sh` today, `docs/port.md` Where the old code goes).
//!
//! The harness database here is opencode's shape: one `session_v2` row per
//! session and one `session_message` row per message, the skill loads sitting in
//! the message's `content` as `skill` tool parts.

use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::error::DomainError;
use crate::session::{Ending, SessionOutcome, SessionStatus, Sessions, SkillCounts};
use crate::store::read_harness_usage;

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

    let usage = read_harness_usage(&path, "ses_1").expect("the row is there");

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
fn a_missing_harness_database_is_a_domain_failure_not_a_panic() {
    let sandbox = Sandbox::new("harness-missing");
    let path = sandbox.path().join("nothing-here.db");

    match read_harness_usage(&path, "ses_1") {
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
        read_harness_usage(&path, "ses_9").unwrap_err(),
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
    let usage = read_harness_usage(&path, "ses_1").expect("the usage is still read");
    assert!(usage.tokens.reported());
    assert!(usage.skills.is_some(), "the readable messages still count");
}

#[test]
fn a_resumed_pass_writes_its_usage_to_its_own_record_and_carries_the_tickets() {
    let sandbox = Sandbox::new("harness-wrapper");
    let path = harness(&sandbox);
    let store = sandbox.store();
    let sessions = Sessions::new(&store);
    let actor = sandbox.actor();

    // The first pass: recorded, linked to the board, stamped, already ended when
    // the harness resumes the session.
    sessions
        .start(
            &actor,
            "ses_1",
            "tandem",
            Some("builder"),
            "The first pass.",
            Some(&at(0)),
        )
        .expect("the first record starts");
    sessions.link(&actor, "ses_1", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_1", TICKET_TWO).unwrap();
    sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("The first pass landed."),
                at: Some(&at(60)),
                ..Ending::default()
            },
        )
        .expect("the first record stamps");

    // The wrapper's composition, in the core: read the harness, mint the pass's
    // own record, link what it carried, stamp the usage on it.
    let usage = read_harness_usage(&path, "ses_1").expect("the harness row is there");
    let continued = sessions
        .continue_after(&actor, "ses_1", "A resumed pass.", Some(&at(90)))
        .expect("the pass gets its own record");
    assert_eq!(continued.value.id, "ses_1-2");

    let stamp = sessions
        .end(
            &actor,
            "ses_1-2",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("What the resumed pass landed."),
                tokens: usage.tokens(),
                cost: usage.cost,
                model: usage.model.as_deref(),
                skills: usage.skills.clone(),
                at: Some(&at(150)),
            },
        )
        .expect("the resumed record stamps");

    assert!(stamp.stamped, "the usage comment posts once");
    assert_eq!(stamp.session.id, "ses_1-2");
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

    // And the ended record it continued still holds its own, usage-free fact.
    let first = sessions.show("ses_1").unwrap();
    assert_eq!(first.cost, None);
    assert_eq!(first.status, SessionStatus::Ended);
}
