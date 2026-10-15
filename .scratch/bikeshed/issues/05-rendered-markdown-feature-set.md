# 05 - Rendered-markdown feature set: what the live preview renders

Type: grilling
Status: resolved (2026-10-15)

## Question

Pin down which markdown features the bikeshed's live preview renders in v1, so the spec has one normative list. The editor-technology verdict has settled the stack (WKWebView + CodeMirror 6 over an incremental Lezer markdown parse, with swift-markdown/cmark-gfm available in-process for wikilink/backlink/search indexing), so this is now a feature-boundary decision, not a feasibility question:

1. **Baseline**: confirm the GFM baseline as the floor — paragraphs, headings, lists, task lists, tables, blockquotes, strikethrough, inline code, fenced code blocks with language highlighting, autolinks (nearly free on CM6).
2. **Math**: render inline + display math (KaTeX) in the preview — in or out for v1?
3. **Mermaid**: render mermaid diagrams from fenced `mermaid` blocks — in or out for v1?
4. **Images**: embed local images referenced from a note (relative paths into the vault, e.g. `![](assets/diagram.png)`) — in or out for v1? (The only item that touches how non-`.md` files are treated — the vault-protocol ticket owns that decision; coordinate, don't re-decide.)
5. **Wikilinks in the render**: `[[wikilinks]]` render as navigable internal links per the wikilink-model ticket's resolution — confirm they are part of the rendered feature set, never shown as raw `[[text]]` in preview.

The answer is the spec's normative rendered-feature list.

## Answer

Settled by grilling (Q1–Q10, all accepted). The normative rendered-feature list for v1 live preview:

**Rendered**

1. **GFM floor** — paragraphs, headings, ordered/unordered/nested lists, task lists, tables, blockquotes, strikethrough, inline code, fenced code blocks, autolinks, markdown links. Same dialect on all three surfaces (Lezer-GFM editor, cmark-gfm index, search), so they can't disagree. Frontmatter stripped from preview (ticket 04).
2. **Task-list checkboxes are interactive** — click toggles `- [ ]` ↔ `- [x]` (one-character transaction; buffer stays source of truth; autosave persists).
3. **Fenced-code highlighting** — curated core set (swift, python, javascript, typescript, json, shell, yaml, toml, sql, html, css; exact list nailed at spec time). Unknown or missing language → plain monospace block.
4. **Math via KaTeX** — inline `$…$` and display `$$…$$` (display is block form only, own line). Trigger rules follow Obsidian: a `$` that would false-positive in prose (currency) doesn't start math. Parse error → raw math text stays visible (no blank, no crash). KaTeX ships in Resources with the CM6 bundle (JS + CSS + woff2 fonts, ~557 KB raw / ~331 KB gzip, fonts co-located with the CSS); pure client-side, no workers.
5. **Local images** — `![](rel/path)` (and `[text](rel/path)` to attachments) resolves relative to the **containing note**, served by a `tandem:` WKURLSchemeHandler confined to the vault root (refuses `..` escapes). Missing file → broken-image placeholder with alt text. Images capped at container width. Remote `http(s)` targets are never fetched (no-network bridging invariant) and render as a broken-image placeholder.
6. **Wikilinks** — per ticket 03's normative model: rendered as navigable internal links, never shown as raw `[[…]]` in preview.
7. **Markdown links** — a target that resolves to a vault file (case-insensitive path *existence* — a path, not a name, so deliberately distinct from the wikilink exactly-one rule) is internal: a note opens in the editor, an attachment in the OS default app; `#fragment` is inert in v1 (opens at top, consistent with ticket 03). Everything else (URL, mailto, unresolvable) → OS default handler.

**Not rendered (literal / plain form)**

- Emoji shortcodes, subscript, superscript, footnotes → literal text.
- Raw HTML → styled source; never executed or laid out.
- Fenced `mermaid` blocks → highlighted code block (mermaid a known language). Diagram rendering is a v2 candidate — Obsidian now ships mermaid *off by default* (since 1.13), and per-keystroke re-render is a documented bad pattern.
- Callouts (`> [!note]`), highlights (`==text==`), comments (`%%…%%`) → plain blockquote / literal text. Obsidian-proprietary, not the GFM floor; v2 candidates, callouts first.
- Embeds `![[…]]` → literal text (ticket 03).
- Tags (`#tag`) → plain text (map scope cut: no tags UI).
- Code-block copy button → out (select + ⌘C suffices; v2 candidate).

**Cross-cutting invariants (for the spec)**

- Raw markdown is the source of truth; decorations are view-only; line height independent of token visibility (ticket 01 invariants).
- Nothing special (wikilink, math, image) is processed inside code fences or inline code.
- Block decorations (tables, display math, images) must come from a **StateField** — CM6 forbids block decorations from a ViewPlugin's `decorations` facet (verified prior art: Atomic Editor).
- The list is **mode-independent**: it holds for the split-view fallback too — if the fallback is taken, its renderer must cover the same set.

**V2 candidates (explicitly deferred)**: mermaid, callouts, footnotes, highlights, comments, emoji shortcodes, sub/superscript, raw-HTML rendering, heading-target scroll (ticket 03), remote images, code-block copy button.

Assets: [Rendered-markdown facts — Obsidian defaults, KaTeX, mermaid, WKWebView file access, CM6 prior art; researched 2026-10-15](../research/rendered-markdown-facts.md)
