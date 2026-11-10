//! The ticket sheet's Sessions section, over the wire: the linked records and
//! their harness transcripts.
//!
//! The renderer never opens the harness store itself. It lists the ticket's
//! session records through `session.list` and reads one record's transcript
//! through `session.transcript`, which resolves the Tandem id (or the raw
//! harness id the older caller speaks) to the newest record naming the harness
//! pointer, per `Sessions::resolve`. A record the harness no longer holds
//! answers its row with an unavailable reason, never an error, so the section
//! still renders.

mod support;

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Connection, params};
use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::domain::session::{
    Ending, HarnessSession, HarnessTransport, SessionOutcome, Sessions,
};
use tandem_core::domain::{Board, Status};
use tandem_core::store::Actor;

fn frame(id: u64, method: &str, params: Value) -> String {
    json!({"id": id, "method": method, "params": params}).to_string()
}

fn answer(
    store: &tandem_core::store::Store,
    environment: &HashMap<String, String>,
    frame: &str,
) -> Value {
    let reply = tandem_core::rpc::handle_in(frame, store, &Actor::Human, environment);
    serde_json::from_str(&reply).expect("the answer is one JSON frame")
}

fn environment_with_harness(database: &std::path::Path) -> HashMap<String, String> {
    HashMap::from([("OPENCODE_DB".to_string(), database.display().to_string())])
}

/// A harness database holding one recorded session of two messages.
fn harness(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.root().join("opencode.db");
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
             VALUES ('ses_harness', 0.1, 10, 5, 0, 0, 0, 'a-model')",
            [],
        )
        .expect("the harness row is written");
    for (id, kind, seq, data) in [
        (
            "msg_1",
            "user",
            1,
            r#"{"time":{"created":100},"text":"Do the thing."}"#,
        ),
        (
            "msg_2",
            "assistant",
            2,
            r#"{"time":{"created":101},"agent":"builder","content":[{"type":"reasoning","text":"Let me think."},{"type":"tool","id":"call_1","name":"shell","state":{"status":"completed","input":{"command":"ls"},"content":[{"type":"text","text":"one\ntwo"}]}}]}"#,
        ),
    ] {
        connection
            .execute(
                "INSERT INTO session_message
                    (id, session_id, type, seq, time_created, time_updated, data)
                 VALUES (?1, 'ses_harness', ?2, ?3, ?4, ?4, ?5)",
                params![id, kind, seq, seq * 1000, data],
            )
            .expect("a harness message is written");
    }
    path
}

fn ticket(sandbox: &Sandbox) -> String {
    Board::new(sandbox.store())
        .add(
            "The ticket the sessions carried.",
            tandem_core::domain::AddTicket {
                lane: None,
                status: Status::InProgress,
                assignee: None,
                description: None,
                origin: "human",
            },
        )
        .expect("the ticket is added")
        .id
        .to_string()
}

fn record(
    sandbox: &Sandbox,
    id: &str,
    started_at: &str,
    ticket: Option<&str>,
    harness: Option<(&str, HarnessTransport)>,
) {
    let store = sandbox.store();
    let sessions = Sessions::new(&store);
    let actor = Actor::Agent("builder".to_string());
    let pointer = harness.map(|(id, transport)| HarnessSession {
        id: id.to_string(),
        transport,
    });
    match pointer {
        Some(pointer) => sessions
            .start_with_harness(
                &actor,
                id,
                "tandem",
                Some("builder"),
                "A brief.",
                pointer,
                Some(started_at),
            )
            .expect("the record starts"),
        None => sessions
            .start(
                &actor,
                id,
                "tandem",
                Some("builder"),
                "A brief.",
                Some(started_at),
            )
            .expect("the record starts"),
    };
    if let Some(ticket) = ticket {
        sessions
            .link(&actor, id, ticket)
            .expect("the record carries the ticket");
    }
    sessions
        .end(
            &actor,
            id,
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("What landed."),
                tokens: None,
                cost: Some(0.5),
                model: Some("deepseek-v4"),
                skills: None,
                turns: None,
                calls: None,
                at: Some("2026-10-30T12:05:00Z"),
            },
        )
        .expect("the record stamps");
}

