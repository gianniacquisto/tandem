//! docs/mcp.md lists every tool in `tools()`, one line each with its parameter
//! names. The list is generated from the registry, so it cannot drift: the
//! block between the markers must equal what the registry renders, in order.
//!
//! Regenerate after a tool change with `TANDEM_BLESS=1 cargo test -p tandem-core --test mcp_doc`.

use tandem_core::mcp;

const PAGE: &str = include_str!("../../../docs/mcp.md");
const BEGIN: &str = "<!-- tools:begin -->";
const END: &str = "<!-- tools:end -->";

fn render() -> String {
    mcp::tools()
        .iter()
        .map(|tool| {
            let required: Vec<&str> = tool.input_schema["required"]
                .as_array()
                .map(|r| r.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let params: Vec<String> = tool.input_schema["properties"]
                .as_object()
                .map(|p| {
                    p.keys()
                        .map(|k| {
                            if required.contains(&k.as_str()) {
                                format!("`{k}` (required)")
                            } else {
                                format!("`{k}`")
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let params = if params.is_empty() {
                "none".to_string()
            } else {
                params.join(", ")
            };
            format!("- `{}`: {} Params: {}.", tool.name, tool.description, params)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block() -> &'static str {
    let start = PAGE.find(BEGIN).expect("docs/mcp.md has the begin marker") + BEGIN.len();
    let end = PAGE.find(END).expect("docs/mcp.md has the end marker");
    PAGE[start..end].trim()
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
