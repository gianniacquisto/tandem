# Tandem

Tandem is a native macOS todo app for you and your AI coding agents.

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

## Install

macOS 27 or later, on an Apple silicon Mac.

```sh
brew install --cask gianniacquisto/tap/tandem
```

That puts `Tandem.app` in `/Applications`. Homebrew will ask you to trust the tap the first time — that's normal.

### First launch (once per install or upgrade)

Tandem is signed but not notarized by Apple, so macOS shows a Gatekeeper warning the first time you launch it after a download, and the app quits by itself a few seconds later. Clear the download flag once, then relaunch:

```sh
xattr -dr com.apple.quarantine /Applications/Tandem.app
```

Tandem then opens normally until the next install or upgrade.

## How it works

All your todos live in `~/.tandem/todos.json`, a plain JSON file you can edit by hand. A build you run from source keeps its own list in `~/.tandem-dev` instead, so developing Tandem can never touch the real one.

The app has a main window and a menu bar item. It watches the file, so the list updates live whenever anything changes it.

Agents read and write the same file through `tandem-cli`, the command line the app bundles (see below). When an agent adds a todo, it shows up in your app; when you check one off, the agent sees it on its next read.

Everything runs locally on your Mac. No accounts, no network.

## Using Tandem from an agent

The app bundles `tandem-cli`, the command line your agent reads and writes the list through. It lives at a stable in-bundle path:

```sh
/Applications/Tandem.app/Contents/MacOS/tandem-cli
```

Four commands:

```sh
tandem-cli todo list [--filter open|done|all]
tandem-cli todo add --title "Water the plants"
tandem-cli todo complete --id <id>
tandem-cli note rename --from "Ideas.md" --to "Backlog.md"
```

`tandem-cli doctor` is a fifth, setup-only command: it prints one JSON report of where your state lives and whether it is healthy (the store, the bikeshed, the lock, the installed app, and the agent skill), and exits `1` if something needs attention. It is the quickest way to check a fresh install.

`tandem-cli --help` prints that syntax and exits `0`. The `tandem` skill, in `skills/tandem/`, teaches an agent those contracts. The whole agent setup — the skills, the agent prompts and this repo's `AGENTS.md` — is the `tandem-setup` skill: ask your agent to set up Tandem, and it links `skills/*` into `~/.config/opencode/skills/`, `agents/*.md` into `~/.config/opencode/agents/`, and `AGENTS.md` into `~/.config/opencode/AGENTS.md`, all pointing back at this checkout.

Links, not copies: an edit in the repo is an edit the host sees. The setup never overwrites a real file, and it reports what it skipped.

For another host, the same idea: link the skills into that host's skills directory — Claude Code takes them in `~/.claude/skills/`, for instance:

```sh
ln -s "$PWD/skills/tandem" ~/.claude/skills/tandem
```

Nothing in this repo names a path on one machine. A clone of it, anywhere, plus the installed app, is the whole setup.

### The `tandem-memory` skill

`skills/tandem-memory/` is an agent skill that makes the bikeshed (`~/.tandem/bikeshed/`) the agent's memory. A session orients from the bikeshed's `Index.md` and its project notes as it starts, then records decisions, gotchas, and open threads as it works, so the next session picks up where the last one left off. The notes double as a wiki of the work, navigable by wikilinks and backlinks in the app.

This skill is a folder, not a single file, so it installs differently from the one-file skills. `SKILL.md` is the entry point — the only file the harness loads when the skill fires — and it holds the two things the agent does: **orient**, then **record**. The other files are companions that `SKILL.md` points to and the agent reads only when it needs them:

| file | what the agent reaches it for |
| --- | --- |
| `CONVENTIONS.md` | where notes go and how they are named |
| `WRITE-PROTOCOL.md` | when and what to record |
| `READ-PROTOCOL.md` | how to orient and find related notes |
| `CONCURRENCY.md` | two sessions writing the same bikeshed |

