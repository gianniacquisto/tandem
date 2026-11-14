# AGENTS.md

A starter for your own global agent instructions. Copy it, edit it, drop what you do not use. It assumes the Tandem MCP server is configured (see [README.md](README.md)).

## Memory

Memory is the notes store (`~/.tandem/bikeshed/`, markdown with `[[wikilinks]]`) and the board. The ticket comment log is the work record: no session records, captures or consolidation pass exist.

- **Orient**: `ticket_list` for the open work, and `Index.md` only when the task needs background. Read a note only for the part the task touches. A note is a hint: where it disagrees with the code or the board, the code and the board win.
- **Record**: write a note with `note_write` only for a decision, gotcha or fact that would cost real effort to re-derive and is not already in a ticket, an ADR or the code. Edit the existing note in place; do not add one beside it. Never copy board state into a note.

## Tickets

Agents call Tandem through the MCP tools, never through the shell.

- **Statuses**: `backlog`, `on-hold` (parked), `ready`, `in-progress`, `in-review`, `done`. `human` is an assignee only while a question is open.
- **Description**: written once as the brief. `ticket_set`'s `description` **replaces** the body and there is no append parameter, so append by reading with `ticket_show` and passing the combined text after a blank line.
- **Comments**: the log is append-only, so a correction is a new comment. Sign with your role written out as `author`.
- **Blocks**: `ticket_block` writes the edge. The block is derived and lifts when the blocker reaches `done`; the reason goes in a `## Blocked on` section of the description.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee, in one `ticket_set`. Before it moves a status, it appends one comment on every ticket it carried, with three short fields: **what landed**, **outcome and next status**, **issues** (or none).

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop or a hyphen. This covers PR bodies, ticket text, notes, commit messages and code comments.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary; when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.
