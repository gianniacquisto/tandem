# 01 - Editor technology: markdown editing + live preview in SwiftUI

Type: research
Status: resolved (2026-10-14)

## Question

Which editing + rendering technology should the spec mandate for the bikeshed editor — a syntax-highlighted markdown editor with **inline live preview** (Obsidian-style: markdown renders in place as you type) — in SwiftUI on macOS 26+? Split view (source | rendered) is the sanctioned fallback if inline proves infeasible; the research must deliver a feasibility verdict on inline.

Evaluate at least:

1. **TextKit 2 / `NSTextView`** with custom markdown tokenization + highlighting (SwiftUI-native): how hard is true inline live preview — rendering `**bold**` in place while keeping caret, selection, and undo sane?
2. **WKWebView + CodeMirror 6** (what Obsidian itself uses) or Monaco: fidelity of inline preview, Swift↔JS bridging, packaging size, does it feel native enough for a macOS menu-bar-era app?
3. **Third-party Swift packages** — CodeEditSourceEditor, MarkdownUI, swift-markdown, and anything else credible: which cover editing vs rendering only, license, maintenance state as of now.
4. **Folder watching** — the app already watches a single file (`StoreWatcher` in `Sources/Tandem/`); what does watching a folder of files (FSEvents) add, and how should the spec describe it?
5. **Scale check** — hundreds of notes, individual notes up to a few hundred KB: where do the options break (startup indexing, per-keystroke re-highlight, memory)?

Deliverable: a recommendation (one primary path + the fallback), the inline-live-preview feasibility verdict, and links to the key sources. Capture the findings as a report on a throwaway `research/bikeshed-editor-tech` branch (report file under `.scratch/bikeshed/research/`), pushed but never merged, no PR.

## Answer

**Inline live preview: FEASIBLE — primary path WKWebView + CodeMirror 6, fallback the sanctioned split view.** Obsidian — the reference product for this exact interaction — runs it on CM6 (its developer docs: "Obsidian uses CodeMirror 6 (CM6) to power the Markdown editor"), and the mechanism is small and well-documented: an incremental Lezer markdown parse plus a `Decoration.replace({})` extension that removes syntax markers from flow (open-source reference implementation Atomic Editor documents both the mechanism and the two invariants to copy: raw markdown is the source of truth and decorations are view-only, and line height must not depend on token visibility). Native TextKit 2 is rejected as primary: styled-source highlighting is easy, but there is no public hook for hiding markers — the `NSLayoutManager` glyph-stripping trick is TextKit 1-only and Apple deliberately removed glyph APIs from TextKit 2 ("there are zero glyph APIs in TextKit 2", WWDC22 10090); the two escape hatches (TextKit 1 compat mode, display-string mapping) are research bets, not spec material. Monaco is rejected: it's a code editor (no markdown live preview) and its own README FAQ says `file://` pages can't create web workers, so bundle embedding would require a local HTTP server. No Swift package does editing-with-inline-preview: CodeEditSourceEditor is highlight-only and self-declares "not ready for production use"; MarkdownUI is a renderer in maintenance mode (successor Textual is v0.1.x); swift-markdown/cmark-gfm are parsers — keep the former for in-process wikilink/backlink/search indexing (Apache-2.0, thread-safe, same GFM dialect CM6 highlights). Scale is a non-issue at target size (500 notes × 300 KB worst case = 150 MB): CM6's buffer + incremental parse are O(dirty) and a full vault bulk-index via cmark-gfm is sub-second; pressure points plus a ~1 MB soft per-note ceiling are spelled out in §5 of the report. Folder watching: the spec should mandate one recursive `FSEventStream` over the vault root (`FileEvents | NoDefer | UseCFTypes | WatchRoot`, `sinceWhen = kFSEventStreamEventIdSinceNow`, latency ≈ 0.2 s) with self-write dedup and rescan on `RootChanged`; `StoreWatcher` stays as-is for the todos store. Bridging, packaging, and license notes and the open spec decisions are in the report.

Full findings: branch `research/bikeshed-editor-tech`, file `.scratch/bikeshed/research/editor-technology.md` (pushed to origin; throwaway branch, no PR).
