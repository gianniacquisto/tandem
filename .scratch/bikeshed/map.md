# Map: Bikeshed — an Obsidian-like notes vault in Tandem

Type: wayfinder:map

## Destination

A sharp, implementable spec for the bikeshed: an Obsidian-like markdown notes surface built into Tandem. The bikeshed is a plain folder of `.md` files at `~/.tandem/bikeshed/` (filename = note identity, no metadata dir), agents reach it as on-disk files (no new MCP tools), and the app watches it live. v1 UI: a Bikeshed tab in the main window with a nested file tree, a syntax-highlighted editor, inline live preview (split as fallback), `[[wikilinks]]`, full-text search, and backlinks. The map only settles the decisions the spec depends on; the spec itself is written at handoff.

## Notes

> **Amended (2026-10-17, [ADR 0009](../../docs/adr/0009-cli-agent-face.md)):** the MCP target retired; the agent face is now `tandem-cli`, in-bundle. The substance below stands — notes are reached as on-disk files, with no note tools beyond the single `tandem-cli note rename` verb (renaming through the CLI is what keeps wikilinks intact).

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

- [Vault protocol: file rules, frontmatter, conflicts, deletion](issues/04-vault-protocol.md): notes are `.md` files (`#`/`|` never created, tolerated on disk), frontmatter opaque, ~1 s autosave, dirty buffers never clobber external edits (banner: mine/theirs), UI delete → Trash, app writes hold a one-sided `flock` on `bikeshed.lock`, journal-free rename (crash → broken links only), missing folder → lazy creation — ADR 0004.
- [Editor technology: markdown editing + live preview in SwiftUI](issues/01-editor-technology.md): Inline live preview is feasible — primary path WKWebView + CodeMirror 6 (Obsidian's stack, ~600 KB JS bundle, hidden-marker decorations over an incremental Lezer parse); fallback the sanctioned split view (MarkdownUI/Textual render pane); native-only TextKit 2 has no public hook to hide syntax markers.
- [Rendered-markdown feature set: what the live preview renders](issues/05-rendered-markdown-feature-set.md): GFM floor + interactive task checkboxes, curated code highlighting, KaTeX math, local images, wikilinks and markdown links to vault files; everything else (mermaid, callouts, footnotes, emoji, raw HTML, embeds, tags) renders literally, with the deferred set listed as v2 candidates.
- [Bikeshed surface: main-window tab, tree, editor, search, backlinks](issues/02-bikeshed-ui-prototype.md): the main window's second tab holds a left sidebar (nested tree; the search field lives there, results replace the tree while typing — case-insensitive substring over title + body, one row per note, vault-path order) and an editor column with the inline live-preview mock and a collapsible "Backlinks" dock at the bottom (linking note + the line that mentions it); empty-vault and no-note-selected are designed states; window sizing across tabs and a ⌘K entry point stay open for the spec — prototype on branch `prototype/bikeshed-ui`.
- [Wikilink model: resolution, broken links, rename](issues/03-wikilink-model.md): wikilinks resolve live from the file set by case-insensitive name — bare target needs a unique basename match, qualified target is vault-root-relative, duplicates → visible broken link (no tie-break); grammar `[[target[#fragment][|alias]]]` with inert fragments, no embeds, code blocks exempt; a broken link's click creates the note at its path; an app-performed rename rewrites every resolving wikilink (canonical form: bare if unique) — wikilinks only, frontmatter untouched, agent renames rewrite nothing, crash leaves broken links that a retry self-heals (mechanics per [ADR 0004](../../docs/adr/0004-vault-protocol.md)) — see [ADR 0005](../../docs/adr/0005-wikilink-rename-rewriting.md).

## Not yet specified

— none: every map decision is resolved (2026-10-15). Open items the spec should settle are noted in the tickets (window sizing per tab, ⌘K search entry point).

## Out of scope

- MCP note tools (`list_notes` / `read_note` / `write_note`) — on-disk files are the agent surface; a separate effort if wanted later.
- Graph view, plugins, daily notes, templates, canvas, sync/publish, tags UI, mobile — beyond the v1 scope cut.
- Menu bar item note quick-actions — v1 surface is the main window.
- Note↔todo cross-references — the domains stay separate in v1.
- Multiple bikesheds / vault picker — v1 is a single fixed vault at `~/.tandem/bikeshed/`.
