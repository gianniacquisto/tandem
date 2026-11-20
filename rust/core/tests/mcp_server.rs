//! The MCP door's protocol, without a process: one JSON-RPC frame in, one
//! frame out (or silence for a notification).
//!
//! These are the claims the pipe test cannot isolate:
//!
//! - `initialize` negotiates the protocol version instead of assuming it;
//! - `tools/list` names every tool with an object-rooted schema;
//! - `tools/call` drives the whole ticket loop through the `rpc` door, and a
//!   domain refusal is a result with `isError`, not a JSON-RPC error;
//! - a frame the door cannot route is a JSON-RPC error and never a panic.

mod support;

use std::sync::{Arc, Mutex};

/// Capture tests swap a global callsite-interest cache, so they run in turn.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::env::Environment;
use tandem_core::mcp;
use tandem_core::store::Actor;

fn environment() -> Environment {
    Environment::new()
}

/// Every plain call takes the capture lock too: a callsite first hit by a
/// parallel plain test while a capture test swaps the subscriber would cache
/// the wrong interest and drop an event.
fn call(store: &tandem_core::store::Store, frame: &str) -> Option<Value> {
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    call_unlocked(store, frame)
}

fn call_unlocked(store: &tandem_core::store::Store, frame: &str) -> Option<Value> {
    let answer = mcp::handle_mcp(frame, store, &Actor::Human, &environment())?;
    Some(
        serde_json::from_str(&answer)
            .unwrap_or_else(|error| panic!("the answer is one JSON frame ({error}): {answer}")),
    )
}

fn request(id: Value, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn tool_call(id: i64, name: &str, arguments: Value) -> String {
    request(
        json!(id),
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// Declare a label in the seed project through the tool, with no `detail`: the
/// label tools do not carry one, and a stray argument is refused (TAN-339).
fn declare_label(store: &tandem_core::store::Store, name: &str) {
    let answer = call(
        store,
        &tool_call(1, "label_add", json!({"name": name, "project": "Main"})),
    )
    .expect("the call answers");
    tool_text(&answer);
}

fn tool_text(answer: &Value) -> String {
    assert!(
        answer["error"].is_null(),
        "the call answers a result: {answer}"
    );
    assert!(
        answer["result"]["isError"].is_null(),
        "the call succeeds: {answer}"
    );
    answer["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the result carries text: {answer}"))
        .to_string()
}

#[test]
fn initialize_negotiates_the_protocol_version() {
    let sandbox = Sandbox::new("mcp-initialize");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let asked = call(
        &store,
        &request(
            json!(1),
            "initialize",
            json!({"protocolVersion": "2025-03-26"}),
        ),
    )
    .expect("initialize answers");
    assert_eq!(asked["id"], 1);
    assert_eq!(asked["result"]["protocolVersion"], "2025-03-26");

    let newer = call(
        &store,
        &request(
            json!(2),
            "initialize",
            json!({"protocolVersion": "1999-01-01"}),
        ),
    )
    .expect("initialize answers");
    assert_eq!(
        newer["result"]["protocolVersion"],
        mcp::NEGOTIATED_DEFAULT,
        "a version the door does not speak answers the newest it does"
    );

    assert_eq!(asked["result"]["serverInfo"]["name"], "tandem-mcp");
    assert!(
        !asked["result"]["serverInfo"]["version"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "the server names the binary's own version"
    );
    assert!(
        asked["result"]["capabilities"]["tools"].is_object(),
        "the door serves tools"
    );
}

#[test]
fn initialize_carries_a_small_health_block() {
    let sandbox = Sandbox::new("mcp-initialize-health");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let asked = call(
        &store,
        &request(
            json!(1),
            "initialize",
            json!({"protocolVersion": "2025-03-26"}),
        ),
    )
    .expect("initialize answers");
    let health = &asked["result"]["health"];
    assert!(
        health.is_object(),
        "initialize carries a health block beside serverInfo: {asked}"
    );
    assert!(
        !health["home"].as_str().unwrap_or("").is_empty(),
        "the health block names the resolved home: {health}"
    );
    assert_eq!(
        health["storeOk"], true,
        "a fresh sandbox store is ok: {health}"
    );
    assert!(
        !health["version"].as_str().unwrap_or("").is_empty(),
        "the health block names the version: {health}"
    );
    assert_eq!(
        health["tools"], 31,
        "the health block names the tool count: {health}"
    );
}

#[test]
fn initialize_reports_the_release_version_or_the_crate_version() {
    let sandbox = Sandbox::new("mcp-release-version");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let answered = call(
        &store,
        &request(
            json!(1),
            "initialize",
            json!({"protocolVersion": "2025-11-25"}),
        ),
    )
    .expect("initialize answers");
    // A dev build carries no `TANDEM_VERSION` stamp, so the crate version is
    // the answer (ADR 0045 retired the bundled `VERSION` marker).
    assert_eq!(
        answered["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION"),
        "no stamp is the crate version"
    );
}

#[test]
fn a_notification_is_answered_with_silence() {
    let sandbox = Sandbox::new("mcp-notification");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let initialized = mcp::handle_mcp(
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        &store,
        &Actor::Human,
        &environment(),
    );
    assert!(
        initialized.is_none(),
        "a notification is answered with nothing at all"
    );

    let ping = call(&store, &request(json!(7), "ping", json!({}))).expect("ping answers");
    assert_eq!(ping["id"], 7);
    assert_eq!(ping["result"], json!({}));
}

#[test]
fn tools_list_names_the_31_tools_with_object_schemas() {
    let sandbox = Sandbox::new("mcp-tools-list");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let listed = call(&store, &request(json!(1), "tools/list", json!({}))).expect("tools answer");
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    assert_eq!(tools.len(), 31);
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("");
        assert!(
            !name.starts_with("sprint_")
                && !name.starts_with("workflow_run_")
                && !name.starts_with("workflow_")
                && !name.starts_with("settings_")
                && name != "ticket_file"
                && name != "note_capture",
            "the retired tickets and workflow surfaces are gone: {name}"
        );
        assert!(
            !tool["name"].as_str().unwrap_or("").is_empty(),
            "every tool is named: {tool}"
        );
        assert!(
            !tool["description"].as_str().unwrap_or("").is_empty(),
            "every tool is documented: {tool}"
        );
        assert_eq!(
            tool["inputSchema"]["type"], "object",
            "every input schema has an object root: {tool}"
        );
    }
}

#[test]
fn the_label_tools_replace_the_lane_tools_and_the_label_argument_replaces_lane() {
    let sandbox = Sandbox::new("mcp-label-tools");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let listed = call(&store, &request(json!(1), "tools/list", json!({}))).expect("tools answer");
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or(""))
        .collect();
    for wanted in [
        "label_list",
        "label_add",
        "label_rename",
        "label_delete",
        "label_move",
    ] {
        assert!(names.contains(&wanted), "{wanted} is served: {names:?}");
    }
    assert!(
        names.iter().all(|name| !name.starts_with("lane_")),
        "no lane_* tool remains: {names:?}"
    );
    for tool in tools {
        let text = tool.to_string().to_lowercase();
        assert!(
            !text.contains("lane"),
            "no tool name, description or argument says lane: {tool}"
        );
    }
    for name in ["ticket_add", "ticket_list", "ticket_move"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("{name} is served"));
        let properties = &tool["inputSchema"]["properties"];
        assert!(properties.get("label").is_some(), "{name} takes label");
        assert!(properties.get("lane").is_none(), "{name} has no lane");
    }
}

