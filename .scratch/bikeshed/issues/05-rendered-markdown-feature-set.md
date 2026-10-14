# 05 - Rendered-markdown feature set: what the live preview renders

Type: grilling
Status: open

## Question

Pin down which markdown features the bikeshed's live preview renders in v1, so the spec has one normative list. The editor-technology verdict has settled the stack (WKWebView + CodeMirror 6 over an incremental Lezer markdown parse, with swift-markdown/cmark-gfm available in-process for wikilink/backlink/search indexing), so this is now a feature-boundary decision, not a feasibility question:

1. **Baseline**: confirm the GFM baseline as the floor — paragraphs, headings, lists, task lists, tables, blockquotes, strikethrough, inline code, fenced code blocks with language highlighting, autolinks (nearly free on CM6).
2. **Math**: render inline + display math (KaTeX) in the preview — in or out for v1?
3. **Mermaid**: render mermaid diagrams from fenced `mermaid` blocks — in or out for v1?
4. **Images**: embed local images referenced from a note (relative paths into the vault, e.g. `![](assets/diagram.png)`) — in or out for v1? (The only item that touches how non-`.md` files are treated — the vault-protocol ticket owns that decision; coordinate, don't re-decide.)
5. **Wikilinks in the render**: `[[wikilinks]]` render as navigable internal links per the wikilink-model ticket's resolution — confirm they are part of the rendered feature set, never shown as raw `[[text]]` in preview.

The answer is the spec's normative rendered-feature list.
