---
name: tandem-memory
description: Orient from Tandem's notes and board at the start of a session, before reading or editing project files. Reach for it to find prior decisions or to record a fact that would be costly to re-derive.
---

# Tandem memory

The notes store is `~/.tandem/bikeshed/`: markdown notes linked with `[[wikilinks]]`, shared by the human and agents. The board holds the open work. The ticket comment log is the work record (ADR 0045); there are no session records, captures or consolidation passes.

## Orient

1. `ticket_list` for the open work on the project.
2. Read `Index.md` only when the task needs background, then the one note it points to. Follow a wikilink only for what the task touches.
3. Trust the code and the board over a note: a note is a hint with a date, and may predate the work.

Done when you know the open threads and have read nothing the task does not need. No matching note means no prior memory: proceed.

## Record

Write with `note_write` only when a decision, gotcha or fact would cost real effort to re-derive and is not already in a ticket, an ADR or the code. Edit the existing note in place rather than adding one beside it. Never copy board state into a note.

The work itself is recorded where ADR 0045 puts it: the handoff comment on each ticket you carried.
