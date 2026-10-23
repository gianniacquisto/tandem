# Issue tracker: the Tandem board

The tracker for this project is the **Tandem board** — the shared list in `~/.tandem/todos.json`, read and written through `tandem-cli`. There are no GitHub issues, no labels, and no blocking edges. Everything below is how a skill that expects a tracker speaks to this one.

## The ticket

One ticket, one subject, one piece of work. It is a row on the board, not an issue on a remote.

```sh
tandem-cli ticket add --title "<title>" --status <status> --lane "<project>"
tandem-cli ticket list --filter all
tandem-cli ticket list --filter all --text "<words from the title or description>"
```

- **A ticket is identified by its id**, and the board is read by title and description, not by id: `--text` matches the title and the description. A bare id finds nothing.
- **The lane is the project.** `--lane "<project>"` puts a ticket in a project's lane; `--lane ""` is the default area, and it always comes first. A lane must exist before a ticket can name it.
- **The description is the brief**, and it is written once and appended to, never overwritten. It opens as the brief and gains the outcome at the end, under a heading of its own. The refiner owns the shape of a brief: `agents/refiner.md`.

## Statuses

`backlog` → `refined` → `ready` → `in-progress` → `in-review` → `done`. The gates are the board skill's; a ticket that skips `refined` arrives at an agent with a title and no decision.

## The triage vocabulary, on this tracker

The canonical triage roles are statuses and assignees here, not labels:

| role | on this board |
| --- | --- |
| `needs-triage` | `--status backlog`, unassigned |
| `needs-info` | `--status backlog`, the open questions in the description |
| `ready-for-agent` | `--status ready` |
| `ready-for-human` | `--status refined --assignee human` |
| `wontfix` | `--status done`, the reason under **Decided against**; `tandem-cli ticket archive` sweeps the done pool |

There is no label vocabulary to set up, and no label to create: a ticket's state is its status and its assignee, and both are set with `tandem-cli ticket set`.

## Conversation

Comments, not issue comments:

```sh
tandem-cli ticket comment add --id <id> --body -
tandem-cli ticket comment list --id <id>
```

A comment is how a question is put to a human inside a ticket; a ticket's description is how a brief is written. An empty body is a domain failure, not an empty comment.

## Closing work

Work closes when the evidence exists: a merged PR, a changed file, or a recorded answer.

```sh
tandem-cli ticket complete --id <id>
```

A ticket moves to `in-review` when its PR is open, green, and waiting for a reader. The review gate is the `reviewer` agent in this repo, not a review skill.

## Domain docs

The project's vocabulary is `GLOSSARY.md` at the root, and its decisions are the ADRs in `docs/adr/`. Use the glossary's words in a spec, and respect the ADRs in the area the work touches.

## Wayfinding operations

**Not supported yet.** The board has no labels, no blocking edges and no child tickets, so a map of decision tickets cannot be expressed on it. The `wayfinder` skill is deferred until the board can carry a map; the skills that need one are not vendored here.
