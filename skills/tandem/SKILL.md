---
name: tandem
description: "Read and write the shared Tandem ticket list, manage its cycle lanes, and rename bikeshed notes, from `tandem-cli` on this Mac. Reach for it when the user says to add a todo, asks what's on their list, wants a ticket's status or assignee changed, wants a cycle lane created, renamed, or deleted, asks to mark a todo done, or wants a note renamed (reopened under a new name). Also runs a local setup check when the user asks whether Tandem is installed or working. The same tickets a human sees and edits in the Tandem app."
---

# Tandem

`tandem-cli` is Tandem's agent-facing interface on this Mac — the binary the app bundles (`/Applications/Tandem.app/Contents/MacOS/tandem-cli`, stable across upgrades), so an agent reads and writes the same list and notes a human uses in the app. `tandem-cli --help` prints the syntax and exits `0`.

## The store

`~/.tandem/todos.json`, the human's real list. A from-source build (`swift run tandem-cli`) writes `~/.tandem-dev/` instead, so development can never reach the real list; notes live beside it (`~/.tandem/bikeshed/`, `~/.tandem-dev/bikeshed/` from source). Every write keeps one previous good copy beside it — `todos.json.bak` — and `mv ~/.tandem/todos.json.bak ~/.tandem/todos.json` is the one-command recovery; the app reloads it on its own.

## Tickets

Six verbs under `ticket` — `list`, `add`, `set`, `move`, `complete`, `archive` — plus `comment` with sub-verbs of its own. Every object they return has the same shape:

| field | meaning |
| --- | --- |
| `id` | pass it back to `set`, `move`, `complete` |
| `title` | one line, trimmed |
| `status` | `backlog` (default), `refined`, `ready`, `in-progress`, `in-review`, `done` |
| `archived` | true when set aside — out of the default views, still in the store |
| `description` | markdown body, or null; write from stdin with `ticket set --description -` |
| `assignee` | `human`, an agent's name, or null for unowned |
| `origin` | who filed it — written at add, never changed |
| `section` | the **cycle lane**, or null for the default area; the wire keeps its v1 name |
| `done` | derived from `status == "done"`, kept on the wire for older readers — read `status`, not this |
| `link` | a bikeshed note path, or null; a human sets it in the app |
| `createdAt`, `completedAt` | ISO-8601 UTC; `completedAt` is null unless done |

`status` is the truth, and a change is free-form — any status to any other, backwards included — because the board records what is true, not a process.

### `ticket list [--filter open|done|all] [--status <v[,v]>] [--assignee <name|self>] [--lane <name>] [--text <string>] [--archived]`

The tickets in view, in stored order (the order the human arranged; new tickets land on top). Every filter is optional and they combine:

- `--filter` — `open` (default: not done, not archived), `done`, or `all` (every ticket, the archive excluded). The CLI's filters never carry the archive.
- `--status` — comma-separated values, e.g. `--status ready,in-progress`; a repeated flag keeps only its last value.
- `--assignee` — an exact name, `self` for your own (`$TANDEM_AGENT`), or `""` for the unowned.
- `--lane` — a cycle lane name, or `""` for the default area.
- `--text` — a case- and accent-insensitive substring of the title **or the description**.
- `--archived` — no value. Alone it is the archive; with a filter, the archive joins it.

An empty result is a success: `{"tickets":[]}`.

### `ticket add --title <string> [--status <value>] [--assignee <name|self>] [--lane <name>] [--origin <name|self>]`

Title required, trimmed, empty rejected; duplicates allowed. A title alone lands `backlog`, unowned, on top of the default area. `--assignee self` claims it — `add` never assigns implicitly, because agents routinely file tickets *for* the human (`--assignee human`). A lane must already exist. `--origin` stamps who filed it, never changed after: pass `--origin self` when you file — an omitted flag means `human`, the app's own stamp, and your proposal filed under the human's name is the exact mix-up the field ends.

### `ticket set --id <id> [--title …] [--status …] [--assignee …] [--description …]`