#[test]
fn tools_call_files_reads_sets_and_completes_a_ticket() {
    let sandbox = Sandbox::new("mcp-ticket-loop");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                1,
                "ticket_add",
                json!({"project": "Main", "title": "First"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let id = added["ticket"]["id"].as_str().expect("an id").to_string();
    assert_eq!(added["ticket"]["title"], "First");

    let shown: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(2, "ticket_show", json!({"id": id}))).expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(shown["ticket"]["title"], "First");

    let moved: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                3,
                "ticket_set",
                json!({"id": id, "status": "in-progress", "assignee": ""}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(moved["ticket"]["status"], "in-progress");

    let done: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(4, "ticket_complete", json!({"id": id})))
            .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(done["ticket"]["status"], "done");
}

#[test]
fn a_domain_refusal_is_a_tool_error_not_a_wire_error() {
    let sandbox = Sandbox::new("mcp-domain");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let refused = call(&store, &tool_call(1, "ticket_show", json!({"id": "nope"})))
        .expect("the call answers");
    assert!(
        refused["error"].is_null(),
        "a domain refusal is still a tool result: {refused}"
    );
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        !refused["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "the domain's words reach the caller"
    );
}

#[test]
fn an_unknown_tool_is_an_invalid_params_error() {
    let sandbox = Sandbox::new("mcp-unknown-tool");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let refused = call(&store, &tool_call(1, "ticket_bogus", json!({}))).expect("the call answers");
    assert_eq!(refused["id"], 1);
    assert_eq!(refused["error"]["code"], -32602);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("ticket_bogus"),
        "the error names the tool: {refused}"
    );
}

#[test]
fn an_unknown_method_names_itself_and_a_batch_is_refused() {
    let sandbox = Sandbox::new("mcp-unknown-method");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let refused = call(&store, &request(json!(4), "ticket.bogus", json!({}))).expect("it answers");
    assert_eq!(refused["id"], 4);
    assert_eq!(refused["error"]["code"], -32601);

    let batch = mcp::handle_mcp(
        r#"[{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}]"#,
        &store,
        &Actor::Human,
        &environment(),
    )
    .expect("a batch is answered, not dropped");
    let batch: Value = serde_json::from_str(&batch).expect("the answer is JSON");
    assert!(
        batch["id"].is_null(),
        "the refused batch carries no id: {batch}"
    );
    assert_eq!(batch["error"]["code"], -32600);

    let unknown_notification = mcp::handle_mcp(
        r#"{"jsonrpc":"2.0","method":"notifications/bogus"}"#,
        &store,
        &Actor::Human,
        &environment(),
    );
    assert!(
        unknown_notification.is_none(),
        "an unknown notification is silence, not an error"
    );
}

