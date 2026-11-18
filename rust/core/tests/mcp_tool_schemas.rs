//! What `tools/list` promises a host: which fields a call must carry, which
//! values a closed set allows, and which tools may run without asking the human.
//!
//! The annotations are the security claim. A host that reads `readOnlyHint:
//! true` may run the tool unprompted, so a writing tool marked read-only is an
//! approval bypass. The tests pin every flag from three sides: an inline
//! golden of the whole list (a new tool cannot ship unannotated or unpinned),
//! a read-only set named by wire method, and a behavioural check that every
//! read-only tool leaves the store's revision where it was.

mod support;

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::domain::query::Filter;
use tandem_core::domain::ticket::Status;
use tandem_core::env::Environment;
use tandem_core::mcp;
use tandem_core::store::Actor;

fn frame(store: &tandem_core::store::Store, method: &str, params: Value) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let answer = mcp::handle_mcp(
        &request.to_string(),
        store,
        &Actor::Human,
        &Environment::new(),
    )
    .expect("a request is answered");
    serde_json::from_str(&answer).expect("one JSON frame")
}

fn listed(store: &tandem_core::store::Store) -> Vec<Value> {
    frame(store, "tools/list", json!({}))["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .clone()
}

/// One line per tool: `name read-only/destructive/idempotent required,fields`.
/// A `-` stands for none. The whole served list, so adding, renaming or
/// re-flagging a tool fails here until the golden is edited on purpose.
fn projection(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|tool| {
            let flags = &tool["annotations"];
            let flag = |key: &str| {
                flags[key]
                    .as_bool()
                    .unwrap_or_else(|| panic!("{} carries {key} as a boolean", tool["name"]))
            };
            let required: Vec<&str> = tool["inputSchema"]["required"]
                .as_array()
                .expect("required is an array")
                .iter()
                .map(|name| name.as_str().expect("a required name"))
                .collect();
            assert_eq!(
                flags["openWorldHint"],
                json!(false),
                "the store is a closed domain: {}",
                tool["name"]
            );
            format!(
                "{} {}{}{} {}",
                tool["name"].as_str().unwrap(),
                if flag("readOnlyHint") { "R" } else { "-" },
                if flag("destructiveHint") { "D" } else { "-" },
                if flag("idempotentHint") { "I" } else { "-" },
                if required.is_empty() {
                    "-".to_string()
                } else {
                    required.join(",")
                }
            )
        })
        .collect()
}

#[test]
fn tools_list_is_pinned_flag_by_flag_and_field_by_field() {
    let sandbox = Sandbox::new("mcp-schema-golden");
    let store = sandbox.store();
    let got = projection(&listed(&store));
    let want = [
        "ticket_list R-I -",
        "ticket_show R-I id",
        "ticket_add --- title",
        "ticket_set --I id",
        "ticket_move --- id",
        "ticket_block --I id,on",
        "ticket_unblock -D- id",
        "ticket_complete --I id",
        "ticket_archive -D- -",
        "ticket_comment_add --- id,body",
        "ticket_comment_list R-I id",
        "label_list R-I -",
        "label_add --- name",
        "label_rename --- from,to",
        "label_delete -D- name",
        "label_move --- name",
        "project_list R-I -",
        "project_add --- name",
        "project_rename --- from,to",
        "project_delete -D- name",
        "doctor_report R-I -",
        "note_tree R-I -",
        "note_read R-I path",
        "note_write -D- path,text",
        "note_create --- path",
        "note_create_folder --- path",
        "note_move -D- path,into",
        "note_health R-I -",
    ];
    assert_eq!(got, want, "tools/list changed: edit the golden on purpose");
}

#[test]
fn a_tool_is_read_only_only_when_its_wire_method_takes_no_write() {
    // The reads, by wire method. Anything not named here is a write, and a
    // write marked read-only is an approval bypass.
    const READS: [&str; 9] = [
        "ticket.list",
        "ticket.show",
        "ticket.comment.list",
        "label.list",
        "project.list",
        "doctor.report",
        "note.tree",
        "note.read",
        "note.health",
    ];
    for tool in mcp::tools() {
        assert_eq!(
            tool.annotations.read_only,
            READS.contains(&tool.method_tag),
            "{}: readOnlyHint must be true exactly for the reads",
            tool.name
        );
        assert!(
            !(tool.annotations.read_only && tool.annotations.destructive),
            "{}: a read cannot be destructive",
            tool.name
        );
    }
}

#[test]
fn a_read_only_tool_leaves_the_store_where_it_was() {
    let sandbox = Sandbox::new("mcp-schema-readonly");
    let store = sandbox.store();
    let revision = |store: &tandem_core::store::Store| {
        frame(
            store,
            "tools/call",
            json!({"name": "ticket_list", "arguments": {"filter": "all"}}),
        )["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string()
    };
    frame(
        &store,
        "tools/call",
        json!({"name": "ticket_add", "arguments": {"title": "seed"}}),
    );
    let before = revision(&store);
    for tool in mcp::tools()
        .iter()
        .filter(|tool| tool.annotations.read_only)
    {
        frame(
            &store,
            "tools/call",
            json!({"name": tool.name, "arguments": {}}),
        );
    }
    assert_eq!(before, revision(&store), "a read-only tool wrote");
}

#[test]
fn a_call_missing_a_required_field_is_still_refused_by_the_domain() {
    let sandbox = Sandbox::new("mcp-schema-required");
    let store = sandbox.store();
    for tool in mcp::tools() {
        let required = tool.input_schema["required"].as_array().expect("array");
        let answer = frame(
            &store,
            "tools/call",
            json!({"name": tool.name, "arguments": {}}),
        );
        assert!(
            answer["error"].is_null(),
            "{}: a missing field is a domain refusal, not a protocol error",
            tool.name
        );
        let refused = answer["result"]["isError"] == json!(true);
        assert_eq!(
            refused,
            !required.is_empty(),
            "{}: required {required:?} but the domain answered {answer}",
            tool.name
        );
    }
}

#[test]
fn the_enums_equal_the_domains_sets() {
    let sandbox = Sandbox::new("mcp-schema-enums");
    let store = sandbox.store();
    let tools = listed(&store);
    let enum_of = |tool: &str, field: &str| -> Vec<String> {
        tools
            .iter()
            .find(|candidate| candidate["name"] == tool)
            .expect("the tool is served")["inputSchema"]["properties"][field]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{tool}.{field} is an enum"))
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect()
    };
    let statuses: Vec<String> = Status::WORKFLOW
        .iter()
        .map(|status| status.as_str().to_string())
        .collect();
    assert_eq!(enum_of("ticket_add", "status"), statuses);
    assert_eq!(enum_of("ticket_set", "status"), statuses);
    let filters = enum_of("ticket_list", "filter");
    for value in &filters {
        assert!(
            Filter::parse(Some(value)).is_ok(),
            "{value} is a filter the domain parses"
        );
    }
    assert_eq!(filters.len(), 3, "open, done and all");
    // ticket_list.status is a comma-separated set, so it stays a plain string.
    let list = tools.iter().find(|t| t["name"] == "ticket_list").unwrap();
    assert!(list["inputSchema"]["properties"]["status"]["enum"].is_null());
}
