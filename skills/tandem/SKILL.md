---
name: tandem
description: "Read and write the shared Tandem ticket list, manage its cycle lanes, and rename bikeshed notes, from `tandem-cli` on this Mac. Reach for it when the user says to add a todo, asks what's on their list, wants a ticket's status or assignee changed, wants a cycle lane created, renamed, or deleted, asks to mark a todo done, or wants a note renamed (reopened under a new name). Also runs a local setup check when the user asks whether Tandem is installed or working. The same tickets a human sees and edits in the Tandem app."
---

# Tandem

`tandem-cli` is Tandem's agent-facing interface on this Mac: the binary the app bundles so an agent can read and write the same shared todo list and notes a human uses in the Tandem app. It is the agent's only interface to the list — there is no server to configure and no handshake.

The binary lives inside the app bundle:

```
/Applications/Tandem.app/Contents/MacOS/tandem-cli
```

That path is stable across upgrades. `tandem-cli --help` prints the syntax and exits `0`.

## The store

`tandem-cli` reads and writes the same file the app does: `~/.tandem/todos.json`, the human's real list. A from-source build (`swift run tandem-cli`) writes `~/.tandem-dev/todos.json` instead, so development can never reach the real list. Notes live under the same home — `~/.tandem/bikeshed/`, or `~/.tandem-dev/bikeshed/` from source.

Every write keeps one previous good copy beside it, in `todos.json.bak` — the file as it was before that write. If a write goes wrong, `mv ~/.tandem/todos.json.bak ~/.tandem/todos.json` is the one-command recovery; the app reloads it on its own.

## Tickets

Six commands under `todo`; each section below is that command's contract. A todo is a **ticket** — the same object a human sees in the Tandem app — and every object these commands return has the same shape:

| field | meaning |
| --- | --- |
| `id` | pass it back to `set`, `move`, `complete` |
| `title` | one line, trimmed |
| `status` | `backlog` (default), `refined`, `ready`, `in-progress`, or `done` |
| `archived` | true when the ticket is set aside — out of the default views, still in the store |
| `description` | markdown body, or null; write it from stdin with `todo set --description -` |
| `assignee` | `human`, an agent's name, or null for unowned |
| `origin` | who filed it — `human` or an agent's name; written at add, never changed |
| `section` | the **cycle lane**, or null for the default area; the wire keeps its v1 name |
| `done` | derived from `status == "done"`; kept on the wire for older readers — read `status`, not this |
| `link` | a bikeshed note path, or null; a human sets it in the app |
| `createdAt`, `completedAt` | ISO-8601 UTC; `completedAt` is null unless done |

`done` is derived and no longer stored, but it stays on the wire for older readers; `status` is the truth. A status change is free-form — any status to any other, backwards included — because the board records what is true, not a process.

### `tandem-cli todo list [--filter open|done|all] [--status <value[,value]>] [--assignee <name|self>] [--lane <name>] [--text <string>]`

List the tickets in view, in the list's stored order (the order the human arranged; new tickets land on top). Every filter is optional, and they combine:

- `--filter` — `open` (default: not done, not archived), `done`, or `all` (everything, the archive included).
- `--status` — one or more comma-separated values, e.g. `--status ready,in-progress`. A repeated flag keeps only its last value, which is why the axis is comma-separated.
- `--assignee` — an exact name, `self` for your own (`$TANDEM_AGENT`), or `""` for the unowned tickets.
- `--lane` — a cycle lane name, or `""` for the default area.
- `--text` — a case- and accent-insensitive substring of the title **or the description**.

An empty result is a success: `{"todos":[]}`.

```
$ tandem-cli todo list --status ready --assignee self
{"todos":[{"archived":false,"assignee":"ui","completedAt":null,"createdAt":"2026-10-09T14:03:22Z","description":"## Why\n…","done":false,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"origin":"ui","section":"Core","status":"ready","title":"Ship the board"}]}
```

### `tandem-cli todo add --title <string> [--status <value>] [--assignee <name|self>] [--lane <name>] [--origin <name|self>]`

Add a ticket to the shared Tandem list (store: ~/.tandem/todos.json), which a human on this Mac shares with you through the Tandem app. The title is required, trimmed, and empty is rejected; duplicate titles are allowed. A title alone lands in `backlog`, unowned, on top of the list's default area. `--assignee self` claims it — `add` never assigns implicitly, because agents routinely file tickets *for* the human (`--assignee human`). A lane must already exist.

`--origin` stamps who filed the ticket, and it is never changed after the add. When you file, pass `--origin self`: an omitted flag means `human`, which is what the app — the human's face — stamps, and a proposal of yours filed under the human's name is exactly the mix-up the field exists to end.

```
$ tandem-cli todo add --title "Water the plants" --assignee self --origin self
{"archived":false,"assignee":"ui","completedAt":null,"createdAt":"2026-10-17T09:12:00Z","description":null,"done":false,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"origin":"ui","section":null,"status":"backlog","title":"Water the plants"}
```

