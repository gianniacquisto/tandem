# Concepts

This page explains the words the board, the CLI and the MCP tools share. Read it once before the [CLI](cli.md) or [MCP](mcp.md) reference: every verb there acts on one of these things.

## Tickets

A **ticket** is the one kind of work item Tandem keeps. Every ticket has:

| Field | What it is |
| --- | --- |
| `title` | One line. |
| `description` | Optional markdown body. `ticket list` carries only its length, `descriptionLength`; `ticket show` carries the body. |
| `project` | The project it lives in. Set when the ticket is filed and never changed. |
| `label` | An optional grouping inside the project, or `null`. |
| `ref` | Its handle, such as `TAN-42`. See [refs](#refs). |
| `id` | Its UUID. Every verb that takes a ticket accepts the ref or the UUID. |
| `status` | Where it is in the workflow. See [statuses](#statuses). |
| `assignee` | Who is on it: a name, or `null` for unowned. |
| `origin` | Who filed it: `human` or an agent's name. Written once, never changed. |
| `blockedBy`, `blocks`, `blocked` | Its [blocks](#blocks), in both directions. |
| `archived` | Whether it is [archived](#archive). |
| `createdAt`, `completedAt` | When it was filed, and when it reached `done` (cleared if it leaves). ISO 8601, UTC, whole seconds. |

Each ticket also has a **comment log**: an append-only list of comments, each with a time, an author and a body. A comment is never edited or deleted, so a correction is a new comment.

## Projects and labels

A **project** is where tickets live. Every ticket belongs to exactly one, so filing a ticket always names a project (or a label, which implies its project). A project has a name and a **key**, the `TAN` in `TAN-42`. The key is set when the project is added (by default, the first three letters of the name in upper case) and can change only until the project has filed its first ticket.

A **label** is an optional grouping of tickets inside one project, drawn as a row on the board. A ticket has at most one label, and that label belongs to the ticket's own project. Tickets with no label come first. Deleting a label keeps its tickets: they lose the label and stay in their project.

Projects and labels are declared before they are used: `project add`, then `label add`, then tickets that name them.

## Refs

A **ref** is a ticket's short handle: its project's key and a number the project issues in order, `TAN-1`, `TAN-2` and so on. A ref never changes while the ticket stays in its project, wherever it moves between labels. Refs are case-insensitive on input. A ref from before a project's key was merged into another (such as `CUT-34`) still finds its ticket, and the answer shows the current ref.

## Statuses

Every ticket has one of six statuses. They are fixed: a board cannot add or rename them.

| Status | Meaning |
| --- | --- |
| `backlog` | Filed, not yet shaped or scheduled. The default for a new ticket. |
| `on-hold` | Parked: waiting on a decision, or on something outside the board. |
| `ready` | Shaped and ready to pick up. |
| `in-progress` | Someone is working on it. |
| `in-review` | The work is done and waiting for review. |
| `done` | Finished. |

A ticket is **open** until it is `done`. `ticket complete` is the short path to `done`; to reopen a ticket, set its status back.

## Blocks

A **block** records that one ticket waits on another. `ticket block --id TAN-2 --on TAN-1` means TAN-2 waits for TAN-1. The dependent shows the edge in `blockedBy` and the blocker in `blocks`.

`blocked` is worked out on every read, never stored: a ticket is blocked while any ticket it waits on is not `done`. A block lifts the moment its blocker reaches `done`, with no write to the waiting ticket. The edge itself stays as a record, until `ticket unblock` removes it.

A block that could never lift is refused: on the ticket itself, one that would close a cycle, or one on a ticket that is already done or archived. For a wait that is not another ticket, park the ticket `on-hold` and say why in its description.

## Archive

**Archiving** sets a ticket aside: it leaves the default views but stays in the store. `ticket archive` with no ticket archives every done ticket, which is how a list is cleared without deleting anything. `ticket list --archived` lists them. Tandem has no delete for tickets.

## Actors

Every write records who made it, the **actor**. In order:

1. A team member's personal token or session: that member's name.
2. The `X-Tandem-Agent` header on an MCP call, or `TANDEM_AGENT` for a local CLI run.
3. Otherwise, `human`.

An actor name is lowercase letters, digits and hyphens. Where a verb takes `self` (`--assignee self`, `--origin self`), it means the current `TANDEM_AGENT`, and it is refused when that is unset. See [configuration](configuration.md#connecting-an-agent).

## Notes

The **bikeshed** is Tandem's notes folder, `~/.tandem/bikeshed/`: plain markdown files you can open in any editor or keep in git. The board shows and edits them, and agents read and write them through the `note_*` MCP tools.

- A **note** is one `.md` file. Its path is its identity.
- A **wikilink**, `[[name]]`, links one note to another by name, case-insensitively, with an optional `#heading` and `|alias`. A wikilink that matches no note is a **broken link**, and a note no other note links to is an **orphan**. `tandem-cli note links` reports both.
- Moving or renaming a note through Tandem rewrites every wikilink that points at it.

Note writes are atomic. An agent that passes the hash from its last read is refused if the note changed in between, so it never overwrites an edit it has not seen.

## The home

Everything above lives in one folder, the **home** (`~/.tandem` by default): the tickets, projects, labels and comments in one SQLite database, `tandem.db`, and the notes in `bikeshed/`. Every door (the board, the MCP tools, the CLI) reads and writes the same store, so they never disagree, and the board updates live when an agent writes. See [the store](store.md) for backup and restore, and [configuration](configuration.md#the-home) for choosing another home.
