# Tandem

A macOS ticket board shared by humans and agents on the same machine.

## Language

### The app

**Tandem**:
The macOS ticket board, usable by both humans and agents on the same machine.
_Avoid_: "the app", "the todo app"

**Main window**:
Tandem's desktop face: the full ticket list and the board for browsing, adding, and editing, beside the bikeshed.
_Avoid_: panel, dashboard, view

**Board**:
The main window's ticket view: Status columns across, Cycle lanes down, a ticket drawn in the cell where its Status and lane meet. A view of the same tickets, not a fourth axis — the list and the board differ in layout, never in data.
_Avoid_: kanban board, swimlane view, board tab (when the view is meant)

**Menu bar item**:
Tandem's presence in the macOS menu bar (top bar): quick glance and quick action on tickets.
_Avoid_: top bar element, status bar, tray, badge

**Quick open**:
The main window's ⌘K palette: one ranked list over the bikeshed's notes and the store's tickets — a note opens in the editor, a ticket in its sheet. The match is the store's own comparison, so the palette and the list find the same things.
_Avoid_: command palette, launcher, fuzzy finder, spotlight

**Agent face**:
Tandem's skills, agent prompts and `AGENTS.md` as one set: shipped inside the app, installed at `<TandemHome>/agent/`, and read from there by every host. One copy, version-locked to the binary; `tandem-cli setup install` is its only writer.
_Avoid_: prompt pack, agent assets, skill bundle

**Host**:
A program that reads the agent face: OpenCode, Claude Code, or any agent that loads skills and prompts from a directory. Each host has its own paths, and `tandem-cli setup install --host` names the ones to wire.
_Avoid_: client, harness, runtime

### The domain

**Ticket**:
The only entity in the domain — the word the store, the CLI and the board use for it. "Todo" was the word until ADR 0030; it survives only as the CLI's deprecated alias, retired with the old wire keys.
_Avoid_: task, item, to-do (hyphenated), todo (the old word), card, issue, story

**Session**:
A record of one agent session — its brief, its outcome, and the tickets and notes it carried — a row of the store's `sessions` table with its own two states (`running`, `ended`); the CLI's five verbs are `session start/link/end/list/show`. Not a ticket, and not on the board. Its record is a fact, not a log: created once, stamped once, linked to.
_Avoid_: session log, journal

**Sprint**:
A record of one sprint — its lane, batch, run log and close — with its own two states (`open`, `closed`), stored in the store's `sprints` table and identified by the pair (lane, number). Its six verbs are `sprint open/ticket/run/close/list/show`. Not a ticket, and not on the board.
_Avoid_: sprint ticket

**Open**:
A ticket that has not yet been completed.
_Avoid_: pending, active

**Done**:
A ticket that has been completed.
_Avoid_: closed, finished, complete (as a noun)

**Status**:
A ticket's position in the workflow: backlog (the default), on-hold, ready, in-progress, in-review, or done. One axis with six values — done is where the workflow ends, not a flag set beside it; in-review is a finished piece of work whose PR is waiting for a reader, before it counts as done.
_Avoid_: state, stage, step

**Archived**:
A ticket set aside: out of the default views, still in the store. Any ticket can be archived, and archiving a done ticket is how a list is cleared without deleting. The app's archive view is where one is brought back; the board does not draw archived tickets at all.
_Avoid_: closed, deleted, cleared, hidden