#[test]
fn a_frame_that_is_not_a_request_answers_a_parse_error() {
    let sandbox = Sandbox::new("mcp-parse");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let refused = call(&store, "{ not json").expect("it answers");
    assert!(refused["id"].is_null());
    assert_eq!(refused["error"]["code"], -32700);

    let scalar = call(&store, "42").expect("it answers");
    assert_eq!(scalar["error"]["code"], -32600);
}

#[test]
fn tools_call_drives_a_ticket_and_the_note_tools() {
    let sandbox = Sandbox::new("mcp-notes");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                1,
                "ticket_add",
                json!({"project": "Main", "title": "Batch"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(added["ticket"]["title"], "Batch");

    let written: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                7,
                "note_write",
                json!({"path": "notes/Index.md", "text": "# Index\n\n[[Daily]]"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        written["write"]["path"], "notes/Index.md",
        "the write answers its path: {written}"
    );

    let read: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(8, "note_read", json!({"path": "notes/Index.md"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(read["note"]["text"], "# Index\n\n[[Daily]]");

    let tree: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(9, "note_tree", json!({}))).expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(tree["nodes"][0]["path"], "notes");

    let created: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(10, "note_create", json!({"path": "notes/Daily.md"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(created["path"], "notes/Daily.md");

    let folder: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(11, "note_create_folder", json!({"path": "archive"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(folder["path"], "archive");

    let moved: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                12,
                "note_move",
                json!({"path": "notes/Daily.md", "into": "archive"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(moved["moved"]["path"], "archive/Daily.md");

    let health: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(13, "note_health", json!({}))).expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        health["health"]["counts"]["notes"], 2,
        "the health read walks the same vault: {health}"
    );
}

// MARK: - The application log (stderr under TANDEM_LOG, ADR 0046)

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("the buffer locks")).into_owned()
    }

    /// A fmt subscriber at `level` on this thread, like the CLI's at its filter.
    fn install(&self, level: tracing::Level) -> tracing::subscriber::DefaultGuard {
        let writer = self.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        tracing::callsite::rebuild_interest_cache();
        guard
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the buffer locks")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_tool_call_logs_name_outcome_and_latency_and_never_the_arguments() {
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let sandbox = Sandbox::new("log-call");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let captured = Captured::default();
    let _guard = captured.install(tracing::Level::INFO);

    let added = call_unlocked(
        &store,
        &tool_call(
            1,
            "ticket_add",
            json!({"project": "Main", "title": "secretword title"}),
        ),
    )
    .expect("an answer");
    assert!(added["result"]["isError"].is_null());
    let refused = call_unlocked(
        &store,
        &tool_call(2, "ticket_show", json!({"id": "secretword-missing"})),
    )
    .expect("an answer");
    assert_eq!(refused["result"]["isError"], true);

    let text = captured.text();
    assert!(
        text.contains("tool=\"ticket_add\"") || text.contains("tool=ticket_add"),
        "{text}"
    );
    assert!(
        text.contains("outcome=\"ok\"") || text.contains("outcome=ok"),
        "{text}"
    );
    assert!(
        text.contains("outcome=\"error\"") || text.contains("outcome=error"),
        "{text}"
    );
    assert!(text.contains("kind="), "the error kind is logged: {text}");
    assert!(text.contains("latency_ms="), "{text}");
    assert!(
        !text.contains("secretword"),
        "no argument or message text reaches the log: {text}"
    );
}

#[test]
fn a_bad_frame_logs_one_warn_and_still_answers_and_quiet_methods_log_nothing_at_warn() {
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let sandbox = Sandbox::new("log-warn");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let frames = [
        "{not json".to_string(),
        "[]".to_string(),
        json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}).to_string(),
        request(json!(2), "no/such", json!({})),
    ];
    for frame in &frames {
        let captured = Captured::default();
        let _guard = captured.install(tracing::Level::WARN);
        let answer = call_unlocked(&store, frame).expect("the frame is still answered");
        assert!(answer["error"].is_object(), "{frame}: {answer}");
        let text = captured.text();
        assert_eq!(text.lines().count(), 1, "{frame}: one warn: {text}");
        assert!(text.contains(" WARN "), "{text}");
    }

    let captured = Captured::default();
    let _guard = captured.install(tracing::Level::WARN);
    call_unlocked(&store, &request(json!(3), "ping", json!({}))).expect("pong");
    call_unlocked(&store, &request(json!(4), "tools/list", json!({}))).expect("tools");
    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string();
    assert!(call_unlocked(&store, &note).is_none());
    assert_eq!(captured.text(), "", "healthy traffic is silent at warn");
}

#[test]
fn a_hostile_tool_name_is_logged_escaped_on_one_line() {
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let sandbox = Sandbox::new("log-hostile");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let captured = Captured::default();
    let _guard = captured.install(tracing::Level::WARN);

    let answer = call_unlocked(
        &store,
        &tool_call(1, "evil\n2026 INFO forged line\u{1b}[31m", json!({})),
    )
    .expect("an answer");
    assert!(answer["error"].is_object());
    let method = call_unlocked(&store, &request(json!(2), "bad\nmethod\u{7}", json!({})))
        .expect("an answer");
    assert!(method["error"].is_object());

    let text = captured.text();
    assert_eq!(text.lines().count(), 2, "one line per event: {text}");
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{7}'),
        "{text:?}"
    );
    assert!(text.contains("\\n"), "the newline is escaped: {text}");
}

/// A store the build wrote, then stamped one version past what it reads: the
/// shape a newer binary leaves after its first write.
fn stamp_newer(database: &std::path::Path) -> i64 {
    let ahead = tandem_core::store::CURRENT_VERSION + 1;
    let connection = rusqlite::Connection::open(database).expect("the store opens");
    connection
        .execute(
            "INSERT INTO schema_migrations (version, applied_at, author) VALUES (?1, '2026-11-16T00:00:00Z', 'newer')",
            [ahead],
        )
        .expect("the newer version is stamped");
    ahead
}

#[test]
fn a_newer_store_is_named_in_the_health_block_and_on_a_tool_call() {
    let sandbox = Sandbox::new("mcp-newer-store");
    sandbox
        .board()
        .add("seed", support::ticket())
        .expect("the seed lands");
    let ahead = stamp_newer(&sandbox.database());
    let store = sandbox.store();

    let asked = call(
        &store,
        &request(
            json!(1),
            "initialize",
            json!({"protocolVersion": "2025-03-26"}),
        ),
    )
    .expect("initialize answers");
    let health = &asked["result"]["health"];
    assert_eq!(health["storeOk"], false, "{health}");
    let detail = health["storeDetail"]
        .as_str()
        .expect("storeDetail is a string");
    assert!(detail.contains(&format!("schema v{ahead}")), "{detail}");
    assert!(detail.contains("Upgrade this binary"), "{detail}");
    assert!(!detail.contains("ADR"), "{detail}");

    let listed = call(&store, &tool_call(2, "ticket_list", json!({}))).expect("the call answers");
    assert_eq!(listed["result"]["isError"], true, "{listed}");
    let text = listed["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(text.contains("written by a newer Tandem"), "{text}");
    assert!(!text.contains("Try again"), "{text}");
}

#[test]
fn a_store_at_the_build_version_has_no_store_detail() {
    let sandbox = Sandbox::new("mcp-current-store");
    let asked = call(
        &sandbox.store(),
        &request(
            json!(1),
            "initialize",
            json!({"protocolVersion": "2025-03-26"}),
        ),
    )
    .expect("initialize answers");
    assert!(
        asked["result"]["health"]["storeDetail"].is_null(),
        "{asked}"
    );
}

// MARK: - Lean and full answers (MCP-3)

/// One tool call, its result text parsed.
fn result_of(store: &tandem_core::store::Store, name: &str, arguments: Value) -> (String, Value) {
    let text = tool_text(&call(store, &tool_call(1, name, arguments)).expect("the call answers"));
    let value = serde_json::from_str(&text).expect("the tool result is JSON");
    (text, value)
}

/// The same call asking for the lean shape, which is opt-in.
fn lean_of(store: &tandem_core::store::Store, name: &str, mut arguments: Value) -> (String, Value) {
    arguments["detail"] = json!("lean");
    result_of(store, name, arguments)
}

const SECRET_BRIEF: &str = "BRIEF-SENTINEL-the-description-an-agent-already-holds";

/// The ticket a write answered, with a description the agent sent.
fn filed(store: &tandem_core::store::Store) -> String {
    let (_, added) = lean_of(
        store,
        "ticket_add",
        json!({"project": "Main", "title": "Carries a brief", "description": SECRET_BRIEF}),
    );
    added["ticket"]["id"].as_str().expect("an id").to_string()
}

#[test]
fn no_write_tool_echoes_the_description_it_was_sent_and_a_comment_does_not_echo_its_body() {
    let sandbox = Sandbox::new("mcp-lean-writes");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    declare_label(&store, "epic");
    let other = lean_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "Other"}),
    )
    .1["ticket"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let id = filed(&store);

    let calls = [
        (
            "ticket_add",
            json!({"project": "Main", "title": "Again", "description": SECRET_BRIEF}),
        ),
        (
            "ticket_set",
            json!({"id": id, "status": "ready", "description": SECRET_BRIEF}),
        ),
        ("ticket_move", json!({"id": id, "label": "epic"})),
        ("ticket_block", json!({"id": id, "on": other})),
        ("ticket_unblock", json!({"id": id})),
        ("ticket_complete", json!({"id": id})),
        ("ticket_archive", json!({"id": id})),
    ];
    for (name, arguments) in calls {
        let (text, value) = lean_of(&store, name, arguments);
        assert!(
            !text.contains(SECRET_BRIEF),
            "{name} echoed the description: {text}"
        );
        assert!(value.get("revision").is_some(), "{name} names its revision");
        assert!(value["ticket"].get("description").is_none(), "{name}");
        assert!(value["ticket"].get("createdAt").is_none(), "{name}");
        assert_eq!(
            value["ticket"]
                .as_object()
                .map(|fields| fields.contains_key("status")),
            Some(true),
            "{name} says where the ticket stands"
        );
    }

    let id = filed(&store);
    let (text, comment) = lean_of(
        &store,
        "ticket_comment_add",
        json!({"id": id, "author": "builder", "body": "COMMENT-SENTINEL"}),
    );
    assert!(!text.contains("COMMENT-SENTINEL"), "{text}");
    assert_eq!(
        comment.as_object().map(|fields| {
            let mut keys: Vec<_> = fields.keys().cloned().collect();
            keys.sort();
            keys
        }),
        Some(vec!["at".to_string(), "author".into(), "revision".into()])
    );
}

#[test]
fn a_receipt_names_the_title_only_when_the_write_could_have_changed_it() {
    let sandbox = Sandbox::new("mcp-lean-title");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let (_, added) = lean_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "  Trim me  "}),
    );
    assert_eq!(added["ticket"]["title"], "Trim me", "the stored title");
    let id = added["ticket"]["id"].as_str().expect("an id").to_string();
    let (_, moved) = lean_of(&store, "ticket_set", json!({"id": id, "status": "ready"}));
    assert!(moved["ticket"].get("title").is_none());
    let (_, renamed) = lean_of(&store, "ticket_set", json!({"id": id, "title": "New"}));
    assert_eq!(renamed["ticket"]["title"], "New");
}

#[test]
fn a_default_open_row_drops_its_defaults_and_a_busy_row_keeps_its_own() {
    let sandbox = Sandbox::new("mcp-lean-rows");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    lean_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "Plain"}),
    );
    let plain = lean_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "Blocker"}),
    )
    .1["ticket"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let busy = lean_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "Busy", "assignee": "builder", "status": "ready"}),
    )
    .1["ticket"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    lean_of(&store, "ticket_block", json!({"id": busy, "on": plain}));
    sandbox
        .board()
        .set_link(&busy, Some("notes/busy.md"))
        .expect("the link is set");

    let (_, listed) = lean_of(&store, "ticket_list", json!({}));
    let rows = listed["tickets"].as_array().expect("rows");
    let row = |title: &str| {
        rows.iter()
            .find(|row| row["title"] == title)
            .unwrap_or_else(|| panic!("a row titled {title}"))
    };

    let plain_row = row("Plain");
    for dropped in [
        "archived",
        "assignee",
        "blocked",
        "blockedBy",
        "blocks",
        "completedAt",
        "done",
        "link",
    ] {
        assert!(plain_row.get(dropped).is_none(), "{dropped}: {plain_row}");
    }
    assert!(plain_row.get("ref").is_some(), "every ticket has a ref");
    assert!(plain_row.get("project").is_some(), "and a project");

    let busy_row = row("Busy");
    assert_eq!(busy_row["assignee"], "builder");
    assert_eq!(busy_row["blocked"], true);
    assert_eq!(busy_row["link"], "notes/busy.md");
    assert!(busy_row.get("done").is_none());
    let blocker = &busy_row["blockedBy"][0];
    assert_eq!(blocker["status"], "backlog");
    assert_eq!(
        blocker.as_object().map(|fields| fields.len()),
        Some(2),
        "a blocker is a name and a status: {blocker}"
    );
    assert_eq!(
        row("Plain")["blocks"],
        Value::Null,
        "the edge reads from one end per row"
    );
}

