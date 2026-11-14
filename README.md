# Tandem

[![CI](https://github.com/gianniacquisto/tandem/actions/workflows/release.yml/badge.svg?branch=main)](https://github.com/gianniacquisto/tandem/actions/workflows/release.yml)
[![Release](https://img.shields.io/github/v/release/gianniacquisto/tandem)](https://github.com/gianniacquisto/tandem/releases/latest)
[![single binary: macOS arm64 and Linux x86_64](https://img.shields.io/badge/single%20binary-macOS%20arm64%20%2B%20Linux%20x86__64-lightgrey.svg)](https://github.com/gianniacquisto/tandem/releases/latest)
[![Licence](https://img.shields.io/badge/licence-MIT-sage.svg)](LICENSE)

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

**Tandem is a board for you and your AI coding agents that ships as one binary.** One ticket list, one board by status and lane, an inbox for what the agents owe you, and the bikeshed (a folder of markdown notes) that humans and agents share. Your agent and you share one list: it files tickets you see, and it sees the ones you close. Everything runs on your machine: one SQLite database and a folder of markdown notes you own, with no account to create. `tandem serve` opens the board in your browser; `tandem mcp` serves the agent face from the same binary. A hosted core is a later deployment shape, so the boundary is today's, not the product's.

## Install

Tandem ships as one binary. A release publishes a standalone `tandem` archive for macOS on Apple silicon and for Linux on x86_64, with `install.sh` and one `SHA256SUMS` beside them.

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh
```

The script picks this machine's archive, verifies its SHA-256 against the release's manifest before writing anything, and installs `tandem` into `~/.local/bin` (or `$TANDEM_INSTALL_DIR`, or `--bin-dir`). It also writes a `tandem-cli` link to the same binary, so both names work. An existing `tandem` is left alone unless you pass `--reinstall`. To remove it later:

```sh
curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --uninstall
```

`--uninstall` removes the binary and its link; `~/.tandem` (the store and the notes) stays. On Windows, download `tandem-<version>-windows-x86_64.zip`, extract `tandem.exe` and put it on PATH. [`docs/install-paths.md`](docs/install-paths.md) is the install page.

Then start the board:

```sh
tandem serve
```

It prints the loopback address it bound; `tandem serve --open` opens the browser for you.

### First launch on macOS

macOS quarantines a downloaded archive. The installer clears the flag on macOS (`xattr -dr com.apple.quarantine`); if you unpack an archive by hand, run that command on the installed binary yourself. Linux archives carry no such flag.

### Check the install

```sh
tandem doctor
```

One JSON report of where your state lives and whether it is healthy: the home directory, the store and the bikeshed. It exits `1` when the store or the bikeshed needs attention, so it is the quickest way to know a fresh install is whole.

### What the release covers

| | Supported |
| --- | --- |
| macOS | Apple silicon (a `darwin-arm64` archive) |
| Linux | x86_64 (a `linux-x86_64` archive) |
| Windows | x86_64 (a `windows-x86_64` zip, extracted by hand) |
| Agent hosts | OpenCode (skills, agent prompts, `AGENTS.md`), Claude Code (skills), and any host that reads skills from a directory |
| Network | none today, `tandem` is local |
| Accounts | none today, the state is files in your home directory |

Every tag publishes those three archives. CI runs the cargo gates on macOS,
Windows and Ubuntu, and the installer and archived-binary smoke tests on macOS
and Linux, so a checkout on any of the three builds and tests what it can run.
[CONTRIBUTING.md](CONTRIBUTING.md) is the per-platform on-ramp.

## What it does

The board and the agent read and write the same store, and the server watches it: a ticket an agent files appears in your browser, and a ticket you close is gone from the agent's next read.

| | Since |
| --- | --- |
| `tandem serve`, the board in the browser, one column per Status | CUT-14 |
| The board: Status columns across, cycle lanes down, a ticket drawn where its Status and lane meet | v0.12.0 |
| The ticket model: Status, Archived, a description, an assignee, a cycle lane | v0.12.0 |
| `in-review`, the sixth Status, and `origin`, who filed a ticket | v0.15.0 |
| The inbox: the tickets the agents assigned to you | v0.12.0 |
| The archive: set a ticket aside and bring it back | v0.12.0 |
| The ticket sheet: open a ticket to edit it whole and read its comments | v0.12.0 |
| The comment log: a ticket's discussion, appended to and never rewritten | v0.15.0 |
| A comment composer in the ticket sheet | v0.16.0 |
| The bikeshed: a folder of markdown notes, watched live | v0.6.0 |
| Note editing: a textarea with a preview | CUT-7 |
| A tree drag that re-parents a note and rewrites the wikilinks that resolve to it | v0.15.0 |
| The health report: broken links, orphan notes | v0.15.0 |
| The store: one SQLite database, watched live | v0.1.0 (a JSON file until v0.27.0) |
| The rolling backup `tandem.db.bak`: the one previous good copy | v0.15.0 (a whole-database copy since v0.27.0) |
| Standalone archives, `SHA256SUMS` and `install.sh` | CUT-16 |

The board is served by `tandem serve`; there is no desktop bundle and no renderer build.

### The notes — where a project's page starts

The bikeshed is a wiki, not a log: its pages hold what a project is, how to run it, where things live, what is open now, and the decisions behind it. It does not hold the story of what was tried: git history and the ticket comments are that.

Every project has one entry point, its **Overview**:

```
~/.tandem/bikeshed/projects/<project>/<project>.md
```

The prose says what the project is, how to run it and where things live; its `## Threads` list is what is open now (the board is the full list — `tandem-cli ticket list`); and it links the project's Decisions, Risks, Processes and sub-pages. Each page names the date it was last true, `Reviewed <date>`, and the Notes page's mtime is the same signal — a page that has drifted is the page to fix.

Start at `~/.tandem/bikeshed/Index.md`, or the board's Notes page, when you do not know the project's name yet.

## The agent's CLI

### `tandem` and `tandem-cli` on PATH

The installer puts the binary in `~/.local/bin` and links `tandem-cli` to it, so both names are the same executable. Add `~/.local/bin` to PATH if the installer says it is missing:

```sh
export PATH="$HOME/.local/bin:$PATH"
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
                      [--append-description <string|->]
tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
tandem-cli ticket complete --id <id>
tandem-cli ticket archive [--id <id>]
tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
tandem-cli ticket comment list --id <id>

tandem-cli lane list
tandem-cli lane add --name <name> [--abbr <key>] [--project <name>]
tandem-cli lane rename --from <name> --to <name> [--abbr <key>] [--project <name>]
tandem-cli lane delete --name <name>
tandem-cli lane move --name <name> [--index <number>]
tandem-cli project list
tandem-cli project add --name <name>
tandem-cli project rename --from <name> --to <name>
tandem-cli project delete --name <name>

tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
tandem-cli doctor
```

The Statuses are `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done` (`on-hold` is the parked slot: a blocker, a human decision pending); a ticket's cycle lane is named with `--lane`, and the unnamed default area always comes first. A ticket in a lane carries a **ref** — the lane's key and a per-lane number, `TAN-42` — and every `--id` (or `--ticket`) accepts it beside the full UUID; the default area's tickets have none, and a lane's key is frozen once its tickets carry numbers.

`todo` still answers for `ticket` as a deprecated alias: same parse, same output, one line on stderr saying so, and the `list` reply carries both `tickets` and `todos` with the same array ([ADR 0030](docs/adr/0030-one-entity-the-ticket.md)). The alias and the doubled reply go in the release after the one that deprecated them.

That block is `main`'s grammar, the trunk the next release is cut from: `tandem-cli --help` prints the whole of it, and `--help` on any subcommand or verb prints that subcommand's own usage (`tandem-cli note rename --help` the `note rename` line) on stdout, exit 0.

### The agent examples

The app no longer carries an agent face (ADR 0045). The skills and the pass prompts live in `examples/` for this repository's own workflow: a clone is the copy, and a host reads them from there or from a checkout. There is no `tandem-cli setup` verb, no `<TandemHome>/agent/` installed copy and no host symlink to keep in step.

## The store

`~/.tandem/tandem.db` is the store: one SQLite database holding the tickets and their cycle lanes, the comment logs and the projects. The server watches it, so the board updates live whenever anything changes it, including an agent. There is no second source of truth and no sidecar file to keep in step; the bikeshed's notes stay plain markdown beside it.

The tickets are rows, in stored order — `tickets.position` **is** the list order, new tickets land on top, and the order is never recomputed from `createdAt`. Each ticket's `lane` names one of the `lanes` rows, and a null lane is the default (ungrouped) area, which always comes first. The columns are typed, so a count is a query: the open and done counts come straight out of the database rather than from loading the list. See [ADR 0031](docs/adr/0031-one-database-the-store.md) for the decision, [ADR 0006](docs/adr/0006-stored-order-and-sections.md) for order and lanes, and [ADR 0030](docs/adr/0030-one-entity-the-ticket.md) for the vocabulary.

The schema is versioned in the database itself: `schema_migrations` holds one row per migration — a version and when it was applied — so "which shape is this store?" is a query, and a migration that fails rolls back whole rather than leaving the store half-upgraded.

Beside the store, every changed write keeps **one previous complete copy** in `tandem.db.bak`, taken through SQLite's backup API before the write touches the database. There is one generation, no rotation. Recovery is the backup and two commands, and the journal files go first or the restored file is read under a dead WAL:

```sh
rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
```

**Quit the server first.** Its watch holds `tandem.db` open for as long as it runs. On macOS and Linux the rename over the held file is allowed, and the running server notices the replacement and re-attaches to the restored store by itself. Windows does not allow it: SQLite opens the database without `FILE_SHARE_DELETE`, so the `rm` and the `mv` over it fail with *The process cannot access the file because it is being used by another process.* Close `tandem serve`, and any `tandem mcp` process, before the two commands, then start them again: the next open attaches to the restored file, and its first write switches it back to WAL. On Windows the home those two commands name is `%USERPROFILE%\.tandem`.

See [ADR 0018](docs/adr/0018-rolling-store-backup.md).

### Moving off the JSON store

Tandem's store was `tickets.json` until v0.27.0, with the comments, sessions, sprints and settings in files beside it. A home from that era is read as it is: a read falls back to the JSON while the database has not imported it, and the first write to each domain imports its file once — copying it aside as `tickets.json.pre-sqlite-<timestamp>` and recording the import in the database (`store_imports`). After that the file is never read again, so a hand-restored `tickets.json` beside a database that already imported it is **ignored**. Copying a file back is not a recovery path; the backup is, or, deliberately, dropping the domain's import row so the next write imports the file again. The same marker silences writes from the other direction: a pre-v0.27.0 build writing `tickets.json` beside the database is ignored too — its writes never reach the store — so upgrade every writer on the Mac, not just one.

## What it is not yet

- **Not a service yet.** There is no account to create and nothing syncs; the state is one SQLite database and a folder of notes in your home directory. A hosted core for several humans is a later deployment shape, not a feature you have now.
- **Not a team tool yet.** One Mac, one store. There is no shared board and no realtime collaboration; the bikeshed is not shareable either.
- **Not every platform.** The release publishes macOS arm64, Linux x86_64 and Windows x86_64 archives. Linux arm64, Intel Mac and static musl builds are later work.
- **Not a tracker with edges.** The board has no labels and no child tickets; blocking is its one edge, a ticket waiting on another ([ADR 0035](docs/adr/0035-ticket-dependencies.md)).
- **Not a text file.** The store is a database the server and the CLI own; the bikeshed is where prose, and hand edits, belong. A JSON store from an older build is read once and then ignored.

## How to contribute

The tracker is the **Tandem board**, not GitHub issues. The tracker contract is the `tandem` skill (`examples/skills/tandem/SKILL.md`).

- **Branch off `main`.** `main` is the trunk: it takes merges, and a release is a lightweight tag on its head, so installs come from the tag ([ADR 0019](docs/adr/0019-integration-branch.md), amended 2026-10-29). Pull requests target `main`.
- **Merge, don't rewrite.** Prefer merge commits over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`.
- **Run the gates.** `cargo test`, `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` in `rust/`.
- **Use the project's words.** [GLOSSARY.md](GLOSSARY.md) is the vocabulary; the ADRs are the decisions. Respect the ADRs in the area your change touches, and amend one in the same pull request when you change what it decided.
- **Keep the ticket whole.** A ticket's description is written once as the brief and appended to with the outcome, never overwritten. The shape of that brief is the refiner's: `examples/agents/refiner.md`.

## Releasing

See [docs/releasing.md](docs/releasing.md).

## Licence

MIT — [LICENSE](LICENSE). The third-party notices are in [NOTICE.txt](NOTICE.txt).

## Where the ADRs live

The decisions this project has made, one file each, are in [docs/adr/](docs/adr/) — the store protocol, the distribution, the bikeshed, the ticket model, the CLI, the board and notes cut, the branch policy. Read the newest one before you change what it decided.
