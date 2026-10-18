# Tandem

A native macOS todo app shared by humans and agents on the same machine.

## Language

### The app

**Tandem**:
The macOS todo app, usable by both humans and agents on the same machine.
_Avoid_: "the app", "the todo app" (when ambiguous)

**Main window**:
Tandem's desktop face: the full todo list for browsing, adding, and editing.
_Avoid_: panel, dashboard, view

**Menu bar item**:
Tandem's presence in the macOS menu bar (top bar): quick glance and quick action on todos.
_Avoid_: top bar element, status bar, tray, badge

### The domain

**Todo**:
A single item with a title and a done state. The only entity in the todo domain.
_Avoid_: task, item, to-do (hyphenated)

**Open**:
A todo that has not yet been completed.
_Avoid_: pending, active

**Done**:
A todo that has been completed.
_Avoid_: closed, finished, complete (as a noun)

**Filter**:
Which todos are in view: `open` (the default), `done`, or `all`. One term for two surfaces: the main window's segmented control and the CLI's `list --filter` argument.
_Avoid_: view, tab, scope, state

**Store**:
The single source of truth for todos, shared by the app and agents. All reads and writes go through the store; no other copy is authoritative.
_Avoid_: database, backend, sync, cache

### The bikeshed

**Bikeshed**:
Tandem's notes surface: a plain folder of notes and attachments, shared by the app and agents on the same machine and watched live by the app. The folder and the surface share one name — there is no second word for the folder.
_Avoid_: vault, notebook, notes folder

**Note**:
A single markdown file in the bikeshed. The filename is the note's identity; renaming a note is renaming the file.
_Avoid_: document, entry, page

**Memory note**:
A note created by a memory session recording the highlights of work — where the work left off, decisions with one-line rationale, learnings and gotchas, expensive-to-derive facts — written for the next agent session as reader.
_Avoid_: session log, journal, scratch note

**Capture**:
A raw memory note an agent files with `tandem-cli note capture` as it works: a small, create-only record under its project's hidden `.raw/` layer, out of the curated graph, for the consolidation pass to fold into the project's curated notes.
_Avoid_: note (ambiguous with a curated note), log entry, scratch note

**Consolidator**:
The agent that performs a consolidation pass: the sole writer of the curated notes (`Index.md` and the notes under `projects/**`), invoked as the `consolidator` agent or the `/consolidate` command. It folds the pending raw captures in and archives them — it never writes a raw note, and no other agent writes the curated layer.
_Avoid_: memory agent, curator, synthesizer, cleaner

**Human note**:
A note a person wrote, rather than an agent memory session. Indistinguishable from a memory note on disk (no author field), so its authority is judged by content: human prose is context, not state of record; only a memory note's _Where left off_/decisions carry current-state authority.
_Avoid_: user note, personal note

**Anchor note**:
A project's note: the orientation surface for that project, Where left off at the top. A project's memory starts as a single anchor; it becomes a folder when a second note is earned.
_Avoid_: project page, home note, project index

**Orientation note**:
The bikeshed's entry point: the note a new session opens first to find the project it needs. Exactly one, at the bikeshed root (`Index.md`); pointers and one-line status, never detail.
_Avoid_: dashboard, home page, table of contents

**Wikilink**:
An in-note reference to another note, written `[[target]]` with an optional `#fragment` and `|alias`, resolved live by case-insensitive name against the bikeshed; a wikilink that resolves to no note is a broken link.
_Avoid_: internal link, embed

**Backlink**:
A note that contains a wikilink resolving to a given note; the open note's backlinks are what the Backlinks panel lists. Derived state recomputed from the file set — broken links contribute nothing.
_Avoid_: inbound link, references, mentions

**Attachment**:
A non-markdown file (image, etc.) living in the bikeshed. Invisible in the file tree; reachable only by relative path from the note that references it.
_Avoid_: asset, media file, resource

### The users

**Human**:
A person using Tandem through the main window or menu bar item.
_Avoid_: user (ambiguous with the OS account), client, owner

**Agent**:
A local AI coding agent (e.g. OpenCode, Claude Code) running as the same OS user on the same Mac, interacting with Tandem through the CLI.
_Avoid_: bot, client, automation, user

**CLI**:
Tandem's agent-facing interface: a command-line binary, `tandem-cli`, shipped inside the app bundle and reached at its in-bundle path.
_Avoid_: API (too generic), backend, endpoint