### `tandem-cli todo set --id <string> [--title …] [--status …] [--assignee …] [--description …]`

Change the fields you name in **one write**; everything you do not name is left alone. `--description -` reads the body from stdin, which is how multi-line markdown lands intact:

```
$ printf '## Why\n\nA long body.\n' | tandem-cli todo set --id 550e8400-… --status refined --description -
```

`--assignee ""` clears the owner, `--description ""` clears the body, and `--assignee human` hands the ticket to the human for a decision. Moving to `done` stamps `completedAt`; moving out of it clears the stamp. An empty `--title` is rejected.

### `tandem-cli todo move --id <string> [--lane <name>] [--index <number>]`

Move a ticket between cycle lanes and/or within one. `--index` is 0-based inside the target lane; omit it to append, which leaves the order above it untouched. `--lane ""` moves it to the default area; the lane must already exist.

### `tandem-cli todo complete --id <string>`

The short path to `--status done`: idempotent — completing an already-done ticket returns it unchanged — and an unknown id's error carries the current open list so you can pick the right one. There is no `reopen` verb: `todo set --id … --status backlog` is the reopen.

```
$ tandem-cli todo complete --id 550e8400-e29b-41d4-a716-446655440000
{"archived":false,"assignee":null,"completedAt":"2026-10-17T09:14:37Z","createdAt":"2026-10-09T14:03:22Z","description":null,"done":true,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"origin":"ui","section":null,"status":"done","title":"Water the plants"}
```

### `tandem-cli todo archive`

Archive every done ticket in one call: nothing is deleted, the tickets leave the default views (and `--filter all` still shows them). Returns how many it archived; a second call returns 0.

```
$ tandem-cli todo archive
{"archived":3}
```

## Lanes

A **cycle lane** is the area axis of the ticket model — the list groups by it and the board draws its rows; `--lane ""` (or a null `section`) is the default area. Three verbs manage them, and they are the only CLI verbs that change the lane list itself:

| command | returns |
| --- | --- |
| `tandem-cli lane add --name <name>` | `{"lanes": [...]}` |
| `tandem-cli lane rename --from <name> --to <name>` | `{"lanes": [...]}` |
| `tandem-cli lane delete --name <name>` | `{"lanes": [...], "moved": <n>}` |

`lanes` is the lane list after the write, in render order, so no follow-up call is needed to learn the names. Names are trimmed; an empty name, a name already in use, and (for rename/delete) an unknown name are domain failures with actionable text. `rename` rewrites the lane list and every ticket in it in one write; renaming to the same name is a no-op. `delete` never deletes tickets — its members fall back to the default area, and `moved` is how many did.

```
$ tandem-cli lane add --name Tandem
{"lanes":["Tandem"]}
$ tandem-cli lane delete --name Tandem
{"lanes":[],"moved":2}
```

A lane must exist before `todo add --lane` / `todo move --lane` will accept it, which is what these verbs are for. `lane list` does not exist: `todo list --filter all` shows every lane that has a ticket in its `section`; a lane with no tickets is visible only in the app.

## Notes

The bikeshed (`~/.tandem/bikeshed/`) is a plain folder of `.md` files, and the file itself is the interface. Create, edit, and delete curated notes with your own file tools; the CLI writes no note content. Three CLI verbs act on notes: `rename`, the one rename path; `capture`, which files a capture; and `stats`, the read-only memory-health readout.

### `tandem-cli note rename --from <path> --to <path>`

Rename a bikeshed note (both paths bikeshed-root-relative, `.md` included):

```
$ tandem-cli note rename --from "Ideas.md" --to "Backlog.md"
{"path":"Backlog.md"}
```

It rewrites every wikilink that resolves to the note — bare, qualified, aliased, and `#fragment` forms, in every note including itself — in one locked transaction. Rename through the CLI only: a raw `mv` moves the file but leaves every wikilink pointing at the old name silently broken, and nothing repairs them but re-running the rename through the CLI.

### `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> [--project <name>] [--session <id>]`

File one **capture** — a small, create-only record of a moment that would be lost when the session ends — into the project's hidden saddlebag. The body is read from **stdin** (all of it, UTF-8; empty is allowed):

```
$ printf '# Caret not placed\n\nThe editor opened without focus.\n' | tandem-cli note capture --kind gotcha
{"path":"projects/tandem/.raw/ses_eafd…/20261018T140211Z-gotcha-caret-not-placed.md"}
```

- `--kind` (required): one of `decision`, `gotcha`, `fact`, `work`, `thread` — the curated section the consolidation pass folds it into.
- `--project` (optional): the project folder; defaults to the working directory's last path component (`…/projects/tandem` → `tandem`).
- `--session` (optional): defaults to `$OPENCODE_SESSION_ID`, else a short generated token.

