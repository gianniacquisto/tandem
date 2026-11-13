# Tandem

A local ticket board, served by one binary, shared by humans and agents on the same machine.

## Language

### The app

**Tandem**:
The macOS ticket board, usable by both humans and agents on the same machine.
_Avoid_: "the app", "the todo app"

**Serve**:
Tandem's browser face: `tandem serve` binds a loopback address and draws the board and the bikeshed as HTML. `tandem mcp` is the agent face over stdio. Both doors are the same binary, so the human's board and the agent's tools cannot drift.
_Avoid_: main window, desktop app, renderer, shell

**Installer**:
`install.sh`, the one-line script a release publishes beside its archives and `SHA256SUMS`. It picks this machine's archive, verifies its SHA-256 against the manifest before writing anything, and installs `tandem` and its `tandem-cli` link.
_Avoid_: package manager, bundle installer

**Archive**:
One platform's release artifact: a `tandem-<version>-darwin-arm64.tar.gz`, `tandem-<version>-linux-x86_64.tar.gz` or `tandem-<version>-windows-x86_64.zip`, each carrying the executable. A release publishes all three, `install.sh` and one `SHA256SUMS`.
_Avoid_: app bundle, package, build

**Board**:
The served ticket view: Status columns across, Cycle lanes down, a ticket drawn in the cell where its Status and lane meet. A view of the same tickets, not a fourth axis.
_Avoid_: kanban board, swimlane view, board tab (when the view is meant)

**Example face**:
The skills and pass prompts under `examples/`, kept for this repository's own workflow. They are read from a checkout or copied by hand, not installed into a Tandem home; the cut retired the bundled face and the install verb ([ADR 0045](docs/adr/0045-the-board-and-notes-cut.md)).
_Avoid_: prompt pack, agent assets, skill bundle

**Host**:
A program that reads skills and prompts from a directory: OpenCode, Claude Code, or any agent that loads them. A host points at `examples/` or at a copy of it.
_Avoid_: client, harness, runtime

### The domain

**Ticket**:
The only entity in the domain — the word the store, the CLI and the board use for it. "Todo" was the word until ADR 0030; it survives only as the CLI's deprecated alias, retired with the old wire keys.
_Avoid_: task, item, to-do (hyphenated), todo (the old word), card, issue, story

**Open**:
A ticket that has not yet been completed.
_Avoid_: pending, active

**Done**:
A ticket sitting in the active workflow's finished stage. The default `agile` workflow names that stage `done`; a custom workflow names its own, and `ticket complete` moves the ticket there.
_Avoid_: closed, finished (as a second flag), complete (as a noun)

**Status**:
A ticket's position in the workflow: the key of one of the six fixed stages. They are `backlog` (intake), `on-hold` (parked), `ready`, `in-progress`, `in-review` and `done` (finished); a key from an older build's workflow data reads as `backlog`.
_Avoid_: state, step, column

