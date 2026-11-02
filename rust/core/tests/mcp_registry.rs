//! The MCP tool registry: one tool per covered wire method, pinned.
//!
//! The brief's contract, three assertions plus the exclusion pin:
//!
//! - every covered [`Method`](tandem_core::wire::rpc::Method) has a tool;
//! - every tool names a covered method, and the count is exactly 29;
//! - every tool's schema derives from its params struct: the schema's
//!   property set is the struct's serialized field set, and `required` is
//!   empty, because every wire param is optional at parse (an absent field is
//!   the CLI's absent flag, and the domain refuses what must carry text).
//!
//! Methods that stay wire-only are named here, not forgotten: `setup.install`
//! (host mutation stays a human CLI act), the curated note writes
//! (`note.write`, `note.move`, and the CLI's `note rename`, which has no wire
//! row and stays out), the vault's link-graph read (`note.health`) and the
//! settings focus (both stay CLI reads in v1), and the app's reads
//! (`usage.report`, `settings.get`, `markdown.blocks`, `note.tree`,
//! `note.read`, `note.backlinks`, `note.recoveries`, `setup.status`,
//! `doctor.report`, `session.transcript`).

use serde_json::Value;
use tandem_core::wire::rpc::METHODS;

/// The wire methods the MCP door covers: the 17 ticket, comment and lane
/// verbs on the wire today, the 5 session verbs, the 6 sprint verbs (three of
/// which already rode the wire for the app) and the capture verb.
const COVERED: [&str; 29] = [
    "ticket.list",
    "ticket.show",
    "ticket.add",
    "ticket.addFromNote",
    "ticket.set",
    "ticket.move",
    "ticket.block",
    "ticket.unblock",
    "ticket.complete",
    "ticket.answer",
    "ticket.archive",
    "ticket.comment.add",
    "ticket.comment.list",
    "lane.list",
    "lane.add",
    "lane.rename",
    "lane.delete",
    "session.start",
    "session.link",
    "session.end",
    "session.list",
    "session.show",
    "sprint.open",
    "sprint.ticket",
    "sprint.run",
    "sprint.close",
    "sprint.list",
    "sprint.show",
    "note.capture",
];

/// The wire methods with no tool. Every row of [`METHODS`] is either here or
/// in [`COVERED`]; a new wire method fails this file until it picks a side.
/// `session.transcript` stays wire-only for the sheet (TAN-264): the app reads
/// it, and the MCP door lists sessions but does not page transcripts in v1.
const EXCLUDED: [&str; 15] = [
    "setup.install",
    "note.write",
    "note.move",
    "note.health",
    "settings.set",
    "usage.report",
    "settings.get",
    "markdown.blocks",
    "note.tree",
    "note.read",
    "note.backlinks",
    "note.recoveries",
    "setup.status",
    "doctor.report",
    "session.transcript",
];

/// The tool name for a covered tag. Mechanical (`ticket.list` becomes
/// `ticket_list`) except `ticket.addFromNote`, whose tool is the CLI verb it
/// shares (`ticket file`), not its wire spelling.
fn tool_name(tag: &str) -> String {
    if tag == "ticket.addFromNote" {
        return "ticket_file".to_string();
    }
    tag.replace('.', "_").replace('-', "_")
}

#[test]
fn every_covered_method_has_a_tool_with_its_name() {
    let tools = tandem_core::mcp::tools();
    for tag in COVERED {
        let name = tool_name(tag);
        assert!(
            tools
                .iter()
                .any(|tool| tool.method_tag == tag && tool.name == name),
            "the covered method `{tag}` has a tool named `{name}`"
        );
    }
}

#[test]
fn every_tool_has_a_covered_method_and_the_count_is_29() {
    let tools = tandem_core::mcp::tools();
    assert_eq!(
        tools.len(),
        COVERED.len(),
        "one tool per covered method, no more"
    );
    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), tools.len(), "tool names are unique");
    for tool in &tools {
        assert!(
            COVERED.contains(&tool.method_tag),
            "the tool `{}` names the covered method `{}`",
            tool.name,
            tool.method_tag
        );
        assert_eq!(
            tool.name,
            tool_name(tool.method_tag),
            "the tool name is the method tag in snake case"
        );
    }
}

#[test]
fn every_tool_schema_matches_its_params_struct() {
    let tools = tandem_core::mcp::tools();
    for tool in &tools {
        let schema = &tool.input_schema;
        assert_eq!(
            schema["type"], "object",
            "the tool `{}` takes an object",
            tool.name
        );
        let params = tool.default_params();
        let fields: Vec<String> = params
            .as_object()
            .unwrap_or_else(|| panic!("the params of `{}` are an object", tool.name))
            .keys()
            .cloned()
            .collect();
        let properties: Vec<String> = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("the tool `{}` has properties", tool.name))
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            properties, fields,
            "the tool `{}` derives its properties from its params struct",
            tool.name
        );
        assert_eq!(
            schema["required"],
            Value::Array(Vec::new()),
            "the tool `{}` requires nothing: every wire param is optional at parse",
            tool.name
        );
    }
}

#[test]
fn the_excluded_methods_have_no_tool_and_nothing_is_forgotten() {
    let tools = tandem_core::mcp::tools();
    for tag in EXCLUDED {
        assert!(
            tools.iter().all(|tool| tool.method_tag != tag),
            "the wire-only method `{tag}` has no tool"
        );
    }
    assert!(
        tools.iter().all(|tool| tool.name != "note_rename"),
        "the CLI-only `note rename` has no tool"
    );
    let mut table: Vec<&str> = METHODS.iter().map(|(tag, _)| *tag).collect();
    table.sort_unstable();
    let mut pinned: Vec<&str> = COVERED.into_iter().chain(EXCLUDED).collect();
    pinned.sort_unstable();
    assert_eq!(
        table, pinned,
        "every wire method is either covered or named as excluded"
    );
}
