# 01 - Editor technology: markdown editing + live preview in SwiftUI

Type: research
Status: claimed

## Question

Which editing + rendering technology should the spec mandate for the bikeshed editor — a syntax-highlighted markdown editor with **inline live preview** (Obsidian-style: markdown renders in place as you type) — in SwiftUI on macOS 26+? Split view (source | rendered) is the sanctioned fallback if inline proves infeasible; the research must deliver a feasibility verdict on inline.

Evaluate at least:

1. **TextKit 2 / `NSTextView`** with custom markdown tokenization + highlighting (SwiftUI-native): how hard is true inline live preview — rendering `**bold**` in place while keeping caret, selection, and undo sane?
2. **WKWebView + CodeMirror 6** (what Obsidian itself uses) or Monaco: fidelity of inline preview, Swift↔JS bridging, packaging size, does it feel native enough for a macOS menu-bar-era app?
3. **Third-party Swift packages** — CodeEditSourceEditor, MarkdownUI, swift-markdown, and anything else credible: which cover editing vs rendering only, license, maintenance state as of now.
4. **Folder watching** — the app already watches a single file (`StoreWatcher` in `Sources/Tandem/`); what does watching a folder of files (FSEvents) add, and how should the spec describe it?
5. **Scale check** — hundreds of notes, individual notes up to a few hundred KB: where do the options break (startup indexing, per-keystroke re-highlight, memory)?

Deliverable: a recommendation (one primary path + the fallback), the inline-live-preview feasibility verdict, and links to the key sources. Capture the findings as a report on a throwaway `research/bikeshed-editor-tech` branch (report file under `.scratch/bikeshed/research/`), pushed but never merged, no PR.
