# Tandem

A native macOS todo app shared by humans and agents on the same machine.

## Language

### The app

**Tandem**:
The macOS todo app, usable by both humans and agents on the same machine.
_Avoid_: "the app", "the todo app" (when ambiguous)

**Main window**:
Tandem's desktop face: the full todo list and the board for browsing, adding, and editing, beside the bikeshed.
_Avoid_: panel, dashboard, view

**Board**:
The main window's ticket view: Status columns across, Cycle lanes down, a ticket drawn in the cell where its Status and lane meet. A view of the same todos, not a fourth axis — the list and the board differ in layout, never in data.
_Avoid_: kanban board, swimlane view, board tab (when the view is meant)

**Menu bar item**:
Tandem's presence in the macOS menu bar (top bar): quick glance and quick action on todos.
_Avoid_: top bar element, status bar, tray, badge

**Quick open**:
The main window's ⌘K palette: one ranked list over the bikeshed's notes and the store's tickets — a note opens in the editor, a ticket in its sheet. The match is the store's own comparison, so the palette and the list find the same things.
_Avoid_: command palette, launcher, fuzzy finder, spotlight

### The domain

**Todo**:
A single item in the shared list — the only entity in the todo domain, and the word the store and the CLI use for it.
_Avoid_: task, item, to-do (hyphenated)

**Ticket**:
A Todo carrying the full shape — a description, an assignee, a Status, and a Cycle lane. The board's unit of work; not a second entity, since the store's word for it stays Todo.
_Avoid_: card, issue, story

**Session ticket**:
A ticket one agent session files for itself, titled `Session — <project> · <session-id>`, so the board shows what is running and what each session did. It is a ticket, not a log entry: it carries a brief, the board refines it and moves it through the same gates as any other ticket, and a session that ends leaves behind work a successor can pick up. Its description is written once as a brief and appended to with the outcome, never overwritten.
_Avoid_: session log, journal, session record

**Open**:
A todo that has not yet been completed.
_Avoid_: pending, active

**Done**:
A todo that has been completed.
_Avoid_: closed, finished, complete (as a noun)

**Status**:
A todo's position in the workflow: backlog (the default), refined, ready, in-progress, in-review, or done. One axis with six values — done is where the workflow ends, not a flag set beside it; in-review is a finished piece of work whose PR is waiting for a reader, before it counts as done.
_Avoid_: state, stage, step

**Archived**:
A todo set aside: out of the default views, still in the store. Any todo can be archived, and archiving a done todo is how a list is cleared without deleting. The app's archive view is where one is brought back; the board does not draw archived tickets at all.
_Avoid_: closed, deleted, cleared, hidden

