# Tandem

[![CI](https://github.com/gianniacquisto/tandem/actions/workflows/release.yml/badge.svg?branch=main)](https://github.com/gianniacquisto/tandem/actions/workflows/release.yml)
[![Release](https://img.shields.io/github/v/release/gianniacquisto/tandem)](https://github.com/gianniacquisto/tandem/releases/latest)
[![macOS](https://img.shields.io/badge/macOS-27%2B%20%2B%20Apple%20silicon-lightgrey.svg)](https://github.com/gianniacquisto/tandem/releases/latest)
[![Licence](https://img.shields.io/badge/licence-MIT-sage.svg)](LICENSE)

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

**Tandem is a macOS board for you and your AI coding agents.** One ticket list, one board by status and lane, an inbox for what the agents owe you, and the bikeshed — a folder of markdown notes — that doubles as the agents' memory. Your agent and you share one list: it files tickets you see, and it sees the ones you close. Everything runs on your Mac today: one SQLite database and a folder of markdown notes you own, with no account to create. A hosted core is a later deployment shape, so the boundary is today's, not the product's.

## Install

macOS 27 (Golden Gate) or later, on an Apple silicon Mac.

```sh
brew install --cask gianniacquisto/tap/tandem
```

Spell the tap out in full, every time. Homebrew core has an unrelated cask named `tandem`, and the Caskroom is keyed by the token — a bare `brew install --cask tandem` installs that one over this app and offers it as an upgrade forever. The install puts `Tandem.app` in `/Applications` and links the bundled `tandem-cli` into Homebrew's `bin`.

Without Homebrew, the same zip installs through a script the release publishes beside it:

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh
```

The script verifies the zip against the release's `SHA256SUMS` before writing anything, copies `Tandem.app` to `/Applications`, and leaves an existing install (a cask or a previous script run) alone unless re-run with `--reinstall`. It reports the `tandem-cli` link state instead of changing it. To remove a script install later:

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --uninstall
```

(The cask stays the first-documented path; [`docs/install-paths.md`](docs/install-paths.md) compares the two.)

### First launch (once per install or upgrade)

Tandem is ad-hoc signed, not Developer ID signed or notarized, so macOS still shows a Gatekeeper warning the first time you launch it after a download, and the app quits by itself a few seconds later. The bundle's signature is valid and its resources are sealed, so the warning is the missing notarization, not a damaged app. Clear the download flag once, then relaunch:

```sh
xattr -dr com.apple.quarantine /Applications/Tandem.app
```

Tandem then opens normally until the next install or upgrade.

### Check the install

```sh
tandem-cli doctor
```

One JSON report of where your state lives and whether it is healthy: the home directory, the store, the bikeshed, the comment log, the write lock, the installed app and the agent skills. It exits `1` when something needs attention, so it is the quickest way to know a fresh install is whole.

### What it runs on

| | Supported |
| --- | --- |
| macOS | 27 (Golden Gate) or later |
| Hardware | Apple silicon |
| Agent hosts | OpenCode (skills, agent prompts, `AGENTS.md`), Claude Code (skills), and any host that reads skills from a directory |
| Network | none today, the app and the CLI are local |
| Accounts | none today, the state is files in your home directory |

## What it does

The app and the agent read and write the same store, and the app watches it: a ticket an agent files appears in your window, and a ticket you close is gone from the agent's next read.

| | Since |
| --- | --- |
| The main window and the menu bar item — the same list, two faces | v0.1.0 |
| A ticket list in stored order, grouped by cycle lane, drag to reorder | v0.8.0 |
| Search across every lane, by title and description | v0.10.0 |
| The board — Status columns across, cycle lanes down, a ticket drawn where its Status and lane meet | v0.12.0 |
| The ticket model — Status, Archived, a description, an assignee, a cycle lane | v0.12.0 |
| `in-review`, the sixth Status, and `origin`, who filed a ticket | v0.15.0 |
| The inbox — the tickets the agents assigned to you | v0.12.0 |
| The archive — a fourth state, and the way back | v0.12.0 |
| The ticket sheet — double-click a ticket to edit it whole | v0.12.0 |
| The comment log — a ticket's discussion, appended to and never rewritten | v0.15.0 |
| A comment composer in the ticket sheet, so a human can answer in the app | v0.16.0 |
| Quick open (⌘K) — one ranked list over the notes and the tickets | v0.15.0 |
| An undo toast: one click back from a mis-clicked completion | v0.15.0 |
| The bikeshed — a folder of markdown notes, watched live | v0.6.0 |
| The editor — a CodeMirror page, a formatting toolbar, the folder you choose | v0.10.0 |
| A tree drag that re-parents a note and rewrites the wikilinks that resolve to it | v0.15.0 |
| The health report — broken links, orphan notes | v0.15.0 |
| The hidden layer — the saddlebag of captures, revealed on request | v0.15.0 |
| The note↔ticket bridge — "Send to tickets", "Promote to note" | v0.15.0 |
| The face says tickets — the tab, the search field, the button | v0.16.0 |
| A first-run setup wizard | v0.10.0 |
| The store — one SQLite database, watched live | v0.1.0 (a JSON file until v0.27.0) |
| The rolling backup `tandem.db.bak` — the one previous good copy | v0.15.0 (a whole-database copy since v0.27.0) |
| The session record — an agent session's own artifact, in the store | v0.16.0 |
| A development build that keeps its own state in `~/.tandem-dev`, so it can never touch yours | v0.9.0 |

Every picture below, and every word of the app's own copy in it, is `main`'s, the trunk the next release is cut from. An installed build can lag `main` by a release.

The board: the same tickets, laid out by where they are in the workflow.

![The board — Status columns across, cycle lanes down](docs/screenshots/board.png)

The ticket sheet: a ticket's title, its Status, its lane, who filed it, the brief, and the discussion under it.

![The ticket sheet — title, Status, lane, description and comments](docs/screenshots/ticket-sheet.png)

The bikeshed: the notes the app watches and the agents write, with the tree on the left, the note in the editor, and its backlinks below. **Send to tickets** files the selection as a ticket.

![The bikeshed — the notes tree, the editor and the backlinks](docs/screenshots/bikeshed.png)

### The notes — where a project's page starts

The bikeshed is a wiki, not a log: its pages hold what a project is, how to run it, where things live, what is open now, and the decisions behind it. It does not hold the story of what was tried — git history and the session records are that.

Every project has one entry point, its **Overview**:

```
~/.tandem/bikeshed/projects/<project>/<project>.md
```

The prose says what the project is, how to run it and where things live; its `## Threads` list is what is open now (the board is the full list — `tandem-cli ticket list`); and it links the project's Decisions, Risks, Processes and sub-pages. Each page names the date it was last true, `Reviewed <date>`, and the Notes tab's mtime is the same signal — a page that has drifted is the page to fix.

Start at `~/.tandem/bikeshed/Index.md`, or the app's Notes tab, when you do not know the project's name yet.

## The agent face

### `tandem-cli` on PATH

The cask links the bundled binary into Homebrew's `bin`, so `tandem-cli` is on your PATH after the install. Where the app is installed but the link is not:

```sh
ln -s "/Applications/Tandem.app/Contents/MacOS/tandem-cli" "$(brew --prefix)/bin/tandem-cli"
```

Every command prints JSON on stdout.

```sh
tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                       [--assignee <name|self>] [--lane <name>] [--text <string>]
                       [--archived] [--blocked]
tandem-cli ticket show --id <id>
tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                      [--lane <name>] [--origin <name|self>] [--description <string|->]
tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                      [--assignee <name|self>] [--description <string|->]
tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
tandem-cli ticket complete --id <id>
tandem-cli ticket answer --id <id> --status <value> [--assignee <name|self>]
                         [--body <string|->]
tandem-cli ticket archive [--id <id>]
tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
tandem-cli ticket comment list --id <id>

tandem-cli lane list
tandem-cli lane add --name <name> [--abbr <key>]
tandem-cli lane rename --from <name> --to <name> [--abbr <key>]
tandem-cli lane delete --name <name>
tandem-cli note rename --from <path> --to <path>
tandem-cli note capture --kind <decision|gotcha|fact|thread> [--project <name>] [--session <id>]
tandem-cli note stats [--project <name>]
tandem-cli note links
tandem-cli session start [--project <name>] [--agent <name>] [--session <id>]
                         [--harness-session <id>] [--harness-transport <value>]
                         [--brief <string|->]
tandem-cli session end [--session <id>] [--outcome <value>] [--summary <string|->]
                       [--tokens-input <n>] [--tokens-output <n>]
                       [--tokens-reasoning <n>] [--tokens-cache-read <n>]
                       [--tokens-cache-write <n>] [--cost <dollars>] [--model <name>]
                       [--skills <name=count[,name=count]>]
                       [--usage-from-harness] [--harness-events <path>]
tandem-cli session link --ticket <id> [--session <id>]
tandem-cli session list [--project <name>] [--status <value>]
tandem-cli session show [--session <id>]
tandem-cli sprint open --lane <name> [--mode serial|parallel] [--session <id>]
tandem-cli sprint ticket --lane <name> --ticket <id>
tandem-cli sprint run --lane <name> --note <string|->
tandem-cli sprint close --lane <name> [--number <n>] [--note <string|->]
tandem-cli sprint list [--lane <name>] [--status open|closed]
tandem-cli sprint show --lane <name> --number <n>
tandem-cli setup install [--host <opencode|claude|agents>] [--force]
tandem-cli setup uninstall [--host <opencode|claude|agents>]
tandem-cli setup status
tandem-cli doctor
```

The Statuses are `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done` (`on-hold` is the parked slot: a blocker, a human decision pending); a ticket's cycle lane is named with `--lane`, and the unnamed default area always comes first. A ticket in a lane carries a **ref** — the lane's key and a per-lane number, `TAN-42` — and every `--id` (or `--ticket`) accepts it beside the full UUID; the default area's tickets have none, and a lane's key is frozen once its tickets carry numbers. A session keeps its own vocabulary — `running` and `ended`, and an outcome of `succeeded`, `failed`, `interrupted` or `unknown` — and never borrows the board's Status. A sprint keeps its own vocabulary too — `open` and `closed`, with a batch built `serial` or `parallel` — and never borrows the board's Status either.

`todo` still answers for `ticket` as a deprecated alias: same parse, same output, one line on stderr saying so, and the `list` reply carries both `tickets` and `todos` with the same array ([ADR 0030](docs/adr/0030-one-entity-the-ticket.md)). The alias and the doubled reply go in the release after the one that deprecated them.

That block is `main`'s grammar, the trunk the next release is cut from: `tandem-cli --help` prints the whole of it, and `--help` on any subcommand or verb prints that subcommand's own usage (`tandem-cli note capture --help` the `note capture` line) on stdout, exit 0.

### A session is its own record

Not a ticket: `session start` opens a record of one agent session's run — the project it works in, the agent's name, the brief it was given — `session end` stamps how it ended and what it did, `session link` ties the record to a ticket, and `session list` and `session show` read the record back. The record is a row of the store's `sessions` table, beside the tickets, and is created once, stamped once and linked to — there is no edit and no delete.

### The skills

| skill | what it is for | since |
| --- | --- | --- |
| `tandem` | the CLI contract an agent reads before it touches the store | v0.9.0 |
| `tandem-memory` | the bikeshed as the agent's memory: orient from the project's **Overview**, record the session, consolidate into the curated notes on request | v0.6.0, the capture-and-consolidation protocol in v0.11.0 |
| `tandem-sprint` | the SDLC in one invocation — the loop refine → build → review → retro: orient, triage with refine filling the batch, the batch through `tandem-build`, retro at the close | v0.15.0 |
| `tandem-build` | the sprint's build phase: each batch ticket claimed, built in its own worktree, gated through review, recorded | v0.15.0 (as `tandem-sprint`; renamed 2026-10-24) |
| `tandem-refine` | dispatches the refiner to shape a backlog ticket into a buildable brief | v0.16.0 |
| `tandem-setup` | installs the whole agent face on a host | v0.16.0 |
| `tandem-consolidate` | dispatches the consolidator pass | v0.16.0 |
| `tandem-tidy` | the board hygiene pass: sweeps open tickets against landed work, completes the done or moot with evidence, archives, reports the oddities | unreleased |
| `review-lenses` | the checklists the reviewer applies to a PR, named by the brief's `## Lenses`: `code`, `security`, `ui-design`, `ux`, `architecture` | unreleased |

`skills/grilling/`, `skills/tdd/` and `skills/writing-for-agents/` are vendored from [mattpocock/skills](https://github.com/mattpocock/skills) (MIT), so a clone of this repo is the whole workflow; `skills/retro/` is adapted from the same collection, its output filed as tickets on the board.

### Setting a host up

The app carries its own agent face: every skill, the four prompts (`builder`, `consolidator`, `refiner`, `reviewer`) and `AGENTS.md`. One command installs it into a host, so a checkout is not a prerequisite:

```sh
tandem-cli setup status    # the bundled and installed versions, each host's state, the warnings
tandem-cli setup install   # OpenCode: the skills, the four prompts and the AGENTS.md instruction
```

Install writes the face once to `<TandemHome>/agent/` and points the host's paths at that copy with symlinks, so an upgrade is one directory replace and no re-linking. `--host claude` or `--host agents` names another host, and only a named host is written. A real file where a link would go is never touched: the report names the path, and its hint says to move it.

The app reaches the same install, the same `SetupInstall` the verb calls: the setup wizard's agent step runs it in one press and shows each host's state and the installed version, and Settings' Agent section shows the bundled and installed versions with the same host list and one Install or Repair button. Nothing is written to a host until you press it.

On a machine working on Tandem itself, the host's links already resolve into the checkout. Install leaves them and reports each as a skip (`foreign-link`), and `tandem-cli doctor` stays green either way. `--force` replaces them with the installed copy.

Nothing in this repo names a path on one machine: the installed app is the whole setup, and a clone of it anywhere behaves the same. A host the three `--host` names do not cover is the human's call: it reads the installed copy under `<TandemHome>/agent/skills/`.

## The store

`~/.tandem/tandem.db` is the store: one SQLite database holding the tickets and their cycle lanes, the comment logs, the session and sprint records, and the app's settings. The app watches it, so the list updates live whenever anything changes it — including an agent. There is no second source of truth and no sidecar file to keep in step; the bikeshed's notes stay plain markdown beside it.

The tickets are rows, in stored order — `tickets.position` **is** the list order, new tickets land on top, and the order is never recomputed from `createdAt`. Each ticket's `lane` names one of the `lanes` rows, and a null lane is the default (ungrouped) area, which always comes first. The columns are typed, so a count is a query: the open and done counts come straight out of the database rather than from loading the list. See [ADR 0031](docs/adr/0031-one-database-the-store.md) for the decision, [ADR 0006](docs/adr/0006-stored-order-and-sections.md) for order and lanes, and [ADR 0030](docs/adr/0030-one-entity-the-ticket.md) for the vocabulary.

The schema is versioned in the database itself: `schema_migrations` holds one row per migration — a version and when it was applied — so "which shape is this store?" is a query, and a migration that fails rolls back whole rather than leaving the store half-upgraded.

Beside the store, every changed write keeps **one previous complete copy** in `tandem.db.bak`, taken through SQLite's backup API before the write touches the database. There is one generation, no rotation. Recovery is the backup and two commands, and the journal files go first or the restored file is read under a dead WAL:

```sh
rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
```

**Quit the app first.** Its watch holds `tandem.db` open for as long as it runs. On macOS and Linux the rename over the held file is allowed, and the running app notices the replacement and re-attaches to the restored store by itself. Windows does not allow it: SQLite opens the database without `FILE_SHARE_DELETE`, so the `rm` and the `mv` over it fail with *The process cannot access the file because it is being used by another process.* Close Tandem, and any `tandem-cli rpc` child, before the two commands, then open it: the next open attaches to the restored file, and its first write switches it back to WAL. On Windows the home those two commands name is `%USERPROFILE%\.tandem`.

See [ADR 0018](docs/adr/0018-rolling-store-backup.md).

### Moving off the JSON store

Tandem's store was `tickets.json` until v0.27.0, with the comments, sessions, sprints and settings in files beside it. A home from that era is read as it is: a read falls back to the JSON while the database has not imported it, and the first write to each domain imports its file once — copying it aside as `tickets.json.pre-sqlite-<timestamp>` and recording the import in the database (`store_imports`). After that the file is never read again, so a hand-restored `tickets.json` beside a database that already imported it is **ignored**, and `tandem-cli doctor` says so: *a fork, ignored since the database took over*. Copying a file back is not a recovery path — the backup is, or, deliberately, dropping the domain's import row so the next write imports the file again. The same marker silences writes from the other direction: a pre-v0.27.0 build writing `tickets.json` beside the database is ignored too — its writes never reach the store — so upgrade every writer on the Mac, not just the app.

## What it is not yet

- **Not a service yet.** There is no account to create and nothing syncs; the state is one SQLite database and a folder of notes in your home directory. A hosted core for several humans is a later deployment shape, not a feature you have now.
- **Not a team tool yet.** One Mac, one store. There is no shared board and no realtime collaboration; the bikeshed is not shareable either.
- **Not cross-platform.** macOS 27 or later, on Apple silicon.
- **Not a tracker with edges.** The board has no labels, no blocking edges and no child tickets — see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md).
- **Not a text file.** The store is a database the app and the CLI own; the bikeshed is where prose, and hand edits, belong. A JSON store from an older build is read once and then ignored.

## How to contribute

The tracker is the **Tandem board**, not GitHub issues — the tracker contract is the `tandem` skill (`skills/tandem/SKILL.md`).

- **Branch off `main`.** `main` is the trunk: it takes merges, and a release is a lightweight tag on its head, so installs come from the tag ([ADR 0019](docs/adr/0019-integration-branch.md), amended 2026-10-29). Pull requests target `main`.
- **Merge, don't rewrite.** Prefer merge commits over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`.
- **Run the gates.** `cargo test`, `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` in `rust/`; `npm run typecheck && npm test` in `app/`; and in `Editor/`: `npm ci && node --test`. The committed editor bundle is prebuilt, and CI rebuilds it and fails on any byte that differs.
- **Use the project's words.** [GLOSSARY.md](GLOSSARY.md) is the vocabulary; the ADRs are the decisions. Respect the ADRs in the area your change touches, and amend one in the same pull request when you change what it decided.
- **Keep the ticket whole.** A ticket's description is written once as the brief and appended to with the outcome, never overwritten. The shape of that brief is the refiner's: `agents/refiner.md`.

## Releasing

See [docs/releasing.md](docs/releasing.md).

## Licence

MIT — [LICENSE](LICENSE). The third-party notices are in [NOTICE.txt](NOTICE.txt).

## Where the ADRs live

The decisions this project has made, one file each, are in [docs/adr/](docs/adr/) — the store protocol, the distribution, the bikeshed, the ticket model, the CLI, the memory protocol, the branch policy. Read the newest one before you change what it decided.
