//! The MCP door's protocol, without a process: one JSON-RPC frame in, one
//! frame out (or silence for a notification).
//!
//! These are the claims the pipe test cannot isolate:
//!
//! - `initialize` negotiates the protocol version instead of assuming it;
//! - `tools/list` names the 48 tools with object-rooted schemas;
//! - `tools/call` drives the whole ticket loop through the `rpc` door, and a
//!   domain refusal is a result with `isError`, not a JSON-RPC error;
//! - `session.end` reads the harness by default and stamps without usage when
//!   the harness has no row, never failing;
//! - a frame the door cannot route is a JSON-RPC error and never a panic.

mod support;

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::mcp;
use tandem_core::store::Actor;

fn environment() -> tandem_core::setup::Environment {
    tandem_core::setup::Environment::new()
}

fn environment_with_bundle(bundle: &std::path::Path) -> tandem_core::setup::Environment {
    let mut environment = tandem_core::setup::Environment::new();
    environment.insert(
        "TANDEM_AGENT_BUNDLE".to_string(),
        bundle.to_string_lossy().into_owned(),
    );
    environment
}

fn call(store: &tandem_core::store::Store, frame: &str) -> Option<Value> {
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

fn tool_call_meta(id: i64, name: &str, arguments: Value, harness_session: &str) -> String {
    request(
        json!(id),
        "tools/call",
        json!({
            "name": name,
            "arguments": arguments,
            "_meta": {"ai.opencode/sessionID": harness_session},
        }),
    )
}

#[test]
fn session_verbs_resolve_the_harness_session_from_the_call_meta() {
    let sandbox = Sandbox::new("mcp-harness-meta");
    let store = sandbox.store();

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(1, "ticket_add", json!({"title": "Carried"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let ticket = added["ticket"]["id"].as_str().expect("an id").to_string();

    let started: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                2,
                "session_start",
                json!({"project": "tandem", "agent": "builder", "brief": "A pass.", "harnessSession": "ses_meta_link"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let record = started["session"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    assert_eq!(
        started["session"]["harnessSessionId"], "ses_meta_link",
        "the record carries the harness pointer: {started}"
    );

    let linked: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call_meta(
                3,
                "session_link",
                json!({"ticket": ticket}),
                "ses_meta_link",
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        linked["session"]["id"], record,
        "no session param still links the caller's record: {linked}"
    );

    let shown: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call_meta(4, "session_show", json!({}), "ses_meta_link"),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        shown["session"]["id"], record,
        "no session param still shows the caller's record: {shown}"
    );
}

#[test]
fn sprint_open_records_the_harness_session_from_the_call_meta() {
    let sandbox = Sandbox::new("mcp-sprint-meta");
    let store = sandbox.store();

    let started: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                1,
                "session_start",
                json!({"project": "tandem", "agent": "builder", "brief": "A pass.", "harnessSession": "ses_meta_sprint"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let record = started["session"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let opened: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call_meta(
                2,
                "sprint_open",
                json!({"lane": "Tandem", "mode": "serial"}),
                "ses_meta_sprint",
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(
        opened["sprint"]["session"], record,
        "no session param still records the opener's record, not the pointer and not null: {opened}"
    );

    let refused = call(
        &store,
        &tool_call(
            3,
            "sprint_open",
            json!({"lane": "Tandem", "session": "no-such-record"}),
        ),
    )
    .expect("the call answers");
    assert_eq!(
        refused["result"]["isError"], true,
        "an unknown session is refused, not recorded: {refused}"
    );
}

#[test]
fn the_call_meta_beats_a_stale_process_environment() {
    let sandbox = Sandbox::new("mcp-meta-precedence");
    let store = sandbox.store();

    for (id, harness) in [(4, "ses_stale_env"), (5, "ses_fresh_call")] {
        let _ = tool_text(
            &call(
                &store,
                &tool_call(
                    id,
                    "session_start",
                    json!({"project": "tandem", "brief": "A pass.", "harnessSession": harness}),
                ),
            )
            .expect("the call answers"),
        );
    }

    let mut stale = tandem_core::setup::Environment::new();
    stale.insert(
        "OPENCODE_SESSION_ID".to_string(),
        "ses_stale_env".to_string(),
    );
    let answer = mcp::handle_mcp(
        &tool_call_meta(6, "session_show", json!({}), "ses_fresh_call"),
        &store,
        &Actor::Human,
        &stale,
    )
    .expect("the call answers");
    let answer: Value = serde_json::from_str(&answer).expect("the answer is JSON");
    let shown: Value = serde_json::from_str(&tool_text(&answer)).expect("the tool result is JSON");
    assert_eq!(
        shown["session"]["harnessSessionId"], "ses_fresh_call",
        "the caller's pointer beats the server's spawn environment: {shown}"
    );
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
        health["tools"], 48,
        "the health block names the tool count: {health}"
    );
}

#[test]
fn session_start_carries_a_small_health_block() {
    let sandbox = Sandbox::new("mcp-session-start-health");
    let store = sandbox.store();

    let started = call(
        &store,
        &tool_call(
            1,
            "session_start",
            json!({"project": "tandem", "agent": "builder", "brief": "A pass."}),
        ),
    )
    .expect("session_start answers");
    let result: Value =
        serde_json::from_str(&tool_text(&started)).expect("the tool result is JSON");
    let health = &result["health"];
    assert!(
        health.is_object(),
        "session_start carries a health block beside the session: {result}"
    );
    assert_eq!(
        health["tools"], 48,
        "the health block names the tool count: {health}"
    );
}

#[test]
fn initialize_reports_the_bundled_release_version_with_crate_fallback() {
    let sandbox = Sandbox::new("mcp-release-version");
    let store = sandbox.store();

    let bundle = sandbox.root().join("bundle");
    std::fs::create_dir_all(&bundle).expect("the bundle is creatable");
    std::fs::write(bundle.join("VERSION"), "1.6.0\n").expect("VERSION is writable");
    let bundled_env = environment_with_bundle(&bundle);
    let answered = mcp::handle_mcp(
        &request(
            serde_json::json!(1),
            "initialize",
            serde_json::json!({"protocolVersion": "2025-11-25"}),
        ),
        &store,
        &Actor::Human,
        &bundled_env,
    )
    .expect("initialize answers");
    let answered: Value = serde_json::from_str(&answered).expect("the answer is JSON");
    assert_eq!(
        answered["result"]["serverInfo"]["version"], "1.6.0",
        "the bundled VERSION marker is the release version"
    );

    let fallback = call(
        &store,
        &request(
            json!(2),
            "initialize",
            json!({"protocolVersion": "2025-11-25"}),
        ),
    )
    .expect("initialize answers");
    assert_eq!(
        fallback["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION"),
        "no marker is the crate version"
    );
}

#[test]
fn a_notification_is_answered_with_silence() {
    let sandbox = Sandbox::new("mcp-notification");
    let store = sandbox.store();

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
fn tools_list_names_the_48_tools_with_object_schemas() {
    let sandbox = Sandbox::new("mcp-tools-list");
    let store = sandbox.store();

    let listed = call(&store, &request(json!(1), "tools/list", json!({}))).expect("tools answer");
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    assert_eq!(tools.len(), 48);
    for tool in tools {
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
fn tools_call_files_reads_sets_and_completes_a_ticket() {
    let sandbox = Sandbox::new("mcp-ticket-loop");
    let store = sandbox.store();

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(1, "ticket_add", json!({"title": "First"})),
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
    let store = sandbox.store();

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
    let store = sandbox.store();

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
    let store = sandbox.store();

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
    let store = sandbox.store();

    let refused = call(&store, "{ not json").expect("it answers");
    assert!(refused["id"].is_null());
    assert_eq!(refused["error"]["code"], -32700);

    let scalar = call(&store, "42").expect("it answers");
    assert_eq!(scalar["error"]["code"], -32600);
}

#[test]
fn session_end_reads_the_harness_by_default_and_stamps_without_it() {
    let sandbox = Sandbox::new("mcp-session-harness");
    let store = sandbox.store();

    let started: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                1,
                "session_start",
                json!({"project": "tandem", "agent": "builder", "brief": "A pass."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let id = started["session"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let linked = call(
        &store,
        &tool_call(2, "session_link", json!({"ticket": id, "session": id})),
    )
    .expect("the call answers");
    assert!(
        linked["result"]["isError"] == true,
        "a ticket that does not exist is a tool error: {linked}"
    );

    let ended: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                3,
                "session_end",
                json!({"session": id, "outcome": "succeeded"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(ended["session"]["status"], "ended");
    assert!(
        ended["session"].get("tokens").is_none(),
        "no harness row is no usage, not invented zeros: {ended}"
    );
    assert!(
        ended["note"]
            .as_str()
            .unwrap_or("")
            .contains("without usage"),
        "the stamp says the harness had nothing: {ended}"
    );
}

#[test]
fn tools_call_drives_the_sprint_record_and_a_capture() {
    let sandbox = Sandbox::new("mcp-sprint");
    let store = sandbox.store();

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(1, "ticket_add", json!({"title": "Batch"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let ticket = added["ticket"]["id"].as_str().expect("an id").to_string();

    let started: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                2,
                "session_start",
                json!({"project": "tandem", "brief": "A pass."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let session = started["session"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let opened: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                3,
                "sprint_open",
                json!({"lane": "Tandem", "session": session}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(opened["sprint"]["lane"], "Tandem");
    assert_eq!(opened["sprint"]["status"], "open");

    let drawn: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                4,
                "sprint_ticket",
                json!({"lane": "Tandem", "ticket": ticket}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(drawn["sprint"]["batch"], json!([ticket]));

    let ran: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                5,
                "sprint_run",
                json!({"lane": "Tandem", "note": "First entry."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert!(
        ran["sprint"]["runs"].as_array().expect("runs").len() == 1,
        "the entry lands on the run log: {ran}"
    );

    let shown: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(6, "sprint_show", json!({"lane": "Tandem"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(shown["sprint"]["batch"], json!([ticket]));

    let listed: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(7, "sprint_list", json!({"lane": "Tandem"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(listed["sprints"].as_array().expect("sprints").len(), 1);

    let captured: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                8,
                "note_capture",
                json!({"kind": "fact", "project": "tandem", "session": session, "body": "The door works."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert!(
        captured["path"].as_str().expect("a path").ends_with(".md"),
        "the capture files a note: {captured}"
    );

    let closed: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                9,
                "sprint_close",
                json!({"lane": "Tandem", "note": "Landed."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(closed["sprint"]["status"], "closed");
}

#[test]
fn session_end_with_explicit_usage_stores_it_and_posts_the_usage_comment() {
    let sandbox = Sandbox::new("mcp-session-usage");
    let store = sandbox.store();

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(1, "ticket_add", json!({"title": "Carried"})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let ticket = added["ticket"]["id"].as_str().expect("an id").to_string();

    let started: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                2,
                "session_start",
                json!({"project": "tandem", "brief": "A pass."}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let id = started["session"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let _ = tool_text(
        &call(
            &store,
            &tool_call(3, "session_link", json!({"ticket": ticket, "session": id})),
        )
        .expect("the call answers"),
    );

    let ended: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(
                4,
                "session_end",
                json!({"session": id, "outcome": "succeeded", "summary": "Done.", "usageFromHarness": false, "tokensInput": 10, "tokensOutput": 5, "model": "probe"}),
            ),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    assert_eq!(ended["session"]["tokens"]["input"], 10);
    assert_eq!(ended["session"]["model"], "probe");
    assert!(
        ended.get("note").is_none(),
        "explicit usage needs no harness note: {ended}"
    );

    let comments: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(5, "ticket_comment_list", json!({"id": ticket})),
        )
        .expect("the call answers"),
    ))
    .expect("the tool result is JSON");
    let bodies: Vec<&str> = comments["comments"]
        .as_array()
        .expect("comments")
        .iter()
        .filter_map(|comment| comment["body"].as_str())
        .collect();
    assert!(
        bodies.iter().any(|body| body.contains(&id)),
        "the stamp posts the usage comment on the carried ticket: {bodies:?}"
    );
}
