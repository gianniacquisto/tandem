# Versions and compatibility

## Version numbers

Releases are numbered `major.minor.patch`, such as `2.7.1`. `tandem --version` prints the one you run. Every release is published on [GitHub Releases](https://github.com/gianniacquisto/tandem/releases), and its notes are the changelog: the pull requests it contains, grouped as breaking changes, features, fixes, and docs and infra.

Only the latest release is supported. A fix ships in a new release, not as a patch to an older one, so upgrade to get it ([installing and upgrading](install-paths.md)).

## What may change, and how you hear about it

Tandem moves quickly, and different surfaces carry different promises. Whatever the surface, a change that renames or removes something, or changes what it means, is listed under **Breaking changes** at the top of the release notes.

| Surface | Promise |
| --- | --- |
| **MCP tools**: names, parameters, answers | None beyond the schema. An agent reads the tools and their schemas each time it connects, so it sees the current ones. Tools, parameters and answer fields may be added, renamed or removed in any release. |
| **Agent examples**: the skills, agent prompts and Claude Code plugin in [`examples/`](../examples/README.md) | None. They change whenever the workflow does. The plugin is pinned to a release, so it moves with the binary. |
| **CLI**: verbs, flags, JSON output, exit codes | Additions at any time: new verbs, new flags, new fields in the JSON. Anything removed or changed in meaning is **deprecated first**: it keeps working for at least one release, with a warning on stderr, then goes. A script that parses the output should ignore fields it does not know. |
| **Environment variables** | As for the CLI. |
| **HTTP**: `/health`, `/mcp`, `/events`, `/export` | As for the CLI: additions at any time, removals deprecated first. The MCP tools behind `/mcp` follow the MCP row. |
| **Operator API**, `/operator/v1` | Versioned by its path. A change that breaks a shape or a status code becomes `/operator/v2`. |
| **The board's own pages and routes** | None. They are the board's internals; script against MCP or the CLI instead. |
| **Error messages** | None. They are written for people and agents to read, not to match. Match on exit codes, HTTP statuses and JSON-RPC error codes. |

## The store

The store's schema only moves forward. A newer binary migrates the store on its first write, keeping the copy from before the migration as the backup, and an older binary then refuses to open it. So an upgrade is one-way unless you restore the backup: see [the store](store.md#upgrading) before downgrading.

An archive made by [`tandem-cli export`](cli.md#export-and-import) can be imported by the same release or a later one.

## Deprecated now

| What | Use instead | Deprecated in |
| --- | --- | --- |
| `tandem-cli todo ...` and the `todos` key in `todo list`'s answer | `tandem-cli ticket ...` and `tickets` | 1.0 ([ADR 0030](adr/0030-one-entity-the-ticket.md)) |

## Known issues

- **`ticket move` without a label removes the ticket's label.** Through the CLI and the `ticket_move` MCP tool, leaving out the label moves the ticket to no label, even when you only meant to reorder it. Name the current label with the new position. See [ticket move](cli.md#ticket-move).
