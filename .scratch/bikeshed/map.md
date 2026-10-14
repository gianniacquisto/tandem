# Map: Bikeshed — an Obsidian-like notes vault in Tandem

Type: wayfinder:map

## Destination

A sharp, implementable spec for the bikeshed: an Obsidian-like markdown notes surface built into Tandem. The bikeshed is a plain folder of `.md` files at `~/.tandem/bikeshed/` (filename = note identity, no metadata dir), agents reach it as on-disk files (no new MCP tools), and the app watches it live. v1 UI: a Bikeshed tab in the main window with a nested file tree, a syntax-highlighted editor, inline live preview (split as fallback), `[[wikilinks]]`, full-text search, and backlinks. The map only settles the decisions the spec depends on; the spec itself is written at handoff.

## Notes

- Domain: Swift/SwiftUI macOS app (macOS 26+). The bikeshed is a plain folder of `.md` files next to `~/.tandem/todos.json`; agents reach it as on-disk files (same OS user, same Mac) — no new MCP tools in v1. The app's file watcher surfaces changes live.
- Tracker: local markdown — map at `.scratch/bikeshed/map.md`, tickets at `.scratch/bikeshed/issues/NN-<slug>.md`. Claim = set `Status: claimed`. Blocking = `Blocked by:` line in the ticket header.
- Spec (the destination): to be written at handoff, e.g. `.scratch/bikeshed/spec.md`.
- Skills to consult: `grilling` + `domain-modeling` for grilling/prototype tickets; `research` for research tickets. `GLOSSARY.md` at repo root is the vocabulary authority; add entries for **Bikeshed**, **Note**, and **Wikilink** as they resolve on this map.
- Settled during charting (conversationally, not as tickets):
  - Plan, don't do: the map produces decisions, not the feature. The spec is the destination, written at handoff.
  - Vault: `~/.tandem/bikeshed/`, plain `.md` files, filename = note identity (renaming a note = renaming the file), no hidden metadata dir, nested folders allowed.
  - Agent surface: on-disk files only; no MCP note tools in v1.
  - Scope cut — in: file tree, syntax-highlighted editor, live preview, wikilinks, full-text search, backlinks. Out: graph view, plugins, daily notes, templates, canvas, sync/publish, tags UI, mobile.
  - UI: second tab in the main window (Todos | Bikeshed); the menu bar item stays todos-only.
  - Preview mode: inline live preview is the goal (Obsidian-style render in place); split view is the fallback if research shows inline is infeasible.
  - Domains stay separate: no note↔todo cross-references in v1.
  - Test seams (settled 2026-10-14, at the to-spec check): one new seam — a `Bikeshed` type in `TandemCore` (sibling of `Store`: `Sendable`, directory-rooted, `init(directory:)` for temp-dir tests) holding all vault/wikilink/search logic incl. rename-with-link-rewriting and the sidecar-lock + atomic-write mechanics; app glue only (an observable model mirroring `TandemModel` + a recursive FSEvents watcher — the existing `StoreWatcher` covers one directory level, insufficient for nested folders); the MCP target stays untouched in v1. Core seam is the only unit-tested surface (real files in temp dirs, threads for contention; prior art: `OperationsTests` / `ConcurrencyTests` / `RecoveryTests` in `Tests/TandemCoreTests/`). GUI tested manually.

## Decisions so far

<!-- one line per closed ticket: name (wrapping its link) + one-line gist -->
- [Editor technology: markdown editing + live preview in SwiftUI](issues/01-editor-technology.md): Inline live preview is feasible — primary path WKWebView + CodeMirror 6 (Obsidian's stack, ~600 KB JS bundle, hidden-marker decorations over an incremental Lezer parse); fallback the sanctioned split view (MarkdownUI/Textual render pane); native-only TextKit 2 has no public hook to hide syntax markers.

## Not yet specified

- **First launch / empty state** — the bikeshed folder is missing or empty on first launch: does the app create it, seed a welcome note, or show an empty state? Sharpens once the vault protocol lands.
- **Search and backlinks behavior** — matching semantics and where results/panels live. Intended to be settled inside the UI prototype ticket's human-in-the-loop exchange; graduate to its own ticket if that exchange leaves them loose.

## Out of scope

- MCP note tools (`list_notes` / `read_note` / `write_note`) — on-disk files are the agent surface; a separate effort if wanted later.
- Graph view, plugins, daily notes, templates, canvas, sync/publish, tags UI, mobile — beyond the v1 scope cut.
- Menu bar item note quick-actions — v1 surface is the main window.
- Note↔todo cross-references — the domains stay separate in v1.
- Multiple bikesheds / vault picker — v1 is a single fixed vault at `~/.tandem/bikeshed/`.
