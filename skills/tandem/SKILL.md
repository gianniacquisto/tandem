---
name: tandem
description: "Read and write the shared Tandem todo list, and rename bikeshed notes, from `tandem-cli` on this Mac. Reach for it when the user says to add a todo, asks what's on their list, asks to mark a todo done, or wants a note renamed (reopened under a new name). Also runs a local setup check when the user asks whether Tandem is installed or working. The same todos a human sees and edits in the Tandem app."
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

## Todos

Three commands under `todo`; each section below is that command's contract. Every todo object they return carries `link` — a path to a bikeshed note, or null. A human sets it in the app; no CLI verb writes it.

### `tandem-cli todo list [--filter open|done|all]`

List todos from the shared Tandem list — the same todos a human sees and edits in the Tandem app on this Mac. Store: ~/.tandem/todos.json. Optional argument `filter`: `open` (default), `done`, or `all`. Returns todos in the list's stored order (the order the human arranged; new todos land on top) as JSON: `id`, `title`, `done`, `section` (null for the default area), `link` (a bikeshed note path, or null), `createdAt`, `completedAt` (ISO-8601 UTC; `completedAt` is null while open).

```
$ tandem-cli todo list --filter open
{"todos":[{"completedAt":null,"createdAt":"2026-10-09T14:03:22Z","done":false,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"section":null,"title":"Water the plants"}]}
```

### `tandem-cli todo add --title <string>`

Add a todo to the shared Tandem list (store: ~/.tandem/todos.json), which a human on this Mac shares with you through the Tandem app. Argument: `title` (required; trimmed, empty is rejected). Duplicate titles are allowed. The new todo lands on top of the list's default area. Returns the created todo as JSON, including the `id` you can pass to `complete`.

```
$ tandem-cli todo add --title "Water the plants"
{"completedAt":null,"createdAt":"2026-10-17T09:12:00Z","done":false,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"section":null,"title":"Water the plants"}
```

### `tandem-cli todo complete --id <string>`

Mark a todo as done on the shared Tandem list (store: ~/.tandem/todos.json). Argument: `id` (from `list` or `add`). Idempotent: completing an already-done todo returns it unchanged. If the id is unknown, the error includes the current open list so you can pick the right one. There is no reopen (a human reopens a done todo in the app).

```
$ tandem-cli todo complete --id 550e8400-e29b-41d4-a716-446655440000
{"completedAt":"2026-10-17T09:14:37Z","createdAt":"2026-10-09T14:03:22Z","done":true,"id":"550e8400-e29b-41d4-a716-446655440000","link":null,"section":null,"title":"Water the plants"}
```

## Notes

The bikeshed (`~/.tandem/bikeshed/`) is a plain folder of `.md` files, and the file itself is the interface. Create, edit, and delete notes with your own file tools; the CLI writes no note content. `note rename` is the one note verb, and the one rename path:

```
$ tandem-cli note rename --from "Ideas.md" --to "Backlog.md"
{"path":"Backlog.md"}
```

It rewrites every wikilink that resolves to the note — bare, qualified, aliased, and `#fragment` forms, in every note including itself — in one locked transaction. Rename through the CLI only: a raw `mv` moves the file but leaves every wikilink pointing at the old name silently broken, and nothing repairs them but re-running the rename through the CLI.

A note filename may not contain `/`, `#`, `|`, or a leading dot, and must end `.md`. `#` and `|` are the wikilink grammar's separators, so a note whose name contains either can never be resolved by a wikilink (ADR 0004). Pre-existing violators are still shown and edited — the rule constrains what is *created*.

## Setup check

`tandem-cli doctor` reports where this machine's Tandem state lives and whether it is healthy — the first-run check for a human or an agent setting Tandem up. It is a setup/diagnostic verb, not part of the `todo` / `note` domains, and takes no arguments.

Returns one JSON object: `{"ok": true, "checks": [{"name": "...", "ok": true, "detail": "..."}]}`. The checks, in this order:

| name | what it reports | can fail |
| --- | --- | --- |
| `home` | the resolved state directory (see The store), and why — `TANDEM_HOME` override, shipped app, or dev build | no |
| `store` | the `todos.json` path and its open/done counts (`absent` = a valid empty store) | yes — present but unreadable or corrupt |
| `bikeshed` | the bikeshed path and its note count | yes — the path is a file, not a folder |
| `lock` | whether a writer currently holds the store lock | yes — held by another process |
| `app` | the installed `/Applications/Tandem.app` version, with the quarantine-strip hint when it is flagged | no |
| `skills` | whether this skill is found for each agent host, and whether opencode's config lists the repo `skills/` dir | no |

`ok` is true when no failing-capable check failed. Exit `0` when `ok` is true and `1` when it is false (each failing check's next step is echoed to stderr); `2` on a usage error. A missing app or an uninstalled agent host are informational, not failures.

```
$ tandem-cli doctor
{"checks":[{"detail":"/Users/you/.tandem-dev (dev build)","name":"home","ok":true},{"detail":"/Users/you/.tandem-dev/todos.json — 3 open, 2 done","name":"store","ok":true}, …],"ok":true}
```

## Streams and exit codes

- **stdout** — one JSON object on success. `list` returns `{"todos": [...]}`; `add` and `complete` return the todo object. A top-level `note` appears only when the call recovered a corrupt store, naming the backup path.
- **stderr** — short, actionable plain text on failure, naming the failed value and the next step. `complete` with an unknown id appends the current open list in the `list` shape:

  ```
  $ tandem-cli todo complete --id does-not-exist
  Unknown todo id "does-not-exist". Here is the current open list to pick the right one from:
  {"todos":[…]}
  ```

- **exit** — `0` success; `1` domain failure (the shared list rejected the call — an empty title, an unknown id, a locked store); `2` usage failure: no arguments, an unknown subcommand, or an unknown flag. A *missing flag value* is deliberately not a usage failure — `todo add` with no `--title` is the empty title the domain rejects, exit `1`. Retry only when the text says the store is locked.