#[test]
fn limit_cuts_the_list_and_total_says_how_many_matched() {
    let sandbox = Sandbox::new("mcp-limit");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    for title in ["A", "B", "C"] {
        result_of(
            &store,
            "ticket_add",
            json!({"project": "Main", "title": title}),
        );
    }
    let (_, cut) = result_of(&store, "ticket_list", json!({"limit": 2}));
    assert_eq!(cut["tickets"].as_array().map(Vec::len), Some(2));
    assert_eq!(cut["total"], 3);
    let (_, whole) = result_of(&store, "ticket_list", json!({"limit": 3}));
    assert_eq!(whole["tickets"].as_array().map(Vec::len), Some(3));
    assert!(
        whole.get("total").is_none(),
        "an uncut list reports no total"
    );
}

#[test]
fn limit_zero_returns_no_rows_and_the_total() {
    let sandbox = Sandbox::new("mcp-limit-zero");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    for title in ["A", "B"] {
        result_of(
            &store,
            "ticket_add",
            json!({"project": "Main", "title": title}),
        );
    }
    let (_, none) = result_of(&store, "ticket_list", json!({"limit": 0}));
    assert_eq!(none["tickets"].as_array().map(Vec::len), Some(0));
    assert_eq!(none["total"], 2);
}

#[test]
fn an_absent_detail_answers_full_and_lean_is_opt_in() {
    let sandbox = Sandbox::new("mcp-default-full");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let (_, added) = result_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "T", "description": SECRET_BRIEF}),
    );
    assert_eq!(added["ticket"]["description"], SECRET_BRIEF);
    let id = added["ticket"]["id"].as_str().expect("an id").to_string();
    let (absent, _) = result_of(&store, "ticket_show", json!({"id": id}));
    let (full, _) = result_of(&store, "ticket_show", json!({"id": id, "detail": "full"}));
    assert_eq!(absent, full, "absent is full, byte for byte");
    let (lean, _) = lean_of(&store, "ticket_show", json!({"id": id}));
    assert_ne!(lean, full);
}