**Stage**:
One of the six fixed steps a ticket moves through: `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` or `done`. A ticket's Status is a stage's key; the board's columns and the sheet's status menu draw the same six.
_Avoid_: status (when the entry is meant), column (that is the board's drawing of a stage)

**Archived**:
A ticket set aside: out of the default views, still in the store. Any ticket can be archived, and archiving a done ticket is how a list is cleared without deleting. The app's archive view is where one is brought back; the board does not draw archived tickets at all.
_Avoid_: closed, deleted, cleared, hidden

**Blocked**:
A ticket waiting on another ticket: a `ticket_dependencies` edge from the dependent to its blocker, derived onto both faces (`blockedBy` on the dependent, `blocks` on the blocker), so the board can order, skip and draw it (ADR 0035). The state is derived, never stored: a block lifts the moment its blocker reaches `done`, with no write to the dependent. Why it waits is the body's `## Blocked on` section, read not copied; a wait that is not a ticket stays prose, with the ticket parked `on-hold`.
_Avoid_: blocked on (as a field), dependency column, gated (that is the gate's word), waiting (as a state)

**Project**:
A lane's parent: the store's `projects` table, one row per project with its render order. A project holds several lanes, and every lane belongs to exactly one project; a ticket's project is derived via its lane, never stored. The default area has no project: cross-project capture, first on the board (ADR 0039).
_Avoid_: epic, portfolio, workspace

**Cycle lane**:
The named area a ticket belongs to: the board's row and the list's grouping. Every ticket sits in one lane, or in the unnamed default area, which stays first. Every lane belongs to exactly one parent project (ADR 0039).
_Avoid_: section, swimlane, epic, list (Trello's word)

**Origin**:
Who filed a ticket: `human`, or the name of the agent that filed it. Written once, at add, and never changed; a ticket that predates the field reads as `human`. A different question from the assignee, who is on it — a ticket an agent files *for* the human has both.
_Avoid_: author, filer, creator

**Filter**:
Which tickets are in view: `open` (the default), `done`, or `all`. One term for two surfaces: the main window's segmented control and the CLI's `list --filter` argument — and they part on the archive: the app's `All` shows it, the CLI's `all` does not. The app adds a fourth state only it needs — the archive itself, so an archived ticket can be found and brought back; the CLI has no fourth filter value, and reaches the archive with `--archived`: on its own that is the archive, and named with a filter it joins it.
_Avoid_: view, tab, scope, state

**Store**:
Tandem's single source of truth for the work it keeps: tickets, their dependency edges, comments, lanes, projects and the store's own metadata, one SQLite database at `<home>/tandem.db`, read and written by `tandem serve`, `tandem mcp` and every `tandem-cli` invocation. All reads and writes go through it; no other copy is authoritative. Its shape changes only by a **schema migration**; its move off the JSON files is recorded by an **import marker**; beside it sits its **rolling backup**, `tandem.db.bak` — the one previous complete generation, which a human restores and nothing reads (ADR 0018, ADR 0031).
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

**Wikilink**:
An in-note reference to another note, written `[[target]]` with an optional `#fragment` and `|alias`, resolved live by case-insensitive name against the bikeshed; a wikilink that resolves to no note is a broken link.
_Avoid_: internal link, embed

**Backlink**:
A note that contains a wikilink resolving to a given note; the open note's backlinks are what the Backlinks panel lists. Derived state recomputed from the file set — broken links contribute nothing.
_Avoid_: inbound link, references, mentions

**Spokes**:
The wikilinks a note sends out, seen as its connections to the rest of the bikeshed. Every note is a small hub with its own spokes.
_Avoid_: links (too broad), connections

**Attachment**:
A non-markdown file (image, etc.) living in the bikeshed. Invisible in the file tree; reachable only by relative path from the note that references it.
_Avoid_: asset, media file, resource

### The users

**Human**:
A person using Tandem through the board in a browser.
_Avoid_: user (ambiguous with the OS account), client, owner

**Agent**:
A local AI coding agent (e.g. OpenCode, Claude Code) running as the same OS user on the same machine, interacting with Tandem through `tandem mcp` or the CLI.
_Avoid_: bot, client, automation, user

**Reviewer**:
The role that gates a pull request: it reads the ticket, the diff, the checks and the repo's rules, then either merges a two-way door with low blast radius or hands the change to the human. It applies `code` and any lens without a specialist, and it reads the specialist findings the brief names, recording each as adopted, rebutted or not applicable. Dispatched for one PR, it ends with a verdict.
_Avoid_: approver, critic, merge bot

**Review lens**:
One checklist one reader applies to a pull request, named by the ticket's brief in its `## Lenses` section: `code`, `security`, `ui-design`, `ux`, or `architecture`, one table in `examples/skills/review-lenses/SKILL.md`. A lens is not a pass and not a ceremony; it may have a specialist reader, and the gate stays one: one merge authority, one verdict line. A brief with no `## Lenses` is read with `code` alone.
_Avoid_: reviewer type, reviewer persona, review mode, sub-reviewer

**Specialist**:
A named reader that owns one lens: `architect` owns `architecture`, `security` owns `security`, and `ux` owns `ux`. It contributes constraints at refine as a ticket comment and a conformance read at review as a PR comment, signs as `--author <name>`, owns no ticket, and never merges, pushes or sets a status. It is not a pass and not an assignee.
_Avoid_: sub-reviewer, lens agent, second reviewer

**Retro**:
The role that reads one pass and files what it finds as backlog tickets — the tickets are the output, the report is a receipt. It runs at every batch close, on the pass's own transcript, and is still invocable alone on a pass the human names. It never fixes what it finds, because a retro that changes the work cannot report on it honestly.
_Avoid_: retrospective agent, postmortem, transcript analyst

**Refiner**:
The role that turns a backlog ticket into a brief a builder can take: it grills the decision tree, writes the spec, and splits an effort too big for one pass into tickets that each fit one pass. It chooses the lens owners from the brief's surface and dispatches a specialist only when the lens has a target. It owns no code and no design of its own — the ticket is the whole output.
_Avoid_: spec writer, planner, triage

**CLI**:
Tandem's agent-facing interface: the `tandem-cli` binary `install.sh` puts on PATH beside `tandem`; both names reach the same executable.
_Avoid_: API (too generic), backend, endpoint