Install it with the `tandem-setup` skill, which links the whole `skills/` directory into `~/.config/opencode/skills/` — one copy, no re-install when the repo changes.

Pointing a host at this repo's `skills/` directory works too, and is the same thing by another route:

```json
{
  "skills": ["/path/to/tandem/skills"]
}
```

Or copy the whole `tandem-memory/` folder into your harness's skills directory the way you would a single-file skill, keeping the files together — `SKILL.md` reaches the companions by their relative paths, so they only work as a set. Don't install the four companions as skills of their own.

### The `tandem-handoff` skill

`skills/tandem-handoff/` assembles the orient a session does on its own into one self-contained block a human can paste into a new agent session: the open tickets (`tandem-cli todo list --filter open`), the bikeshed `Index.md`, a project's `Where left off` and Threads, and the open tickets assigned to `human`. It is **read-only** — it edits no note and changes no ticket. Give it an optional project name for the full block; with none it prints the Index plus every project's one-line status. Ask for it in words — the skill is model-invocable — or bind a `/handoff` command to it in your host.

### The `agents/` directory

`agents/` holds the agent prompts this project owns — `consolidator.md`, `refine.md`, `retro.md` and `reviewer.md` — one markdown file per agent, each with the frontmatter a host reads (a `description`, a `mode`, a `model`).

They are the passes that run on Tandem: the consolidator folds the bikeshed's captures into the curated notes, the refine agent grills a `backlog` ticket into a brief a builder can take, the retro reads a session and files what it finds as backlog tickets, the reviewer reads a ticket and a PR and merges the two-way doors. The `tandem-consolidate` skill dispatches the consolidator; this repo ships no host commands, so every pass is reached by asking, or by whatever binding the host adds. See [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md).

`skills/grilling/`, `skills/implement-spec/`, `skills/retro/`, `skills/to-spec/` and `skills/writing-for-agents/` are vendored from [mattpocock/skills](https://github.com/mattpocock/skills) (MIT) so a clone of this repo is the whole workflow: the retro pass's categories, the note-writing standard, and the refine pass's three procedures all come from here now. `docs/agents/issue-tracker.md` tells them the board is the tracker.

The two builders this workflow also uses — `black` and `blue`, one prompt and two models — and the `terse` style live in the [fleet](https://github.com/gianniacquisto/fleet) repo, not here.

## The store file

`~/.tandem/todos.json` looks like this:

```json
{
  "version": 1,
  "sections": ["Errands"],
  "todos": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "title": "Water the plants",
      "done": false,
      "section": null,
      "createdAt": "2026-10-09T14:03:22Z",
      "completedAt": null
    }
  ]
}
```

The order of `todos` **is** the list order — new todos land on top, and the order is never recomputed from `createdAt`, so you can rearrange the file by hand. Each todo's `section` names one of the `sections`; `null` is the default (ungrouped) area, which always comes first. See [ADR 0001](docs/adr/0001-store-protocol.md) for the file protocol and [ADR 0006](docs/adr/0006-stored-order-and-sections.md) for order and sections.

`section` and `sections` are **additive**: a file from before they existed reads fine, and older builds read this one — they simply ignore the fields. That is also why the envelope version stays `1`; only a change an older reader would *misread* earns a bump.

If the file ever gets corrupted, Tandem moves it to `todos.json.corrupt-<timestamp>` and starts with an empty list. Beside it, every write also keeps **one previous good copy** in `todos.json.bak` — the state of the file before that write — so an ordinary bad write (a kill mid-rename, a bad edit by hand or by an agent) is one `mv` from recovery: `mv ~/.tandem/todos.json.bak ~/.tandem/todos.json`. There is one generation, no rotation: `todos.json.bak` is the file as it was before the write that replaced it. See [ADR 0018](docs/adr/0018-rolling-store-backup.md).

## Releasing

See [docs/releasing.md](docs/releasing.md).