#[test]
fn detail_full_answers_the_whole_shape_for_every_affected_tool() {
    let sandbox = Sandbox::new("mcp-full");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    declare_label(&store, "epic");
    let other = result_of(
        &store,
        "ticket_add",
        json!({"project": "Main", "title": "Other"}),
    )
    .1["ticket"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let id = filed(&store);

    let full = json!({"detail": "full"});
    let with = |mut arguments: Value| {
        arguments["detail"] = full["detail"].clone();
        arguments
    };

    // The whole ticket: what `ticket show` always answered.
    let (_, shown) = result_of(&store, "ticket_show", with(json!({"id": id})));
    let whole_keys: Vec<String> = shown["ticket"]
        .as_object()
        .expect("a ticket")
        .keys()
        .cloned()
        .collect();
    assert!(whole_keys.len() >= 17, "{whole_keys:?}");
    for key in [
        "archived",
        "assignee",
        "blocked",
        "blockedBy",
        "done",
        "link",
    ] {
        assert!(whole_keys.iter().any(|k| k == key), "full keeps {key}");
    }
    assert_eq!(shown["ticket"]["description"], SECRET_BRIEF);

    let writes = [
        (
            "ticket_add",
            json!({"project": "Main", "title": "W", "description": SECRET_BRIEF}),
        ),
        ("ticket_set", json!({"id": id, "status": "ready"})),
        ("ticket_move", json!({"id": id, "label": "epic"})),
        ("ticket_block", json!({"id": id, "on": other})),
        ("ticket_unblock", json!({"id": id})),
        ("ticket_complete", json!({"id": id})),
        ("ticket_archive", json!({"id": id})),
    ];
    for (name, arguments) in writes {
        let (_, value) = result_of(&store, name, with(arguments));
        let keys: Vec<String> = value["ticket"]
            .as_object()
            .unwrap_or_else(|| panic!("{name} answers a ticket"))
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, whole_keys, "{name} full is the whole ticket");
    }

    let id = filed(&store);
    let (_, comment) = result_of(
        &store,
        "ticket_comment_add",
        with(json!({"id": id, "author": "builder", "body": "Landed"})),
    );
    assert_eq!(comment["comment"]["body"], "Landed");
    assert!(comment["comment"].get("at").is_some());

    let (_, listed) = result_of(&store, "ticket_list", with(json!({"filter": "all"})));
    let row = &listed["tickets"][0];
    let mut keys: Vec<&str> = row
        .as_object()
        .expect("a row")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "archived",
            "assignee",
            "blocked",
            "blockedBy",
            "blocks",
            "completedAt",
            "createdAt",
            "descriptionLength",
            "done",
            "id",
            "label",
            "link",
            "number",
            "origin",
            "project",
            "ref",
            "status",
            "title"
        ],
        "a full row is the 18 fields"
    );
    assert!(listed.get("total").is_none());
}

