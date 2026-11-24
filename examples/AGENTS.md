# AGENTS.md

A starter for your own global agent instructions. Copy it, edit it, drop what you do not use. It assumes the Tandem MCP server is configured (see [README.md](README.md)).

## Memory

Memory is the notes (in `~/.tandem/notes/`, markdown with `[[wikilinks]]`) and the board. The ticket comment log is the work record: no session records, captures or consolidation pass exist.

- **Orient**: `ticket_list` for the open work, and `Index.md` only when the task needs background. Read a note only for the part the task touches: find it with `note_search` (and `note_backlinks` for what links to it), and change one passage with `note_edit` (`oldText` must match once; an empty `oldText` appends) instead of rewriting the note. A note is a hint: where it disagrees with the code or the board, the code and the board win.
- **Record**: when a decision settles or something surprising turns up in the work, dispatch the `scribe` agent as `skills/tandem-scribe/` says, with what happened, the ticket if there is one, and the files or pages involved. The scribe decides whether it is worth keeping and where it goes: the project's own docs, a note, or nowhere ([ADR 0056](https://github.com/gianniacquisto/tandem/blob/main/docs/adr/0056-project-docs-and-notes.md)). Do not write the note yourself; where your host cannot dispatch an agent, follow `agents/scribe.md` yourself. Never copy board state into a note.

## Tickets

Agents call Tandem through the MCP tools, never through the shell.

- **Statuses**: `backlog`, `on-hold` (parked), `ready`, `in-progress`, `in-review`, `done`. `human` is an assignee only while a question is open.
- **Answers**: ticket tools answer the whole shape by default. Pass `detail: "lean"` for a short receipt on a write (no description), no body on `ticket_comment_add`, and `ticket_list` rows without empty fields; `limit` cuts the list and `total` reports the matches (`limit: 0` returns no rows and the total).
- **Description**: written once as the brief. Append with `ticket_set`'s `appendDescription`; `description` **replaces** the body.
- **Comments**: the log is append-only, so a correction is a new comment. Sign with your role written out as `author`.
- **Blocks**: `ticket_block` writes the edge. The block is derived and lifts when the blocker reaches `done`; the reason goes in a `## Blocked on` section of the description.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee, in one `ticket_set`. Before it moves a status, it appends one comment on every ticket it carried, with three short fields: **what landed**, **outcome and next status**, **issues** (or none). The actor that closes a hold records the decision in a comment and moves status and assignee in the same `ticket_set`; the inbox is the assignee alone.

## Naming tickets

A ticket named outside its own page carries its ref, its title, and one clause on what it asks or the state it is in. The ref alone is a pointer, never a presentation. In a table, a row may carry the title in its own cell. This rule is the only place the wording lives: the passes that report tickets point here.

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop or a hyphen. This covers PR bodies, ticket text, notes, commit messages and code comments.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary; when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.
