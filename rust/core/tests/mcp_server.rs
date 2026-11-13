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

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::env::Environment;
use tandem_core::mcp;
use tandem_core::store::Actor;

fn environment() -> Environment {
    Environment::new()
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
        health["tools"], 28,
        "the health block names the tool count: {health}"
    );
}

#[test]
fn initialize_reports_the_release_version_or_the_crate_version() {
    let sandbox = Sandbox::new("mcp-release-version");
    let store = sandbox.store();

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
fn tools_list_names_the_28_tools_with_object_schemas() {
    let sandbox = Sandbox::new("mcp-tools-list");
    let store = sandbox.store();

    let listed = call(&store, &request(json!(1), "tools/list", json!({}))).expect("tools answer");
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array");
    assert_eq!(tools.len(), 28);
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
fn tools_call_drives_a_ticket_and_the_note_tools() {
    let sandbox = Sandbox::new("mcp-notes");
    let store = sandbox.store();

    let added: Value = serde_json::from_str(&tool_text(
        &call(
            &store,
            &tool_call(1, "ticket_add", json!({"title": "Batch"})),
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