#[test]
fn an_unknown_detail_is_an_invalid_params_error_and_a_tool_without_one_ignores_it() {
    let sandbox = Sandbox::new("mcp-detail-bad");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let refused = call(
        &store,
        &tool_call(1, "ticket_list", json!({"detail": "everything"})),
    )
    .expect("answers");
    assert_eq!(refused["error"]["code"], -32602);
    let listed =
        call(&store, &tool_call(2, "label_list", json!({"detail": "x"}))).expect("answers");
    assert!(listed["error"].is_null(), "{listed}");
    for tool in mcp::tools() {
        let has = tool.input_schema["properties"].get("detail").is_some();
        let affected = [
            "ticket_list",
            "ticket_show",
            "ticket_add",
            "ticket_set",
            "ticket_move",
            "ticket_block",
            "ticket_unblock",
            "ticket_complete",
            "ticket_archive",
            "ticket_comment_add",
        ]
        .contains(&tool.name);
        assert_eq!(has, affected, "{}", tool.name);
    }
}

/// Add a ticket with `description` through the tool and return its id.
fn add_with_body(store: &tandem_core::store::Store, description: &str) -> String {
    let added: Value = serde_json::from_str(&tool_text(
        &call(
            store,
            &tool_call(
                1,
                "ticket_add",
                json!({"project": "Main", "title": "Brief", "description": description}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    added["ticket"]["id"].as_str().expect("an id").to_string()
}

#[test]
fn append_description_adds_to_the_body_and_reads_back_combined() {
    let sandbox = Sandbox::new("mcp-append");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let id = add_with_body(&store, "The brief.");

    let appended = call(
        &store,
        &tool_call(
            2,
            "ticket_set",
            json!({"id": id, "appendDescription": "A later paragraph."}),
        ),
    )
    .expect("the call answers");
    assert!(appended["result"]["isError"].is_null(), "{appended}");

    let shown: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(3, "ticket_show", json!({"id": id}))).expect("answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        shown["ticket"]["description"],
        "The brief.\n\nA later paragraph."
    );
}

#[test]
fn an_empty_append_description_is_refused_and_the_body_stays() {
    let sandbox = Sandbox::new("mcp-append-empty");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let id = add_with_body(&store, "The brief.");

    let refused = call(
        &store,
        &tool_call(2, "ticket_set", json!({"id": id, "appendDescription": ""})),
    )
    .expect("the call answers");
    assert_eq!(refused["result"]["isError"], true, "{refused}");

    let shown: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(3, "ticket_show", json!({"id": id}))).expect("answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(shown["ticket"]["description"], "The brief.");
}

#[test]
fn description_with_append_description_is_refused_and_nothing_is_written() {
    let sandbox = Sandbox::new("mcp-append-both");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    let id = add_with_body(&store, "The brief.");

    let refused = call(
        &store,
        &tool_call(
            2,
            "ticket_set",
            json!({"id": id, "description": "New", "appendDescription": "More"}),
        ),
    )
    .expect("the call answers");
    assert_eq!(refused["result"]["isError"], true, "{refused}");

    let shown: Value = serde_json::from_str(&tool_text(
        &call(&store, &tool_call(3, "ticket_show", json!({"id": id}))).expect("answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(shown["ticket"]["description"], "The brief.");
}

// MARK: - note_write's expected hash (MCP-5)

#[test]
fn note_write_with_the_hash_it_read_lands_and_a_stale_hash_is_refused_in_words() {
    let sandbox = Sandbox::new("mcp-note-hash");
    let store = {
        sandbox.board();
        sandbox.store()
    };

    let (_, created) = result_of(
        &store,
        "note_write",
        json!({"path": "Plan.md", "text": "v1", "expectedHash": "absent"}),
    );
    assert_eq!(created["write"]["path"], "Plan.md", "{created}");

    let (_, read) = result_of(&store, "note_read", json!({"path": "Plan.md"}));
    let hash = read["note"]["hash"].as_str().expect("a hash").to_string();

    let (_, landed) = result_of(
        &store,
        "note_write",
        json!({"path": "Plan.md", "text": "v2", "expectedHash": hash}),
    );
    assert_eq!(landed["write"]["path"], "Plan.md", "{landed}");

    // The same old hash again: the note moved on, so the write is refused.
    let refused = call(
        &store,
        &tool_call(
            2,
            "note_write",
            json!({"path": "Plan.md", "text": "v3", "expectedHash": hash}),
        ),
    )
    .expect("the call answers");
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let words = refused["result"]["content"][0]["text"]
        .as_str()
        .expect("the refusal carries text")
        .to_string();
    assert!(words.contains("Plan.md"), "names the path: {words}");
    assert!(words.contains("note_read"), "says to read again: {words}");

    let (_, after) = result_of(&store, "note_read", json!({"path": "Plan.md"}));
    assert_eq!(
        after["note"]["text"], "v2",
        "the refusal left the note alone"
    );

    // Omitting the hash still overwrites, so no caller breaks.
    let (_, forced) = result_of(
        &store,
        "note_write",
        json!({"path": "Plan.md", "text": "v4"}),
    );
    assert_eq!(forced["write"]["path"], "Plan.md", "{forced}");
}

// MARK: - note_search, note_backlinks and note_edit (MCP-6)

fn seed(store: &tandem_core::store::Store, path: &str, text: &str) {
    result_of(store, "note_write", json!({"path": path, "text": text}));
}

#[test]
fn note_search_answers_a_hit_with_a_snippet_an_empty_query_with_no_rows_and_cuts_at_limit() {
    let sandbox = Sandbox::new("mcp-note-search");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    seed(
        &store,
        "Plan.md",
        "Title\nWe decided to ship the Gizmo on Friday.",
    );
    seed(&store, "Other.md", "Gizmo Title\nbody");
    seed(&store, "Third.md", "Title\nnothing here");

    let (_, hit) = result_of(&store, "note_search", json!({"query": "gizmo"}));
    let hits = hit["hits"].as_array().expect("rows");
    assert_eq!(hits.len(), 2, "{hit}");
    assert_eq!(hits[0]["path"], "Other.md");
    assert_eq!(hits[0]["display"], "Gizmo Title");
    assert_eq!(hits[1]["path"], "Plan.md");
    assert!(
        hits[1]["display"]
            .as_str()
            .unwrap()
            .contains("Gizmo on Friday")
    );
    assert!(hit.get("total").is_none(), "total only when cut: {hit}");

    let (_, empty) = result_of(&store, "note_search", json!({"query": ""}));
    assert_eq!(empty["hits"], json!([]), "{empty}");

    let (_, cut) = result_of(&store, "note_search", json!({"query": "gizmo", "limit": 1}));
    assert_eq!(cut["hits"].as_array().unwrap().len(), 1, "{cut}");
    assert_eq!(cut["total"], 2);

    let answer = call(&store, &tool_call(2, "note_search", json!({}))).unwrap();
    assert_eq!(
        answer["result"]["isError"], true,
        "a missing query is refused"
    );
}

#[cfg(unix)]
#[test]
fn note_search_does_not_follow_a_link_out_of_the_vault() {
    let sandbox = Sandbox::new("mcp-note-search-link");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    seed(&store, "Inside.md", "Title\nbody");
    let outside = sandbox.root().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("Secret.md"), "Title\nneedle-outside").unwrap();
    let vault = sandbox.home().directory().join("bikeshed");
    std::os::unix::fs::symlink(&outside, vault.join("link")).unwrap();
    let (_, found) = result_of(&store, "note_search", json!({"query": "needle-outside"}));
    assert_eq!(found["hits"], json!([]), "{found}");
}

#[test]
fn note_backlinks_answers_a_row_per_linking_note() {
    let sandbox = Sandbox::new("mcp-note-backlinks");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    seed(&store, "Target.md", "Target\n");
    seed(&store, "Source.md", "Source\nsee [[Target]] here");
    let (_, back) = result_of(&store, "note_backlinks", json!({"path": "Target.md"}));
    assert_eq!(back["path"], "Target.md");
    assert_eq!(back["backlinks"][0]["path"], "Source.md", "{back}");
    assert!(
        back["backlinks"][0]["line"]
            .as_str()
            .unwrap()
            .contains("[[Target]]")
    );
}

#[test]
fn note_edit_replaces_one_match_and_answers_the_path_and_hash_only() {
    let sandbox = Sandbox::new("mcp-note-edit");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    seed(&store, "Plan.md", "Title\nstatus: draft\nowner: ann\n");
    let (text, edited) = result_of(
        &store,
        "note_edit",
        json!({"path": "Plan.md", "oldText": "status: draft", "newText": "status: final-sentinel"}),
    );
    assert_eq!(edited["write"]["path"], "Plan.md", "{edited}");
    assert!(
        !text.contains("final-sentinel"),
        "no echo of the text: {text}"
    );
    let (_, read) = result_of(&store, "note_read", json!({"path": "Plan.md"}));
    assert_eq!(
        read["note"]["text"],
        "Title\nstatus: final-sentinel\nowner: ann\n"
    );
    assert_eq!(read["note"]["hash"], edited["write"]["hash"]);

    let (_, appended) = result_of(
        &store,
        "note_edit",
        json!({"path": "Plan.md", "oldText": "", "newText": "- added\n"}),
    );
    let (_, read) = result_of(&store, "note_read", json!({"path": "Plan.md"}));
    assert!(
        read["note"]["text"]
            .as_str()
            .unwrap()
            .ends_with("owner: ann\n- added\n")
    );
    assert_eq!(read["note"]["hash"], appended["write"]["hash"]);
}

#[test]
fn note_edit_refuses_zero_and_two_matches_and_leaves_the_bytes_alone() {
    let sandbox = Sandbox::new("mcp-note-edit-refuse");
    let store = {
        sandbox.board();
        sandbox.store()
    };
    seed(&store, "Plan.md", "Title\ntodo\ntodo\n");
    let on_disk = sandbox.home().directory().join("bikeshed").join("Plan.md");
    let before = std::fs::read(&on_disk).unwrap();
    for (old, word) in [("absent", "not in"), ("todo", "2 times")] {
        let answer = call(
            &store,
            &tool_call(
                2,
                "note_edit",
                json!({"path": "Plan.md", "oldText": old, "newText": "done"}),
            ),
        )
        .expect("the call answers");
        assert_eq!(answer["result"]["isError"], true, "{answer}");
        let words = answer["result"]["content"][0]["text"].as_str().unwrap();
        assert!(words.contains(word), "{words}");
        assert_eq!(std::fs::read(&on_disk).unwrap(), before, "bytes unchanged");
    }
}