#[test]
fn session_list_answers_the_tickets_records_newest_first() {
    let sandbox = Sandbox::new("rpc-session-list");
    let owned = ticket(&sandbox);
    record(
        &sandbox,
        "ses_old",
        "2026-10-30T11:00:00Z",
        Some(&owned),
        Some(("ses_harness", HarnessTransport::Database)),
    );
    record(
        &sandbox,
        "ses_new",
        "2026-10-30T12:00:00Z",
        Some(&owned),
        Some(("ses_harness", HarnessTransport::Database)),
    );
    record(
        &sandbox,
        "ses_elsewhere",
        "2026-10-30T13:00:00Z",
        None,
        None,
    );

    let store = sandbox.store();
    let environment = HashMap::new();
    let listed = answer(
        &store,
        &environment,
        &frame(1, "session.list", json!({"ticket": owned})),
    );
    let sessions = listed["result"]["sessions"]
        .as_array()
        .expect("the reply carries sessions");
    assert_eq!(sessions.len(), 2, "the unlinked record stays out");
    assert_eq!(sessions[0]["id"], "ses_new", "newest first: {listed}");
    assert_eq!(sessions[1]["id"], "ses_old");
    let newest = &sessions[0];
    assert_eq!(newest["agent"], "builder");
    assert_eq!(newest["status"], "ended");
    assert_eq!(newest["outcome"], "succeeded");
    assert_eq!(newest["brief"], "A brief.");
    assert_eq!(newest["summary"], "What landed.");
    assert_eq!(newest["model"], "deepseek-v4");
    assert_eq!(newest["cost"], 0.5);
    assert_eq!(newest["harnessSessionId"], "ses_harness");
    assert!(newest["startedAt"].is_string());
    assert!(newest["endedAt"].is_string());

    let all = answer(&store, &environment, &frame(2, "session.list", json!({})));
    assert_eq!(
        all["result"]["sessions"]
            .as_array()
            .expect("sessions")
            .len(),
        3,
        "no ticket filter reads every record"
    );
}

#[test]
fn session_transcript_resolves_from_the_tandem_id_and_the_raw_harness_id() {
    let sandbox = Sandbox::new("rpc-session-transcript");
    let database = harness(&sandbox);
    let owned = ticket(&sandbox);
    record(
        &sandbox,
        "ses_old",
        "2026-10-30T11:00:00Z",
        Some(&owned),
        Some(("ses_harness", HarnessTransport::Database)),
    );
    record(
        &sandbox,
        "ses_new",
        "2026-10-30T12:00:00Z",
        Some(&owned),
        Some(("ses_harness", HarnessTransport::Database)),
    );

    let store = sandbox.store();
    let environment = environment_with_harness(&database);
    for id in ["ses_new", "ses_harness"] {
        let read = answer(
            &store,
            &environment,
            &frame(1, "session.transcript", json!({"id": id})),
        );
        let entries = read["result"]["entries"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} reads entries: {read}"));
        assert_eq!(entries.len(), 2, "{id} reads the harness session: {read}");
        assert_eq!(entries[0]["role"], "user");
        assert_eq!(entries[1]["blocks"][1]["name"], "shell");
        assert_eq!(read["result"]["unavailable"], Value::Null);
        assert_eq!(read["result"]["nextCursor"], Value::Null);
    }

    let paged = answer(
        &store,
        &environment,
        &frame(
            2,
            "session.transcript",
            json!({"id": "ses_new", "limit": 1}),
        ),
    );
    assert_eq!(
        paged["result"]["entries"]
            .as_array()
            .expect("entries")
            .len(),
        1
    );
    assert_eq!(paged["result"]["nextCursor"], 1);
    let rest = answer(
        &store,
        &environment,
        &frame(
            3,
            "session.transcript",
            json!({"id": "ses_new", "cursor": 1, "limit": 1}),
        ),
    );
    assert_eq!(rest["result"]["entries"][0]["id"], "msg_2");
}

