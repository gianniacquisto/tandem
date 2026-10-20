# Spec: the human's inbox — where an agent asks a person

Type: spec
Status: implemented
Source: `.scratch/board/map.md` (ticket 5), ADR [0017](../../docs/adr/0017-the-ticket-model.md), `GLOSSARY.md` (Assignee, Inbox)

## Problem Statement

Assignment exists so an agent can hand a decision to a person: file a ticket, assign it to `human`, put the question in the description. Nothing in the app answered the other half of that — *what is waiting for me?* The list shows every ticket, the board shows every stage, and a ticket assigned to `human` looked exactly like one assigned to `core`.

## The Decisions

1. **A fourth tab, not a fifth filter segment.** `Todos | Board | Inbox | Bikeshed`. "What needs me" is a different question from "what is in the list", and it has to be answerable from anywhere — so it gets a face of its own rather than a narrower view of another. A filter segment would also have made the segmented control five wide, squeezing a 540 pt column that already holds two buttons.
   *Reversal:* remove the tab; the query stays.

2. **The inbox is a query in the core, not a flag or a second store.** `TodoQuery.inbox` = open Statuses × `assignee: human` × not archived. Defined beside the other views, so the app, a future notification, and an agent asking the same question get the same answer. `TodoQuery.human` becomes the one spelling of the reserved assignee (normalization and the UI use it too).
   *Reversal:* free — it is a constant value.

3. **A dot on the tab, no other notification.** The tab carries a small ember dot while the inbox is non-empty, visible from every face. No dock badge, no menu-bar badge, no sound: the app's register is quiet, and a badge on the chrome is the smallest thing that can be noticed.
   *Reversal:* delete the dot.

4. **The list's own rows.** The inbox draws `ticketRow` — the exact row the list draws, with every action: complete, reopen, rename inline, move lane, link a note, edit the description, delete. A ticket does not become read-only by being a question. The question itself is the description, read in the description sheet (map ticket 6), and the row already previews its first lines.

5. **Stored order, flat — a queue, not a board.** No lane headers and no Status grouping: the inbox is as long as the number of things waiting, and the order tickets were filed is the order to take them. Sorting and grouping would be a view decision with no ask behind it.

6. **No search field on the inbox.** It is short by construction (a person's queue, not the store). The list and the board already carry the query's text search when a body needs finding.

## Testing

- **The core:** `QueryTests` — the inbox is open, non-archived, `human`-assigned tickets, empty when nothing is assigned (not an error). 263 tests pass.
- **The view:** by hand (no GUI test target, ADR 0009): assign a ticket to `human` from the CLI, confirm the tab dot, the row, the description sheet, and that completing or archiving it empties the inbox.

## Out of Scope

- **Assigning from the GUI** — assignment is an agent's instruction (`--assignee`), not a human's form field, for now.
- **Notifications** (dock badge, menu bar, system notification).
- **An inbox in the menu-bar popover** — a natural next glance, not this ticket.
- **Marking an inbox ticket "seen"** without completing it — there is no read state, and adding one is a schema question.