Change the fields you name in **one write**; everything unnamed is left alone. `--description -` reads the body from stdin, which is how multi-line markdown lands intact. `--assignee ""` clears the owner, `--description ""` clears the body, `--assignee human` hands the ticket to the human. Moving to `done` stamps `completedAt`; moving out clears it. An empty `--title` is rejected, and an empty or whitespace-only `--description -` body is rejected too — exit `1`, `Description body is empty…`, the description left as it was: an empty pipe is a failed write, not a clear.

### `ticket move --id <id> [--lane <name>] [--index <number>]`

Move between lanes and/or within one. `--index` is 0-based inside the target lane; omit to append. `--lane ""` moves to the default area; the lane must exist.

### `ticket complete --id <id>`

The short path to `--status done`: idempotent, and an unknown id's error carries the current open list. No `reopen` verb — `ticket set --id … --status backlog` is the reopen.

### `ticket archive`

Archive every done ticket in one call: nothing is deleted, they leave the default views (`--archived` still shows them). Returns `{"archived": <n>}`; a second call returns 0.

### `ticket comment add --id <id> [--author <name>] --body <string|->` / `ticket comment list --id <id>`

Append to / list the ticket's **discussion log** — where agents talk after refinement, since the description stays refinement-only. The log is a sidecar (`~/.tandem/comments/<id>.jsonl`), not a ticket field, so it survives any edit of the ticket. `--body -` reads from stdin; an empty body is rejected. **`--author` signs the line with the name you write — `self` is not expanded for it** (only `--assignee` and `--origin` expand `self`); write the role out (`skills/tandem-team/SKILL.md`), and never omit it: an unnamed author is a gap in the record. Append-only — no edit, no delete; a correction is a new comment. `comment list` returns comments in append order; an unknown id is an empty list, because the log outlives the store's entries.

## Lanes

A **cycle lane** is the area axis — the list groups by it, the board draws its rows; `--lane ""` (or null `section`) is the default area:

| command | returns |
| --- | --- |
| `lane add --name <name>` | `{"lanes": [...]}` |
| `lane rename --from <a> --to <b>` | `{"lanes": [...]}` |
| `lane delete --name <name>` | `{"lanes": [...], "moved": <n>}` |

`lanes` is the list after the write, in render order. Names are trimmed; empty, duplicate, and unknown names are domain failures. `rename` rewrites the lane list and every ticket in it in one write; `delete` never deletes tickets — members fall back to the default area, `moved` is how many. A lane must exist before `ticket add/move --lane` accepts it; there is no `lane list` verb — `ticket list --filter all --archived` shows every lane that holds a ticket.

## Session tickets

Every agent session files one ticket for itself — the convention lives in `AGENTS.md`; this is the CLI's side.

- **Start** — `ticket add --title "Session — <project> · <session-id>" --status in-progress`, then `ticket set --id <id> --description -` with the brief. `<session-id>` is `$OPENCODE_SESSION_ID`.
- **One per session** — check `ticket list --filter all --text "Session — <project> · <session-id>"` first; `--filter all` because the default filter cannot see a ticket this session already closed.
- **End** — append the outcome below the brief and `ticket set --id <id> --status done --description -`; with no outcome, `ticket complete --id <id>`, which leaves the description alone. The empty-stdin refusal above protects the brief.
- **Where they live** — the default area, not a lane, so a session ticket never depends on a lane that may not exist.
- **They are work** — a sprint's triage refines and moves them like any other ticket: a live session owns its ticket, a dead session's ticket is stale work the next triage picks up.

## Notes

The bikeshed (`~/.tandem/bikeshed/`) is a plain folder of `.md` files, and the file itself is the interface — create, edit, and delete curated notes with your own file tools; the CLI writes no note content. Four CLI verbs act on notes: `rename`, `capture`, `stats`, and `links`.

### `note rename --from <path> --to <path>`

Both paths bikeshed-root-relative, `.md` included. Rewrites every wikilink that resolves to the note — bare, qualified, aliased, `#fragment` — in every note including itself, in one locked transaction. Rename through the CLI only: a raw `mv` breaks every wikilink silently, and nothing repairs them but re-running the rename.

