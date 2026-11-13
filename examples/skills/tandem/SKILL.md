---
name: tandem
description: "Read and write the shared Tandem board and its notes from `tandem-cli` on this Mac. Reach for it when the user says to add a ticket, asks what is on the board, wants a status, assignee, lane or project changed, or wants a note created, renamed or moved. Also runs the local health check when the user asks whether Tandem is installed or working."
---

# Tandem

The tickets and notes a human sees in the browser and the ones an agent writes are the same store. `tandem serve` draws the board; `tandem-cli mcp` serves the agent face. Both run the same binary.

## MCP tools

`tandem-cli mcp` serves the agent face over stdio. Each tool is one CLI verb's own domain call, so the two doors cannot disagree. Agents use the tools only: when they are missing, say so and ask the human to configure the server, never fall back to the shell for a write.

Point the host at the binary once:

```json
{"mcp": {"tandem": {"type": "local", "command": ["tandem-cli", "mcp"]}}}
```

There is no install verb and no bundled face. The actor is the pass's own (`TANDEM_AGENT`, with the same `self` expansion the CLI uses), and the home resolves from `TANDEM_HOME`, else `~/.tandem`. `initialize` carries a small health block (home, store ok, version, tool count); a full `doctor` never runs per call.

## The store

`~/.tandem/tandem.db` is the store: one SQLite database holding `tickets`, `ticket_dependencies`, `comments`, `lanes`, `projects`, `store_meta` and `schema_migrations`. Notes live beside it under `~/.tandem/bikeshed/`. Every changed write keeps one previous complete copy in `tandem.db.bak`; recovery is the backup and two commands, journal files first:

```sh
rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
```

A store from before v0.27.0 is a JSON file: it is read as the fallback and imported once on the domain's first write, and a copy put back beside the database afterwards is ignored.

## Tickets

The Statuses are `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done`; `on-hold` is the parked slot. A ticket in a lane carries a **ref**, the lane's key and a per-lane number (`TAN-42`), and every `--id` accepts it beside the UUID.

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
```

`list` rows are summaries without the description; `show` carries the whole ticket and its dependency state (`blocked`, `blockedBy`, `blockedReason`, `blocks`). A block is derived from the edge and lifts when the blocker reaches `done`; the reason is the ticket's `## Blocked on` section, read not copied.

The comment log is append-only: no edit, no delete, a correction is a new comment. A pass narrates its handoff there, with **what landed**, **outcome and next status**, and **issues**, signed with its own agent name. The description is the brief: written once and appended to, never overwritten.

## Lanes and projects

```sh
tandem-cli lane list
tandem-cli lane add --name <name> [--abbr <key>] [--project <name>]
tandem-cli lane rename --from <name> --to <name> [--abbr <key>] [--project <name>]
tandem-cli lane delete --name <name>
tandem-cli lane move --name <name> [--index <number>]
tandem-cli project list
tandem-cli project add --name <name>
tandem-cli project rename --from <name> --to <name>
tandem-cli project delete --name <name>
```

A lane is a ticket's row on the board. Every lane belongs to one project; a lane with no parent creates or reuses a project named for it. A lane's key is frozen once its tickets carry numbers. Deleting a lane moves its tickets to the unnamed default area, never deletes them.

## Notes

```sh
tandem-cli note create --path <path>
tandem-cli note create-folder --path <path>
tandem-cli note write --path <path> [--text <string|->]
tandem-cli note move --path <path> --into <folder>
tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
```

A note is a markdown file in the bikeshed; its path is its identity. `create` and `create-folder` refuse a taken target. `write` replaces the whole text atomically. `move` and `rename` rewrite every wikilink that resolves to a moved note in one locked transaction. `stats` reports each project's curated-note count and newest note mtime, and `links` reports broken links and orphans. The capture layer and its hidden `.raw/` subtree are gone (ADR 0045); the notes store keeps notes and wikilinks, nothing else.

## Doctor

```sh
tandem-cli doctor
```

One JSON object, `{"ok": bool, "checks": [{"name", "ok", "detail"}]}`, with three checks in order: `home` (the resolved path and why), `store` (path, schema version, counts and the backup), and `bikeshed` (path and note count, or the file that is not a folder). `store` and `bikeshed` can fail; `home` is informational. Exit `0` healthy, `1` not, `2` usage error.

## Serve and mcp

`tandem serve` binds `127.0.0.1:7700` by default and draws the board and notes as HTML; `--host`, `--port`, `--open` and `--allow-unauthenticated` are its flags. A non-loopback host is refused unless `--allow-unauthenticated` is named. `tandem mcp` is the same MCP server over stdio. Neither is listed among the agent verbs in the root usage, and each has its own `--help`.

## Streams and exit codes

Every command prints one JSON object on stdout and a short actionable message on stderr. `list` answers `{"tickets": [...]}`; `show`, `add`, `set`, `move` and `complete` answer the ticket object; `archive` answers `{"archived": <n>}`; `comment add` answers the comment and `comment list` answers `{"comments": [...]}`; `lane list`/`add`/`rename` answer `{"lanes": [...]}` and `lane delete` answers `{"lanes": [...], "moved": <n>}`; `note stats` answers the per-project report. Exit `0` success, `1` a domain failure, `2` a usage error.
