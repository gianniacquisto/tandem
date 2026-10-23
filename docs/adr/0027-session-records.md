# Sessions are records, not tickets: the board holds only work

ADR 0024 made every session file itself as a ticket — `Session — <project> · <session-id>` — and the board refines and moves it like any other ticket. The rule worked: a dead session's ticket became reachable, and the first passes dispatched successors to it. But the convention put a session's row on a work board, and the board now carries sixty-three of them — rows every triage pass must read, refine, dispatch, or close, each one a session's diary wearing a ticket's face. The human's answer to `5C98453A` settles the question the other way: a session is its own artifact — its own file, its own states, never a ticket, never on the board.

## Decision

- **A session is a record, not a ticket.** One record per session, keyed by `$OPENCODE_SESSION_ID`, stored in `sessions.json` (version 1) at TANDEM_HOME — its own file next to `todos.json`, with its own lock (`sessions.lock`). A record is never on the board: it carries no brief for triage to refine, moves through no gates, and `ticket list` never shows it. The app's Sessions tab is its face.
- **Five verbs, two states.** `session start [--project <name>] [--agent <name>] [--session <id>] [--brief <string|->]` opens the record; `session link --ticket <id>` adds a carried ticket; `session end [--session <id>] [--outcome <value>] [--summary <string|->]` closes it; `session list [--project <name>] [--status <value>]` and `session show [--session <id>]` read it. The two states are `running` and `ended`; `ended` carries `endedAt`, an `outcome` of `succeeded | failed | interrupted | unknown`, and a `summary`.
- **The ticket link is stored, the note link is derived.** `link --ticket` appends the ticket id to the record's `tickets` — the link runs from the record to the ticket, so a ticket needs no `session` field. Note captures filed with `note capture --session <id>` land under `projects/<project>/.raw/<session>/`, and `session show` lists those paths as `notes` — the folder is the link; there is no verb for it, and `list` rows omit the field.
- **A session that dies leaves a `running` record and nothing to reclaim.** The board holds only work, so no triage pass reads records looking for stale rows; what a session carried, its record's `tickets` names. Re-`start` on a recorded id returns the record unchanged with a `note` — a resumed session does not double-file.

## Why

- **The board is for work.** Sixty-three session rows crowd every triage pass with rows whose only possible action is "still running". That is the cost ADR 0024 accepted when it made sessions tickets, now measured.
- **The human's answer settles it.** `5C98453A` → **B**: a session is its own artifact. ADR 0024 decided against "a separate ticket kind for session records" — the record is that kind, minus the ticket, and the human's answer arrived fourteen minutes after ADR 0024 merged.
- **Its own file, not a key in the ticket store.** `sessions.json` keeps a session's start and end off `todos.json`'s write path and its lock; a session starting never contends with a ticket write.
- **Stored ticket link, derived note link.** The tickets a session carried are queryable from the record without walking the board; the notes are already organized by session folder on disk, so `show` reads them instead of copying them into the store.

## Decided against

- **A key in `todos.json`** — one file and one lock for both stores would put every session's start and end on the ticket store's write path.
- **One file per session** — sixty-three files and a `list` verb that walks a directory, for a shape the single JSON store already proves.
- **A `session` field on the ticket** — decided against in ADR 0024 and still wrong: a ticket is work, a session is not a kind of ticket, and the link belongs to the record.
- **The board's Status** — reusing `running`/`ended` on tickets would put the record back on the board by another door.
- **Deleting ADR 0024** — the record of a decision that flipped is why ADRs exist; the superseded line carries the flip.

## Consequences

- **The board loses its session rows.** No ticket is ever titled `Session — <project> · <session-id>` again. The documents the convention lives in — `AGENTS.md`, the sprint skill, the CLI skill, the glossary, and ADR 0024's superseded line — change in the same PR, because a split would leave the ADR and a skill disagreeing for however long the second PR waits.
- **A session that dies is no one's pickup on the board.** Stale work is still work — a ticket claimed by a session that died is picked up by the next triage — but the handle is the record (`session show --session <id>`; a record still saying `running` is settled by the host's session list), not a ticket prefix.
- **The sixty-three rows already on the board are S5's** — the human's call (`F1CEFA57`), not this change. This PR governs new sessions; it moves no existing row.
- **The fleet's two documents change in the fleet's own repo** — they cite ADR 0024 by name, so they cannot keep it as the live rule; one author, two PRs, one session.