**Cycle lane**:
The named area a todo belongs to: the board's row and the list's grouping. Every todo sits in one lane, or in the unnamed default area, which stays first.
_Avoid_: section, swimlane, epic, list (Trello's word)

**Origin**:
Who filed a ticket: `human`, or the name of the agent that filed it. Written once, at add, and never changed; a ticket that predates the field reads as `human`. A different question from the assignee, who is on it — a ticket an agent files *for* the human has both.
_Avoid_: author, filer, creator

**Filter**:
Which todos are in view: `open` (the default), `done`, or `all` (archived tickets included). One term for two surfaces: the main window's segmented control and the CLI's `list --filter` argument. The app adds a fourth state only it needs — the archive itself, so an archived ticket can be found and brought back; the CLI has no fourth flag by decision, and reaches the archive with a query.
_Avoid_: view, tab, scope, state

**Store**:
The single source of truth for todos, shared by the app and agents. All reads and writes go through the store; no other copy is authoritative. Beside the file sits its **rolling backup**, `todos.json.bak` — the one previous good copy, written under the same lock as the write it precedes (ADR 0018). It is a fallback, never a source: nothing reads it but a human recovering the list.
_Avoid_: database, backend, sync, cache

### The bikeshed

**Bikeshed**:
Tandem's notes surface: a plain folder of notes and attachments, shared by the app and agents on the same machine and watched live by the app. The folder and the surface share one name — there is no second word for the folder.
_Avoid_: vault, notebook, notes folder

**Note**:
A single markdown file in the bikeshed. The filename is the note's identity; renaming a note is renaming the file.
_Avoid_: document, entry, page

**Move**:
Changing where a note or a folder sits in the bikeshed — the name stays, the path changes. A move made in the app rewrites the wikilinks that resolve to every note that moved with it, so the references travel with the files. Distinct from a rename, which changes the name itself.
_Avoid_: mv, relocate, drag (the gesture, not the operation)

**Memory note**:
A note created by a memory session recording the highlights of work — where the work left off, decisions with one-line rationale, learnings and gotchas, expensive-to-derive facts — written for the next agent session as reader.
_Avoid_: session log, journal, scratch note

**Saddlebag**:
A project's hidden layer of captures — the `projects/<project>/.raw/` folder: what an agent carries as it works, emptied at the consolidation stop. Holds many captures; a **Capture** is one note in it. Hidden from the tree by default; the app's settings can reveal it (ADR 0023) — revealed, it is still outside the curated graph.
_Avoid_: raw layer, queue, inbox, backlog

**Capture**:
A small, create-only note an agent files with `tandem-cli note capture` as it works: one file in its project's **saddlebag**, out of the curated graph, for the consolidation pass to fold into the project's curated notes.
_Avoid_: note (ambiguous with a curated note), raw note, log entry, scratch note

**Consolidator**:
The role that performs a consolidation pass: the agent that is the sole writer of the curated notes (`Index.md` and the notes under `projects/**`). It folds the pending captures in and archives them — it never writes a capture, and no other agent writes the curated layer.
_Avoid_: memory agent, curator, synthesizer, cleaner

**Human note**:
A note a person wrote, rather than an agent memory session. Indistinguishable from a memory note on disk (no author field), so its authority is judged by content: human prose is context, not state of record; only a memory note's _Where left off_/decisions carry current-state authority.
_Avoid_: user note, personal note

**Anchor note**:
A project's note: the orientation surface for that project, Where left off at the top. A project's memory starts as a single anchor; it becomes a folder when a second note is earned.
_Avoid_: project page, home note, project index

**Hub**:
The bikeshed's entry point: the note a new session opens first to find the project it needs. Exactly one, at the bikeshed root (`Index.md`); pointers and one-line status, never detail. Its wikilinks are the **spokes** that reach the project notes.
_Avoid_: orientation note, dashboard, home page, table of contents

**Wikilink**:
An in-note reference to another note, written `[[target]]` with an optional `#fragment` and `|alias`, resolved live by case-insensitive name against the bikeshed; a wikilink that resolves to no note is a broken link.
_Avoid_: internal link, embed

**Backlink**:
A note that contains a wikilink resolving to a given note; the open note's backlinks are what the Backlinks panel lists. Derived state recomputed from the file set — broken links contribute nothing.
_Avoid_: inbound link, references, mentions

**Spokes**:
The wikilinks a note sends out, seen as its connections to the rest of the bikeshed; the Hub's spokes are how the entry point reaches the project notes. Every note is a small hub with its own spokes.
_Avoid_: links (too broad), connections

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

**Reviewer**:
The role that gates a pull request: it reads the ticket, the diff, the checks and the repo's rules, then either merges a two-way door with low blast radius or hands the change to the human. Dispatched for one PR, it ends with a verdict.
_Avoid_: approver, critic, merge bot

**Retro**:
The role that reads one session and files what it finds as backlog tickets — the tickets are the output, the report is a receipt. It never fixes what it finds, because a retro that changes the work cannot report on it honestly.
_Avoid_: retrospective agent, postmortem, session analyst

**Refiner**:
The role that turns a backlog ticket into a brief a builder can take: it grills the decision tree, writes the spec, and splits an effort too big for one session into one-session tickets. It owns no code and no design of its own — the ticket is the whole output.
_Avoid_: spec writer, planner, triage

**CLI**:
Tandem's agent-facing interface: a command-line binary, `tandem-cli`, shipped inside the app bundle and reached at its in-bundle path.
_Avoid_: API (too generic), backend, endpoint