### `note capture --kind <decision|gotcha|fact|work|thread> [--project <name>] [--session <id>]`

File one **capture** — a small, create-only record of a moment that would be lost when the session ends. Body from **stdin** (UTF-8; empty allowed). It lands at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`; the slug comes from the body's first line, same-second same-slug notes get a `-2`, `-3`… suffix. `--project` defaults to the working directory's last path component; `--session` to `$OPENCODE_SESSION_ID`, else a generated token. The saddlebag is **hidden**: captures never appear in search, backlinks, or wikilink resolution, and carry no `[[wikilinks]]` — file as you work, and the consolidation pass promotes them into the curated notes. An unknown `--kind`, or a `--project`/`--session` that is not one folder name, is a domain failure.

A note filename may not contain `/`, `#`, `|`, or a leading dot, and must end `.md` — `#` and `|` are the wikilink grammar's separators, so such a name can never be resolved (ADR 0004). The rule constrains what is *created*; pre-existing violators still show and edit.

### `note stats [--project <name>]`

Whether memory is draining — one JSON object, read-only and lock-free. Per project: `curatedNotes`, `rawPending` / `rawArchived` (captures pending under `.raw/`, consumed under `.raw/.done/`), `oldestPending`, `lastConsolidated` (the newest curated-note mtime); plus `totals`. A high `rawPending` or a stale `lastConsolidated` is the queue not draining — the signal to hand the saddlebag to a consolidation pass (`skills/tandem-consolidate/`).

### `note links`

The link graph — `broken` (wikilinks that resolve to nothing: `note`, `link` as written, 1-based `line`), `orphans` (notes with no incoming link), and `counts`. Read-only and lock-free. Rules: a link resolves case-insensitively by name — bare `[[Name]]` or qualified `[[folder/Note]]`, trailing `.md` optional; `#fragment` and `|alias` change the display, never the target; a duplicated basename is a broken link, not a tie-break; `[[…]]` inside code spans or fenced blocks is literal text; the hidden saddlebag stays out of the graph; orphans exclude the entry points (`Index.md`, and a project's anchor note `projects/<project>/<project>.md`). The app's bikeshed **Health** panel reports the same graph. Nothing is repaired for you — a broken link may be an intention; fix by editing the note or renaming through `note rename`.

## Setup check

`tandem-cli doctor` — where this machine's Tandem state lives and whether it is healthy. One JSON object: `{"ok": true, "checks": [{"name": "...", "ok": true, "detail": "..."}]}`. The checks, in order — `home` (the resolved state directory and why: `TANDEM_HOME` override, shipped app, or dev build), `store` (the `todos.json` path, open/done counts, the rolling backup), `bikeshed`, `comments` (`absent` is a valid early state), `lock`, `app` (the installed version, with the quarantine-strip hint), `skills` (whether this skill is found for each agent host). The first four can fail; a missing app or uninstalled host is informational, not failure. Exit `0` healthy, `1` not, `2` usage error.

## Streams and exit codes

- **stdout** — one JSON object on success. `list` → `{"tickets": [...]}`; `add`, `set`, `move`, `complete` → the ticket object; `archive` → `{"archived": <n>}`; `comment add` → the comment; `comment list` → `{"comments": [...]}`; `lane add`/`rename` → `{"lanes": [...]}`, `lane delete` → `{"lanes": [...], "moved": <n>}`. A top-level `note` appears only when the call recovered a corrupt store, naming the backup path.
- **stderr** — short, actionable plain text on failure, naming the failed value and the next step; `complete` with an unknown id appends the current open list in the `list` shape.
- **exit** — `0` success; `1` domain failure (empty title, unknown id, locked store); `2` usage failure (no arguments, unknown subcommand or flag). A missing flag value is deliberately a domain failure, not usage — `add` with no `--title` is the empty title the domain rejects. Retry only when the text says the store is locked.