#[test]
fn session_transcript_reads_the_embedded_database_for_event_stream_records() {
    let sandbox = Sandbox::new("rpc-session-embedded");
    // An embedded run carries its rows in the home's own harness database, not
    // the external store `OPENCODE_DB` names.
    let embedded = sandbox.home().directory().join("harness");
    std::fs::create_dir_all(&embedded).expect("the harness home is creatable");
    let database = embedded.join("opencode.db");
    let connection = Connection::open(&database).expect("the embedded database opens");
    connection
        .execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, model TEXT);
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
        .expect("the embedded schema is written");
    connection
        .execute(
            "INSERT INTO session_v2 (id, model) VALUES ('ses_embedded', 'a-model')",
            [],
        )
        .expect("the embedded row is written");
    connection
        .execute(
            "INSERT INTO session_message
                (id, session_id, type, seq, time_created, time_updated, data)
             VALUES ('msg_1', 'ses_embedded', 'user', 1, 1000, 1000, '{\"text\":\"Hello.\"}')",
            [],
        )
        .expect("the embedded message is written");

    let owned = ticket(&sandbox);
    record(
        &sandbox,
        "ses_rec",
        "2026-10-30T11:00:00Z",
        Some(&owned),
        Some(("ses_embedded", HarnessTransport::EventStream)),
    );

    let store = sandbox.store();
    let read = answer(
        &store,
        &HashMap::new(),
        &frame(1, "session.transcript", json!({"id": "ses_rec"})),
    );
    let entries = read["result"]["entries"]
        .as_array()
        .expect("the embedded transcript reads");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["blocks"][0]["text"], "Hello.");
    assert_eq!(read["result"]["unavailable"], Value::Null);
}

#[test]
fn session_transcript_refuses_an_unknown_session_and_reports_a_missing_row() {
    let sandbox = Sandbox::new("rpc-session-unavailable");
    let database = harness(&sandbox);
    let owned = ticket(&sandbox);
    record(
        &sandbox,
        "ses_pointerless",
        "2026-10-30T11:00:00Z",
        Some(&owned),
        None,
    );
    record(
        &sandbox,
        "ses_pruned",
        "2026-10-30T12:00:00Z",
        Some(&owned),
        Some(("ses_gone", HarnessTransport::Database)),
    );

    let store = sandbox.store();
    let environment = environment_with_harness(&database);
    let unknown = answer(
        &store,
        &environment,
        &frame(1, "session.transcript", json!({"id": "ses_missing"})),
    );
    assert!(
        unknown.get("error").is_some(),
        "no record and no pointer is a refusal: {unknown}"
    );

    for (id, reason) in [
        ("ses_pointerless", "no harness session"),
        ("ses_pruned", "ses_gone"),
    ] {
        let read = answer(
            &store,
            &environment,
            &frame(2, "session.transcript", json!({"id": id})),
        );
        assert!(
            read.get("error").is_none(),
            "{id} still answers its row: {read}"
        );
        assert_eq!(
            read["result"]["entries"].as_array().map(Vec::len),
            Some(0),
            "{id} carries no entries"
        );
        let unavailable = read["result"]["unavailable"]
            .as_str()
            .unwrap_or_else(|| panic!("{id} names its reason: {read}"));
        assert!(unavailable.contains(reason), "{id} says why: {unavailable}");
    }

    let no_database = answer(
        &store,
        &HashMap::from([(
            "OPENCODE_DB".to_string(),
            sandbox.root().join("missing.db").display().to_string(),
        )]),
        &frame(3, "session.transcript", json!({"id": "ses_pruned"})),
    );
    assert!(
        no_database["result"]["unavailable"]
            .as_str()
            .unwrap_or_default()
            .contains("harness database"),
        "a missing database is a reason, not a refusal: {no_database}"
    );
}
