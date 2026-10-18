# Spec: Bikeshed — an Obsidian-like notes bikeshed in Tandem

Type: spec
Status: implemented
Source: `.scratch/bikeshed/map.md` + tickets 01–05, ADRs 0004/0005, branch `research/bikeshed-editor-tech` (editor-technology research report), branch `prototype/bikeshed-ui` (UI prototype; verdict: search in sidebar, backlinks in bottom dock), `GLOSSARY.md` (Bikeshed / Note / Wikilink / Backlink / Attachment terms)

## Problem Statement

A human and several local AI agents share one Mac, and Tandem already gives them a shared todo list — but notes have no shared home. The human's notes live in an app the agents can't reach, and anything an agent writes down is a stray file the human never sees in context. There is no Obsidian-like editing surface in Tandem: no markdown editor with live preview, no web of `[[links]]` between notes, no search across the human's own writing — and nothing the agents can co-author into. The result is a shared workbench for tasks and a private, disconnected pile for everything else.

## Solution

Tandem gains a **Bikeshed** — a second tab in the Main window (Todos | Bikeshed) — an Obsidian-like markdown notes surface built into the app. All notes live in a plain folder of `.md` files at `~/.tandem/bikeshed/` (filename = note identity, nested folders, no metadata dir), right next to the todos store. Agents (same OS user, same Mac, no auth, no network) read and write the notes as ordinary files — no new MCP tools; the app watches the folder live, so an agent's note appears in the UI within ~0.2 s, and a human's note is on disk for the agent within ~1 s of autosave. The surface: a nested file tree sidebar with full-bikeshed search, a syntax-highlighted editor with inline live preview (CodeMirror 6 in a WKWebView — Obsidian's own stack), `[[wikilinks]]` with rename-safe link rewriting, and a backlinks dock. One workbench, two kinds of actor, two kinds of note — todos and markdown — no divergence.

## User Stories

1. As a Human, I want a second tab in the Main window — Todos | Bikeshed — so that notes and todos live in one app with one face.
2. As a Human, I want a left sidebar in the Bikeshed tab showing the nested file tree of the bikeshed (folders first, alphabetical, `.md` notes only), so that the bikeshed's structure is browsable at a glance.
3. As a Human, I want attachments (non-`.md` files) and hidden files to never appear in the tree, so that the tree is the note map, not a disk listing.
4. As a Human, I want a "new note" action that creates a note at the bikeshed root, so that capturing a new note is one click.
5. As a Human, I want to create notes inside folders and to rename or move a note (rename = change path), so that I can organise notes the way I think in them.
6. As a Human, I want note names containing `#` or `|` to be rejected when I create or rename, so that the wikilink grammar stays unambiguous.
7. As a Human, I want deleting a note or folder to move it to the macOS Trash, so that deletion is recoverable.
8. As a Human, I want a designed empty-bikeshed state with no seeded welcome note, so that a first launch is quiet and mine to fill.
9. As a Human, I want a designed no-note-selected state, so that an unselected editor reads as intentional, not broken.
10. As a Human, I want the tree to update live when an agent or external editor creates, edits, renames, or deletes notes, so that the bikeshed is shared in real time.
11. As a Human, I want a syntax-highlighted markdown editor with inline live preview — syntax markers hidden, formatting rendered in place as I type — so that I read and write formatted text without a separate preview pane.
12. As a Human, I want the editor to autosave ~1 s after my last keystroke, and on note switch, window blur, and ⌘S, with no persistent unsaved badge (at most a transient "saving…"), so that saving is invisible and the file on disk is always close to current.
13. As a Human, I want a failed save to keep my buffer, surface the error, and retry, so that a disk hiccup never loses my words.
14. As a Human, I want a clean (unmodified) note to adopt an external change silently, so that an agent's edits flow through without ceremony.
15. As a Human, I want a banner offering [Overwrite with mine] / [Take theirs] when a note I have open with unsaved edits changes on disk, and a pending autosave to be withheld until I choose, so that my work is never silently clobbered.
16. As a Human, I want task-list checkboxes to be interactive — clicking toggles `- [ ]` ↔ `- [x]` — so that checking off a task is a click, not an edit.
17. As a Human, I want math rendered via KaTeX (inline `$…$` and display `$$…$$`, Obsidian-style trigger rules so a currency `$` doesn't start math; parse errors leave the raw math visible), so that formulas read as formulas.
18. As a Human, I want local images embedded — `![](rel/path)` resolved relative to the containing note and confined to the bikeshed — with a broken-image placeholder and alt text when the file is missing, so that notes can carry figures.
19. As a Human, I want remote `http(s)` image targets to never be fetched (broken-image placeholder instead), so that the app stays no-network.
20. As a Human, I want fenced code blocks highlighted for a curated language set and rendered as plain monospace for unknown languages, so that code reads well without a highlighter for every language.
21. As a Human, I want mermaid blocks, callouts, footnotes, emoji shortcodes, raw HTML, embeds, and tags to render literally, so that v1's behaviour is predictable and the deferred set is a known list.
22. As a Human, I want a note's leading YAML frontmatter stripped from the preview and never parsed, validated, or edited by the app, so that agent-authored metadata stays intact.
23. As a Human, I want a soft ceiling with a visible warning around ~1 MB per note, so that a huge note doesn't promise smoothness the editor can't keep.
24. As a Human, I want `[[wikilinks]]` to resolve by case-insensitive name — a bare target needing exactly one basename match, a qualified target as a bikeshed-root-relative path — so that linking is short and deterministic.
25. As a Human, I want two notes sharing a basename to make a bare link a visible broken link rather than a silently chosen match, so that a link can never quietly point at the wrong note.
26. As a Human, I want `[[note|alias]]` to display the alias and `[[note#Heading]]` to display the fragment while clicking opens the note at the top (heading scroll deferred), so that links read well now and gain depth later.
27. As a Human, I want clicking a resolved link to open the target note and clicking a broken link to create an empty note at the link's path (bare → bikeshed root; qualified → that path, creating folders as needed) and open it, with no confirm dialog, so that link-first note-taking works.
28. As a Human, I want `[[…]]` inside fenced code blocks and inline code to stay literal text, so that code examples are never links.
29. As a Human, I want a rename or move performed in the app to rewrite every wikilink that resolves to the renamed note — in every note, including the note itself, in bare, qualified, and aliased form (alias display text preserved; target re-canonicalised to bare-when-unique or qualified-when-ambiguous) — so that a rename is one action and strands nothing.
30. As a Human, I want markdown links to be never rewritten by a rename, so that the app only rewrites the grammar it owns.
31. As a Human, I want wikilinks to be plain text in source mode — no click, no hover, no resolution — so that source mode is honest text.
32. As a Human, I want a rename onto an already-taken name to be refused with an error and nothing written, so that a rename can never overwrite a note.
33. As a Human, I want a crash mid-rename to leave no torn files and at most some broken links, with re-running the rename self-healing them, so that the worst case is recoverable in one action.
34. As a Human, I want a search field at the top of the sidebar that, while typing, replaces the tree with full-bikeshed results, so that finding a note is one keystroke sequence.
35. As a Human, I want search to be case-insensitive substring over title + body, one row per note (not per match), sorted by bikeshed path, so that results are predictable and never flooded with duplicates of the same note.
36. As a Human, I want title hits to lead with the note's first line and body hits to show a snippet around the first match, so that I know what matched without opening the note.
37. As a Human, I want clicking a search result to open the note and clear the query, so that search doubles as a quick open.
38. As a Human, I want a collapsible "Backlinks" dock at the bottom of the editor column with the count in its header, so that I can see who links to the open note without leaving it.
39. As a Human, I want each backlink row to show the linking note plus the line containing the wikilink that resolves to the open note, so that I can jump to the referencing context.
40. As a Human, I want the backlinks dock to read "Nothing links here yet." when empty, and broken links to contribute nothing to it, so that the dock reflects only live links.
41. As a Human, I want one shared window for both tabs — default 1060×680, user-resizable, and no resize when switching tabs — with the Todos face keeping its 540 pt centred column in the wider window, so that the app keeps one face and one window size.
42. As a Human, I want the menu bar item to stay todos-only, so that the quick glance is not diluted.
43. As a Human, I want the editor's palette to follow the app's Ligne theme in day and night, so that the webview pane never looks foreign to the window.
44. As an Agent, I want the bikeshed to be a plain folder of `.md` files at `~/.tandem/bikeshed/`, so that I can read and write notes with ordinary file tools — no new MCP tools to learn.
45. As an Agent, I want my note writes to appear in the app live (watcher latency ~0.2 s), so that the human sees what I write without a refresh.
46. As an Agent, I want the wikilinks I write to resolve exactly as the app resolves them (case-insensitive, exactly-one rule, bikeshed-root-relative qualified paths), so that my links behave as the human sees them.
47. As an Agent, I want the spec's guidance to tell me to keep note names free of `#` and `|`, so that the notes I create stay nameable by wikilink.
48. As an Agent, I want my renames and deletions to be unmediated — the app shows the resulting broken links or missing notes but does not rewrite or restore anything — so that I understand the asymmetry: app renames rewrite links and go to Trash; mine don't and are permanent.
49. As an Agent, I want to drop attachments (non-`.md` files) into the bikeshed and reference them by relative path from a note, so that notes can carry figures.
50. As an Agent, I want the app to tolerate the files I write — frontmatter, unusual-but-valid names — without corrupting or rewriting them, so that co-authoring is safe.
51. As a Human, I want concurrent app + agent activity to never produce torn files or lost data, so that the bikeshed is trustworthy no matter who writes.
52. As a Human, I want a missing bikeshed folder to be an empty bikeshed — created lazily, never an error state, no welcome note — so that first launch is quiet.
53. As a Human, I want the same GFM dialect everywhere — editor, search index, wikilink parsing — so that what I see highlighted is what search and links agree on.

## Implementation Decisions

### Package and modules

One local SwiftPM package, the existing three targets:

1. **Core module** (shared library) — gains the `Bikeshed` type, sibling of `Store`: a `Sendable`, directory-rooted value type (default bikeshed `~/.tandem/bikeshed`; an explicit-directory initialiser for tests) holding all bikeshed, wikilink, backlink, and search logic — including rename-with-link-rewriting and the locking/file mechanics. The GUI links it; the MCP server links it too but stays untouched in v1 (no note tools — the on-disk files are the agent surface).
2. **GUI app** (SwiftUI, macOS 26+) — gains: an observable model for the Bikeshed tab mirroring the existing model's shape; a recursive FSEvents watcher; the main-window second tab (tree sidebar + editor column + backlinks dock); and the WKWebView editor surface.
3. **MCP server** — unchanged in v1.

New dependency: `swift-markdown` (Apple/Swift-org, Apache-2.0, cmark-gfm-backed, thread-safe persistent AST) for the in-process bikeshed index — wikilink resolution, backlinks, and search. Its NOTICE stays with the app. The same GFM dialect the CM6 editor highlights, so source and index agree.

### Bikeshed core — operations (contract semantics)

The public surface is the note operations, returning the same result-with-optional-note shape the todo operations use; domain failures are typed errors with short, actionable text (the existing `TandemError` convention).

- **list()** — the tree: nested folders + `.md` notes, folders first, alphabetical within each level. Excludes hidden files, non-`.md` attachments, and the lockfile (which lives outside the bikeshed). A missing bikeshed folder is an empty tree — never an error.
- **read(path)** — the note's text plus its on-disk hash and mtime (the hash drives conflict detection and self-write dedup; the mtime is internal truth, not displayed).
- **create(path)** — a new empty note. Validates the name (no `/`, `#`, `|`; spaces allowed); creates parent folders; a target that already exists is an error, not an overwrite.
- **write(path, text)** — the autosave path: atomic write under the bikeshed lock; records what was written (path + hash) so the watcher can dedup self-writes. A failed write is an error; the buffer is the model's problem, not the core's.
- **rename(oldPath, newPath)** — one transaction under the lock: rewrite every referencing note first (per-file atomic), then rename the note file (a single `rename(2)`, valid across folders on the same volume). No journal — a mid-sequence crash yields no torn files and no data loss, at most broken links, and re-running the rename self-heals. New path already taken → refused with an error, nothing written.
- **delete(path)** — removes the file (or folder) from the bikeshed under the lock; rewrites nothing — links to a deleted note become broken. The GUI path performs the macOS Trash move (recoverable) ahead of this; the core's deletion is the permanent primitive the Trash move falls back to.
- **search(query)** — case-insensitive substring over title + body; one row per note (title hit → the note's first line; body hit → a snippet around the first match, sliced from the original-case text); sorted by bikeshed path; empty bikeshed → empty results.
- **backlinks(path)** — the bikeshed notes containing at least one wikilink that resolves to the given note, each with the line that mentions it; broken links contribute nothing; derived state, recomputed from the file set (no stored link index).
- **link resolution** — the single resolver the editor, backlinks, and rename-rewrite all use: a wikilink target → note path, or broken, per the wikilink model below.

### Bikeshed protocol — per ADR 0004

- **Identity & naming** — a note is any file ending `.md` (case-insensitive). The app creates only names free of `/`, `#`, and `|` (the last two are the wikilink grammar's separators); pre-existing violators on disk are still shown and edited. Hidden files are ignored everywhere. Non-`.md` files are **attachments**: invisible in the tree, reachable only by relative path from the referencing note.
- **Frontmatter** — tolerated, opaque. Only a *leading, properly terminated* `---` block counts; stripped from live preview, untouched in source. The app never parses, validates, or edits it; a rename's link rewrite never touches wikilinks inside it.
- **Metadata** — nothing displayed in v1; mtime/birthtime stay the on-disk truth used internally (watcher, conflict detection).
- **Autosave** — debounced ~1 s after the last keystroke, plus on note switch, window blur, and ⌘S. No persistent unsaved badge; at most a transient "saving…". A failed write keeps the buffer, surfaces the error, and retries.
- **External changes** — the buffer tracks the on-disk hash. Clean buffer → reload silently. Dirty buffer → never clobber: banner with [Overwrite with mine] / [Take theirs]; a pending autosave is withheld. The app recognises its own writes by the hash it just wrote.
- **Deletion** — asymmetric, stated outright: UI delete (note or folder) moves to the macOS Trash (recoverable); agent deletion is a plain `rm` (permanent, unmediated). No tombstones — a deleted note is simply a broken wikilink.
- **Locking** — app writes take an exclusive `flock` on `~/.tandem/bikeshed.lock` (outside the bikeshed, never renamed — the ADR 0001 sidecar pattern) held across the whole write, multi-file operations included; readers take no lock; bounded retry with the same ~2 s budget as the todos store, then a clear "bikeshed locked" error. The lock is one-sided by design: agents and external editors bypass it.
- **Atomic writes** — same-directory temp file + fsync + `rename`; orphaned temps older than 60 s cleaned after acquiring the lock (the ADR 0001 orphan rule, same age threshold).
- **First launch** — a missing folder is an empty bikeshed: created lazily (at launch or first write), never an error state, no welcome note.

### Wikilink model — per ticket 03 and ADR 0005

- **Resolution** — a wikilink's target names a *note*; resolution is computed live from the current bikeshed file set (derived state — no stored link index). A **bare target** matches the note's basename (filename minus extension), case-insensitive, and resolves iff **exactly one** note matches; zero or multiple matches → **broken link**. A **qualified target** is a bikeshed-root-relative path, matched case-insensitively against the note's bikeshed-root-relative path; same exactly-one rule. Notes only — attachments are referenced by relative path, never by wikilink. `[[…]]` inside fenced code blocks or inline code is not a link. Consequence: creating or deleting notes never rewrites any link — a broken link starts resolving the moment the note appears; only rename rewrites link text. On a case-sensitive volume, case-variant duplicate names make any link to that name ambiguous → broken (tolerated, not special-cased).
- **Grammar (v1)** — `[[target[#fragment][|alias]]]`: split at the first `#`, then the first `|`; scan left-to-right, first `]]` closes. Display text = alias if present, else fragment if present, else the target as written. The fragment is parsed but **inert in v1**: `[[note#Heading]]` displays "Heading", the click opens the note at the top. Not links (literal text): unclosed `[[` at end of line/file, empty or whitespace-only target, the same-note form `[[#Heading]]`, and embeds `![[…]]`. The app never creates names containing `#` or `|`, keeping the grammar unambiguous; a note whose on-disk name contains either (agent/external creation only) is unnameable by wikilink — the spec's agent guidance says to keep names free of both.
- **Broken links** — rendered visually distinct but unobtrusive (ember tone in the prototype); clickable. Click creates an empty note at the link's path and opens it — bare → bikeshed root; qualified → that path, creating folders as needed; no confirm dialog. A resolved link's click opens the target note.
- **Rename / move** — one operation: change the note's path (rename and folder-move are the same). The app rewrites every wikilink that resolves to the renamed note — in all bikeshed notes, including the note itself, in any form (bare, qualified, aliased — the alias's display text is preserved, only the target portion changes). Wikilinks only: markdown links are never rewritten (they stay the agent's responsibility; spec guidance: use wikilinks); embeds are literal text, untouched. Each rewritten target is written in the **canonical form for the new file set** — bare if the name is unique, bikeshed-root-qualified if not. Links that don't resolve to the renamed note — broken ones included — are never touched. **Only app-performed renames rewrite**: an agent renaming a file on disk leaves stale, visible broken links (Obsidian parity). Mechanics per ADR 0004: one transaction under the sidecar lock, no journal; each referencing note is rewritten first (per-file temp + fsync + rename), then the note file itself is renamed. Crash mid-sequence → `flock` releases, no torn writes, no data loss, at most broken links; re-running the rename self-heals. No auto-repair in v1; the retry is the repair.
- **Backlinks (data model)** — for the open note, its backlinks are the bikeshed notes containing at least one wikilink that resolves to it; broken links contribute nothing. What the panel lists and where it lives is the UI decision below.
- **Source mode** — plain text: no click, no hover, no resolution; markdown highlighting may color wikilink tokens (editor styling, not link UI). The model is view-independent — it holds for inline preview, the split-view fallback, and source mode alike. Any link-inserting UI writes the canonical form.

### Editor — per ticket 01's research verdict

- **Stack**: WKWebView + CodeMirror 6 — Obsidian's own stack. `basicSetup` + `@codemirror/lang-markdown` (GFM) + a live-preview decoration extension. The webview is the only editor surface; the app owns the file system. No web workers, so `loadFileURL` from the app bundle works.
- **Inline live preview mechanism**: an incremental `@lezer/markdown` parse plus `Decoration.replace({})` over marker ranges — markers removed from flow without changing the enclosing line's measured height (the Obsidian/Atomic-Editor mechanism). Two invariants, normative: (1) **raw markdown is the source of truth** — decorations are view-only; (2) line height depends on CSS classes, never on token visibility (no layout shift as markers appear/disappear). Block decorations (tables, display math, images) come from a **StateField** — CM6 forbids block decorations from a ViewPlugin's `decorations` facet.
- **Wikilinks in the editor**: a small custom language extension — linkify `[[…]]` per the grammar above, resolve against the in-process index, decoration for broken links; the same resolver the core uses.
- **Live-preview mode (settled by this spec)**: v1 ships **full inline live preview only** (Obsidian's default). The focus-mode compromise (only the active line/paragraph rendered, the rest dimmed source) and a mode toggle are v2 candidates — the decoration architecture supports both without rework.
- **Sanctioned fallback**: if the WKWebView integration proves infeasible in practice, the split view (CM6 source pane + rendered pane) is the fallback, and the rendered pane must cover the same normative feature list below. Fallback renderer: **MarkdownUI** (battle-tested, stable, in maintenance mode — stability over novelty for a path that may never ship; its successor Textual stays a v2 watch item).
- **Bridge (WKWebView ↔ Swift)**: JS→Swift via `WKScriptMessageHandler` (loaded, dirty-state + text, link-opened, checkbox-toggled); Swift→JS via `callAsyncJavaScript` (set document, insert text, reveal line). The JS side has **no file or network access** — everything crosses the bridge. Navigation is intercepted so link clicks reach Swift and the webview never leaves the bundled editor page. Autosave: JS emits dirty state; Swift debounces (~1 s), performs the atomic write, and records it for self-write dedup.
- **Images & attachments**: a `tandem:` WKURLSchemeHandler confined to the bikeshed root (refuses `..` escapes) serves local image/attachment references; a missing file → broken-image placeholder with alt text; images capped at container width. Remote `http(s)` targets are never fetched (the no-network invariant) and render as a broken-image placeholder.
- **Math**: KaTeX ships in the app's Resources with the CM6 bundle (JS + CSS + fonts); pure client-side, no workers.
- **Theme**: the CM6 theme follows the app's Ligne palette in day and night, so the webview pane matches the window chrome.
- **Scale**: a soft per-note ceiling with a visible warning around ~1 MB (where decoration recompute and IME composition deserve profiling). The bikeshed index is incremental off FSEvent batches, with the full rebuild at launch as a bounded background job; bikeshed-wide *rendered* output is never held in memory — index only.
- **Packaging**: the editor surface is a small static bundle (one HTML page, one prebuilt JS bundle, one CSS file) built with esbuild and shipped under the GUI target's resources. The dependency set is pinned in a checked-in `package.json` + lockfile — **npm metadata is the source of truth** for versions (the CodeMirror GitHub org's repos are archived mirrors; npm is the distribution channel). Pinned versions: `codemirror` 6.0.2, `@codemirror/state` 6.7.6, `@codemirror/view` 6.43.14, `@codemirror/language` 6.13.1, `@codemirror/lang-markdown` 6.5.2, `@lezer/markdown` 1.8.0, plus KaTeX. Bundle measured at ~597 KB min / 203 KB gzip (CM6 set) + ~557 KB raw / ~331 KB gzip (KaTeX). Licenses: CM6/Lezer MIT, swift-markdown Apache-2.0, cmark-gfm BSD-2, KaTeX MIT — license texts stay in the bundle; nothing GPL/AGPL.

### Rendered feature list (normative, per ticket 05)

- **Rendered**: the GFM floor (paragraphs, headings, ordered/unordered/nested lists, task lists, tables, blockquotes, strikethrough, inline code, fenced code blocks, autolinks, markdown links — frontmatter stripped); **interactive task checkboxes** (click toggles `- [ ]` ↔ `- [x]`, one-character transaction, buffer stays source of truth, autosave persists); **fenced-code highlighting** for a curated set (swift, python, javascript, typescript, json, shell, yaml, toml, sql, html, css — exact list nailed at implementation; unknown/missing language → plain monospace); **KaTeX math** (inline + display block form, Obsidian-style trigger rules, parse errors leave raw math visible); **local images** (bikeshed-confined, missing → broken-image placeholder with alt text); **wikilinks** (navigable internal links, never raw `[[…]]` in preview); **markdown links** (a target that resolves to a bikeshed file by case-insensitive path *existence* — a path, not a name, deliberately distinct from the wikilink exactly-one rule — is internal: a note opens in the editor, an attachment in the OS default app; `#fragment` inert in v1; everything else → OS default handler).
- **Not rendered (literal / plain form)**: emoji shortcodes, subscript, superscript, footnotes → literal text; raw HTML → styled source, never executed or laid out; fenced `mermaid` blocks → highlighted code block (diagram rendering a v2 candidate); callouts, highlights, comments → plain blockquote / literal text; embeds `![[…]]` → literal text; tags → plain text; code-block copy button → out (select + ⌘C suffices).
- **Cross-cutting invariants**: raw markdown is the source of truth; nothing special (wikilink, math, image) is processed inside code fences or inline code; the list is mode-independent — it holds for the split-view fallback too.
- **V2 candidates (explicitly deferred)**: mermaid, callouts, footnotes, highlights, comments, emoji shortcodes, sub/superscript, raw-HTML rendering, heading-target scroll, remote images, code-block copy button, focus-mode preview toggle.

### Search and backlinks — per ticket 02's verdict

- **Search** lives in the left sidebar: a field above the file tree; while typing, the tree is replaced by full-bikeshed results. Matching: case-insensitive substring over title + body, one row per note (not per match), sorted by bikeshed path; title hits lead with the note's first line, body hits with a snippet around the first match; clicking a result opens the note and clears the query.
- **Backlinks** live in a collapsible "Backlinks" dock at the bottom of the editor column: each row is the linking note plus the line containing the wikilink that resolves to the open note; the dock header carries the count; empty reads "Nothing links here yet."
- **⌘K palette (settled by this spec)**: out of scope for v1 — the sidebar search field is the sole search affordance. A ⌘K-style palette (quick-open / command palette) is a v2 candidate.

### Watcher — per ticket 01's research

One recursive `FSEventStream` on the bikeshed root: flags `FileEvents | NoDefer | UseCFTypes | WatchRoot`, `sinceWhen = kFSEventStreamEventIdSinceNow`, latency ≈ 0.2 s (built-in coalescing of the temp-write + rename pair), scheduled on a background dispatch queue. Behaviour contract: (1) coalesce each batch into a single "bikeshed changed" pass on the main actor — rebuild the tree; reload the open note only if its path is in the batch; (2) dedup self-writes via the recorded write hash; (3) external change to the open note with a dirty buffer → the conflict policy above; (4) full rescan on `RootChanged`, on stream start, and on app launch (cold start reads the tree directly). `StoreWatcher` stays as-is for the todos store (different lifecycle, different granularity). The watcher assumes the app stays non-sandboxed (it reads/writes `~/.tandem/` directly, as today); a security-scoped bookmark for a future sandbox is out of scope.

### UI shell — per ticket 02's verdict and the prototype

- The Main window gains a second tab — **Todos | Bikeshed** — in the window header; the menu bar item stays todos-only.
- **Bikeshed tab layout**: left sidebar (search field on top, nested tree below: folders first, alphabetical, `.md` notes only; "new note" action creates at the bikeshed root) + editor column (the WKWebView live-preview editor) + collapsible "Backlinks" dock at the bottom.
- **Designed states**: empty bikeshed (no welcome note, a designed placeholder — lazy creation per the bikeshed protocol) and no-note-selected (a designed placeholder).
- **Window sizing (settled by this spec)**: one window, one size for both tabs — switching tabs never resizes the window (window size is a window property, not a tab property; the macOS convention for tabbed windows). Default 1060×680 (the prototype's); user-resizable; the minimum width is whatever lets the Bikeshed face lay out (sidebar + editor side by side). The Todos face keeps its 540 pt column centred in the shared window — the wider window is negative space, not a layout change.
- **Chrome**: reach B (subtle Ligne Claire) is the face the bikeshed is designed against (the v2 prototype's default); the editor palette follows the Ligne theme day/night.

## Testing Decisions

The seams are the ones settled on the map (recorded 2026-10-14): **TandemCore is the only unit-tested surface; the GUI is verified manually.** All core tests run against a temp directory via `Bikeshed`'s explicit-directory initialiser — no fixture bikeshed, no real `~/.tandem`, no network. Prior art: the existing `OperationsTests` / `ConcurrencyTests` / `RecoveryTests` suites (temp dirs, real files, threads for lock contention).

Suites:

- **Note operations** — create / read / write / list / rename / delete: tree shape (folders first, alphabetical), hidden files and attachments excluded, missing folder = empty tree, name validation (create/rename reject `/`, `#`, `|`; pre-existing violators tolerated), existing target = error not overwrite, `write` stores exactly the bytes given (frontmatter pass-through is structural: the core never parses note text).
- **Wikilink resolution** — grammar parsing (bare, qualified, alias, fragment, first-`#`-then-first-`|` split order, unclosed, empty, same-note form, embeds, code-fence and inline-code exemption); case-insensitive; exactly-one rule (zero or multiple matches → broken); attachments never linkable; duplicate basenames → broken, never a silent match.
- **Backlinks** — derived from the file set (no stored index): rows = linking note + the line containing the wikilink; broken links contribute nothing; recomputed after note creation, deletion, and rename.
- **Rename + link rewrite (ADR 0005)** — rewrites every form (bare, qualified, aliased — alias display text preserved); includes the note's own self-links; re-canonicalises to the new file set (bare when unique, qualified when ambiguous); wikilinks only — markdown links, frontmatter, embeds, and non-resolving links byte-unchanged; target name already taken → refused, nothing written.
- **Crash recovery (ADR 0004)** — simulate a mid-sequence crash (writer killed between the link-rewrite pass and the file rename): no torn files, no data loss, at most broken links; re-running the rename self-heals.
- **Locking and atomicity** — two app writers on threads under `flock`: bounded retry, then a "bikeshed locked" error; an external lock-less writer interleaved — readers always observe complete files; orphaned temp files older than 60 s cleaned after acquiring the lock.
- **Search** — case-insensitive substring over title + body; one row per note; bikeshed-path order; title hit → first line; body hit → snippet; empty bikeshed → empty results.
- **Conflict detection** — `read`'s hash reflects on-disk state; the model compares buffer hash against disk hash (clean → adopt, dirty → withhold). The banner flow itself is GUI and manual.
- **Scale sentinel** — one generous, non-flaky bound: a full index rebuild over a synthetic bikeshed (hundreds of notes, up to ~100 MB total) completes in seconds, keeping the sub-second feel honest.

GUI and editor (manual; the `prototype/bikeshed-ui` branch is the visual reference): the editor loads the bundled page; typing → autosave within ~1 s → file on disk changes; an agent writes a file → tree and open note update within ~0.5 s with no self-write loop; broken-link click creates the note; checkbox toggle persists; math, images, and code highlighting render; the conflict banner (mine / theirs) flow; trash deletion is recoverable; day/night palette matches the window.

## Out of Scope

v1 does not include:

- **MCP note tools** — the agent surface is the on-disk files; the MCP target is untouched.
  > **Amended (2026-10-17, [ADR 0009](../../docs/adr/0009-cli-agent-face.md)):** the on-disk files stay the agent surface for every notes operation except `rename`; a `tandem-cli note rename` verb rewrites the wikilinks — the one operation the file tools cannot reproduce safely.
- Graph view, plugins, canvas, embeds.
- Daily notes, templates.
- Sync of any kind; mobile.
- Tags UI or tag rendering (tags render as plain text).
- Menu-bar-item notes (the item stays todos-only); note↔todo cross-references.
- Multiple bikesheds — one fixed folder.
- The deferred rendered set: mermaid rendering, callouts, footnotes, highlights, comments, emoji shortcodes, sub/superscript, raw-HTML rendering, heading-target scroll, remote images, code-block copy button, focus-mode preview toggle (full list under the rendered feature list).
- **⌘K palette** (v2 candidate — settled by this spec).
- Auto-repair of links broken by an agent rename — the app shows them; the retry of the rename (app-side) is the repair.
- Sandboxing / security-scoped bookmarks — the app stays non-sandboxed.

## Further Notes

- **Provenance**: `.scratch/bikeshed/map.md` + tickets 01–05 (each ticket's Answer is normative for its area); ADR 0004 (bikeshed protocol) and ADR 0005 (rename rewriting); branch `research/bikeshed-editor-tech` — `editor-technology.md` (editor verdict, pinned versions, bundle sizes) plus the fact files on main, `obsidian-wikilink-facts.md` and `rendered-markdown-facts.md`; branch `prototype/bikeshed-ui` — the surface prototype and its README (layout verdict; visual reference for the manual pass). `GLOSSARY.md` is the vocabulary authority (Bikeshed, Note, Wikilink, Backlink, Attachment).
- **Single implementation session target** — the same bar as the original Tandem spec: one agent, one session, this document as the only input.
- **Threads settled by this spec** (the map's two open items, plus the research report's open decisions): window sizing (one window, one size, no resize on tab switch, default 1060×680, Todos column centred at 540 pt); ⌘K palette (out of v1, v2 candidate); live-preview mode (full inline only in v1; focus mode + toggle are v2); fallback renderer (MarkdownUI, only if the sanctioned split-view fallback triggers); Trash-vs-permanent split (the core deletes; the GUI trashes first).
- **No-network invariant** — the `tandem:` scheme is confined to the bikeshed root, remote URLs are never fetched, and the editor bundle has no file or network access.
- **Versioning** — npm metadata is the source of truth for CodeMirror versions (the GitHub org is an archived mirror); the pinned `package.json` + lockfile is checked in.
- **Asymmetries, stated**: app renames rewrite wikilinks, agent renames don't; UI deletion goes to the Trash, agent deletion is permanent. The spec's agent-facing guidance is normative: use wikilinks for note links, keep names free of `#` and `|`, and expect unmediated rename/delete semantics.
- ADR 0001 (todos store) is untouched; the bikeshed reuses its sidecar-lock and orphan-temp patterns on a separate lock file.