The note lands at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`, where `<utc>` is a UTC timestamp, `<kind>` is the kind, and `<slug>` comes from the body's first line; a same-second, same-slug note gets a `-2`, `-3`, … suffix rather than overwriting. The saddlebag is **hidden**: captures never appear in the tree, search, backlinks, or wikilink resolution, and they should contain no `[[wikilinks]]` — file captures as you work, and let the consolidation pass promote them into the curated notes. An unknown `--kind`, or a `--project`/`--session` that is not one folder name, is a domain failure (exit `1`).

A note filename may not contain `/`, `#`, `|`, or a leading dot, and must end `.md`. `#` and `|` are the wikilink grammar's separators, so a note whose name contains either can never be resolved by a wikilink (ADR 0004). Pre-existing violators are still shown and edited — the rule constrains what is *created*.

### `tandem-cli note stats [--project <name>]`

Report whether memory is draining: one JSON object, read-only and lock-free (it writes nothing and takes no lock). It is the one command that deliberately looks *inside* the hidden saddlebag.

```json
{"bikeshed":"/Users/you/.tandem/bikeshed","projects":[{"curatedNotes":6,"lastConsolidated":"2026-10-18T21:10:00Z","oldestPending":null,"project":"tandem","rawArchived":15,"rawPending":0}],"totals":{"curatedNotes":6,"rawArchived":15,"rawPending":0}}
```

- `bikeshed` — the resolved bikeshed root, absolute.
- `projects` — one row per project folder, alphabetical; `--project <name>` narrows it to one folder (a name that matches none is an empty list, not an error).
- `curatedNotes` — `.md` notes under `projects/<project>/`, the hidden saddlebag pruned.
- `rawPending` / `rawArchived` — captures pending under `projects/<project>/.raw/`, and consumed ones under `projects/<project>/.raw/.done/`.
- `oldestPending` — the oldest pending capture's UTC time (the queue's age), or `null` when nothing is pending.
- `lastConsolidated` — the newest curated-note mtime, or `null` when the project has no curated notes.
- `totals` — the sums across every reported project.

A pending count that stays high, or an `oldestPending` far behind, is the queue not draining; a `lastConsolidated` that stopped moving is the curated layer going stale. Both are signals to hand the saddlebag to a consolidation pass.

## Setup check

`tandem-cli doctor` reports where this machine's Tandem state lives and whether it is healthy — the first-run check for a human or an agent setting Tandem up. It is a setup/diagnostic verb, not part of the `todo` / `note` domains, and takes no arguments.

Returns one JSON object: `{"ok": true, "checks": [{"name": "...", "ok": true, "detail": "..."}]}`. The checks, in this order:

| name | what it reports | can fail |
| --- | --- | --- |
| `home` | the resolved state directory (see The store), and why — `TANDEM_HOME` override, shipped app, or dev build | no |
| `store` | the `todos.json` path, its open/done counts (`absent` = a valid empty store), and the rolling backup beside it — `backup 5m old`, or `no backup yet` until the second write | yes — present but unreadable or corrupt |
| `bikeshed` | the bikeshed path and its note count | yes — the path is a file, not a folder |
| `lock` | whether a writer currently holds the store lock | yes — held by another process |
| `app` | the installed `/Applications/Tandem.app` version, with the quarantine-strip hint when it is flagged | no |
| `skills` | whether this skill is found for each agent host, and whether opencode's config lists the repo `skills/` dir | no |

`ok` is true when no failing-capable check failed. Exit `0` when `ok` is true and `1` when it is false (each failing check's next step is echoed to stderr); `2` on a usage error. A missing app or an uninstalled agent host are informational, not failures.

```
$ tandem-cli doctor
{"checks":[{"detail":"/Users/you/.tandem-dev (dev build)","name":"home","ok":true},{"detail":"/Users/you/.tandem-dev/todos.json — 3 open, 2 done; backup 5m old","name":"store","ok":true}, …],"ok":true}
```

## Streams and exit codes

- **stdout** — one JSON object on success. `list` returns `{"todos": [...]}`; `add`, `set`, `move` and `complete` return the todo object; `archive` returns `{"archived": <n>}`; `lane add` and `lane rename` return `{"lanes": [...]}`, `lane delete` returns `{"lanes": [...], "moved": <n>}`. A top-level `note` appears only when the call recovered a corrupt store, naming the backup path.
- **stderr** — short, actionable plain text on failure, naming the failed value and the next step. `complete` with an unknown id appends the current open list in the `list` shape:

  ```
  $ tandem-cli todo complete --id does-not-exist
  Unknown todo id "does-not-exist". Here is the current open list to pick the right one from:
  {"todos":[…]}
  ```

- **exit** — `0` success; `1` domain failure (the shared list rejected the call — an empty title, an unknown id, a locked store); `2` usage failure: no arguments, an unknown subcommand, or an unknown flag. A *missing flag value* is deliberately not a usage failure — `todo add` with no `--title` is the empty title the domain rejects, exit `1`. Retry only when the text says the store is locked.
