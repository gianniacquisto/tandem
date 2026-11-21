//! docs/mcp.md documents every tool in `tools()`: a heading per tool, its
//! description, what a host may assume about it, and a table of its
//! parameters with their types, allowed values and descriptions. The section
//! is generated from the registry, so it cannot drift: the block between the
//! markers must equal what the registry renders, in order.
//!
//! Regenerate after a tool change with `TANDEM_BLESS=1 cargo test -p tandem-core --test mcp_doc`.

use serde_json::Value;
use tandem_core::mcp::{self, Tool};

const PAGE: &str = include_str!("../../../docs/mcp.md");
const BEGIN: &str = "<!-- tools:begin -->";
const END: &str = "<!-- tools:end -->";

/// The group a tool is listed under, from the word before its first underscore.
fn group(name: &str) -> &'static str {
    match name.split('_').next().unwrap_or(name) {
        "ticket" => "Tickets",
        "label" => "Labels",
        "project" => "Projects",
        "note" => "Notes",
        "doctor" => "Diagnostics",
        other => panic!("tool {name}: no group for the prefix {other:?}; add one to mcp_doc.rs"),
    }
}

/// A table cell: one line, with the table's own separator escaped.
fn cell(text: &str) -> String {
    text.replace('\n', " ").replace('|', "\\|")
}

/// A parameter's type, with its allowed values when the schema lists them.
fn kind(schema: &Value) -> String {
    let base = schema["type"].as_str().unwrap_or("any");
    match schema["enum"].as_array() {
        Some(values) => {
            let values: Vec<String> = values
                .iter()
                .filter_map(Value::as_str)
                .map(|value| format!("`{value}`"))
                .collect();
            format!("{base}: {}", values.join(", "))
        }
        None => base.to_string(),
    }
}

/// What a host may assume about the call, from its annotations.
fn hints(tool: &Tool) -> &'static str {
    let a = &tool.annotations;
    match (a.read_only, a.destructive, a.idempotent) {
        (true, _, _) => "Read-only.",
        (false, true, true) => {
            "Writes. Destructive: it removes or overwrites. Idempotent: repeating it changes nothing more."
        }
        (false, true, false) => "Writes. Destructive: it removes or overwrites.",
        (false, false, true) => "Writes. Idempotent: repeating it changes nothing more.",
        (false, false, false) => "Writes.",
    }
}

fn render_tool(tool: &Tool) -> String {
    let required: Vec<&str> = tool.input_schema["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut out = format!(
        "#### `{}`\n\n{}\n\n{}\n\n",
        tool.name,
        tool.description,
        hints(tool)
    );
    match tool.input_schema["properties"].as_object() {
        Some(properties) if !properties.is_empty() => {
            out.push_str(
                "| Parameter | Type | Required | Description |\n| --- | --- | --- | --- |\n",
            );
            for (name, schema) in properties {
                let needed = if required.contains(&name.as_str()) {
                    "yes"
                } else {
                    "no"
                };
                out.push_str(&format!(
                    "| `{name}` | {} | {needed} | {} |\n",
                    cell(&kind(schema)),
                    cell(schema["description"].as_str().unwrap_or(""))
                ));
            }
        }
        _ => out.push_str("No parameters.\n"),
    }
    out
}

fn render() -> String {
    let mut out = String::new();
    let mut current = "";
    for tool in mcp::tools() {
        let heading = group(tool.name);
        if heading != current {
            out.push_str(&format!("### {heading}\n\n"));
            current = heading;
        }
        out.push_str(&render_tool(tool));
        out.push('\n');
    }
    out.trim().to_string()
}

fn block() -> String {
    let start = PAGE.find(BEGIN).expect("docs/mcp.md has the begin marker") + BEGIN.len();
    let end = PAGE.find(END).expect("docs/mcp.md has the end marker");
    // A Windows checkout may turn the page's LF into CRLF.
    PAGE[start..end].replace("\r\n", "\n").trim().to_string()
}

#[test]
fn the_page_lists_every_tool_in_the_registry() {
    let expected = render();
    if std::env::var_os("TANDEM_BLESS").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/mcp.md");
        let start = PAGE.find(BEGIN).expect("begin marker") + BEGIN.len();
        let end = PAGE.find(END).expect("end marker");
        let blessed = format!("{}\n{}\n{}", &PAGE[..start], expected, &PAGE[end..]);
        std::fs::write(path, blessed).expect("write docs/mcp.md");
        return;
    }
    assert_eq!(
        block(),
        expected,
        "docs/mcp.md has drifted from tools(); regenerate with TANDEM_BLESS=1 cargo test -p tandem-core --test mcp_doc"
    );
}