**Blocked**:
A ticket waiting on another ticket: a `ticket_dependencies` edge from the dependent to its blocker, derived onto both faces (`blockedBy` on the dependent, `blocks` on the blocker), so the board can order, skip and draw it (ADR 0035). The state is derived, never stored: a block lifts the moment its blocker reaches `done`, with no write to the dependent. Why it waits is the body's `## Blocked on` section, read not copied; a wait that is not a ticket stays prose, with the ticket parked `on-hold`.
_Avoid_: blocked on (as a field), dependency column, gated (that is the gate's word), waiting (as a state)

**Cycle lane**:
The named area a ticket belongs to: the board's row and the list's grouping. Every ticket sits in one lane, or in the unnamed default area, which stays first.
_Avoid_: section, swimlane, epic, list (Trello's word)

**Origin**:
Who filed a ticket: `human`, or the name of the agent that filed it. Written once, at add, and never changed; a ticket that predates the field reads as `human`. A different question from the assignee, who is on it — a ticket an agent files *for* the human has both.
_Avoid_: author, filer, creator

**Filter**:
Which tickets are in view: `open` (the default), `done`, or `all`. One term for two surfaces: the main window's segmented control and the CLI's `list --filter` argument — and they part on the archive: the app's `All` shows it, the CLI's `all` does not. The app adds a fourth state only it needs — the archive itself, so an archived ticket can be found and brought back; the CLI has no fourth filter value, and reaches the archive with `--archived`: on its own that is the archive, and named with a filter it joins it.
_Avoid_: view, tab, scope, state

**Store**:
Tandem's single source of truth for every domain it keeps — tickets and lanes, comments, sessions, sprints, settings — one SQLite database at `<home>/tandem.db`, read and written by the app and by every `tandem-cli` invocation. All reads and writes go through it; no other copy is authoritative, and there is no second file to fall out of step. Its shape changes only by a **schema migration**; its move off the JSON files is recorded by an **import marker**; beside it sits its **rolling backup**, `tandem.db.bak` — the one previous complete generation, which a human restores and nothing reads (ADR 0018, ADR 0031).
_Avoid_: store file, JSON store, hand edits, backend, sync, cache (the JSON store and its sidecars are history: ADR 0001, and ADR 0031's amendments)

**Schema migration**:
One numbered, append-only step in the database's shape, applied in its own transaction and recorded in `schema_migrations` with the version and when it was applied — so a migration either applied whole or did not apply at all, and "which shape is this store?" is a row, not an inspection.
_Avoid_: schema bump, upgrade, patch, version (that is the migration's number, not the shape)

**Import marker**:
The `store_imports` row that records a domain as the database's: its `source` is the legacy JSON file's name when one was imported, and `source` is NULL when a fresh home's first write found nothing to import. It is the durable answer to "is this database the store yet", written in the same transaction as the imported rows and read inside the write transaction — not table emptiness, which cannot tell a killed first write from a home with no tickets. Present, the file is never read for a write again; absent, the file is still the store.
_Avoid_: import flag, seed, bootstrap, checkpoint

**Rolling backup**:
`tandem.db.bak`, the one complete generation of the database as it was before the last changed write — copied through SQLite's backup API, replaced each write, never rotated. It is a fallback, never a source: nothing reads it but a human recovering the store, and a copy we cannot take fails the write (ADR 0018).
_Avoid_: snapshot, dump, backup set, restore point

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
A small, create-only note an agent files with `tandem-cli note capture` for a decision, gotcha or fact whose re-derivation would cost real effort: one file in its project's **saddlebag**, out of the curated graph, for the consolidation pass to fold into the project's curated notes.
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

**Review lens**:
One checklist the reviewer applies to a pull request, named by the ticket's brief in its `## Lenses` section: `code`, `security`, `ui-design`, `ux`, or `architecture`, one table in `skills/review-lenses/SKILL.md`. A lens is not a role and not an agent, and the gate stays one: one merge authority, one verdict line. A brief with no `## Lenses` is read with `code` alone.
_Avoid_: reviewer type, reviewer persona, review mode, sub-reviewer

**Retro**:
The role that reads one session and files what it finds as backlog tickets — the tickets are the output, the report is a receipt. It runs at every sprint close, on the sprint session's own transcript, and is still invocable alone on a session the human names. It never fixes what it finds, because a retro that changes the work cannot report on it honestly.
_Avoid_: retrospective agent, postmortem, session analyst

**Refiner**:
The role that turns a backlog ticket into a brief a builder can take: it grills the decision tree, writes the spec, and splits an effort too big for one session into tickets that each fit one session. It owns no code and no design of its own — the ticket is the whole output.
_Avoid_: spec writer, planner, triage

**CLI**:
Tandem's agent-facing interface: a command-line binary, `tandem-cli`, shipped inside the app bundle and reached at its in-bundle path.
_Avoid_: API (too generic), backend, endpoint
