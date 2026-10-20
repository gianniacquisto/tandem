# Spec: the board — Status columns, Cycle-lane rows

Type: spec
Status: implemented
Source: `.scratch/board/map.md` (ticket 4) + the decisions of 2026-10-20, ADR [0017](../../docs/adr/0017-the-ticket-model.md), `GLOSSARY.md`

Covers map ticket **4** (the board view), which absorbed the original kanban todo (7ED2E8DE). The decisions below were taken without a grilling round: each is reversible by deleting code, and the irreversible part (the store's shape) was already settled in ADR 0017.

## Problem Statement

The ticket model landed with no face. A human could set a Status only by running `tandem-cli`, and the app drew tickets in a flat list that showed done and not-done. The five Statuses — the reason the model exists — were invisible, so the workflow the agents use could not be read at a glance, let alone changed without a terminal.

## The Decisions

1. **The board is a third tab, beside the list.** `Todos | Board | Bikeshed`. The list keeps what it is good at — quick capture, inline rename, lane chrome, the shipped Open/Done/All filter — and the board is the full-width view where Status is visible. It does not replace the list's filter control (the map's open question): a board shows every stage at once by construction, so a filter over stages would be a second way to say the same thing.
   *Reversal:* remove the tab; the model does not care.

2. **Columns are the five Statuses in workflow order, fixed.** Backlog, Refined, Ready, In progress, Done — `Status.workflow`, so the column order is the enum's order; adding a Status would add a column with no view code. User-defined columns stay out (ADR 0017).
   *Reversal:* cheap for layout, expensive for the domain — which is why the column set is the enum and not a view constant.

3. **Rows are the Cycle lanes, in the list's order, and empty lanes draw.** The default area first (labelled "Uncategorized", as the list's Move-to menu says), then declared lanes, then any lane only a ticket names (a hand edit). An empty lane still draws a row because its cells are drop targets — the row is where a ticket *can* go. The grouping is `TodoBoard.rows` in the core, and the list's lane order is `TodoBoard.orderedKeys`, so the two faces cannot disagree about what a lane holds.
   *Reversal:* free.

4. **A drop is one write: `Store.place(id, lane:status:at:)`.** Dragging a ticket onto a cell sets the lane and the Status together; `move` (order) and `set` (fields) each write, so half-landing a drag would take two writes and could fail between them. Dropping a ticket onto another ticket reorders inside the cell, using the list's own `TodoOrdering.dropIndex`.
   *Reversal:* the core op is additive — delete it and the view falls back to `move` + `set`.

5. **Done is a column; Archived is not on the board.** A done ticket still answers "where is the work", so it is drawn (struck through, dimmed) until it is archived. Archived tickets are out of the board by construction — the board's query is `TodoQuery`'s default, which excludes them — because archived means "out of the way". "Archive done" sits in the board header, where done tickets accumulate.
   *Consequence, recorded as a ticket:* archived tickets now have no view anywhere in the app. That is not a regression (they never had one), but it is the next hole to close.

6. **A tile on the board shows the ticket, not its body.** Checkbox (complete/reopen), title (three lines), `@assignee` when set — the reserved `human` stays bare, because it is a person rather than an agent handle — and a paperclip when linked. The description is the ticket's body and reading or editing it is map ticket 6's decision; a tile is a glance, not a page.

7. **Every move is possible without a drag.** The ticket's menu carries a Status submenu, a Move-to-lane submenu, Edit title, Reopen, Delete, and it opens the linked note. Double-click edits the title in place; the checkbox completes. Dragging is the fast path, never the only path — the same rule the list follows.

8. **No horizontal scroll.** The five columns share the width, down to the window's 760 pt minimum (about 137 pt a column). A board that scrolls sideways hides a stage; a narrow tile still reads its title.
   *Reversal:* proportional minimums plus a scroll view, if the human finds 137 pt too narrow in practice.

9. **Capture stays on the list.** No quick-add, no lane creation on the board: the board is for placement. A ticket is born in the list (or the CLI), then refined on the board.
   *Reversal:* free — the lane-add machinery already exists in `MainView`.

10. **The search field is the same query as the list's.** `TodoQuery.text` over title and description; the board draws every lane and column, and their counts say how many of the matches are there. Two empty states, for two different facts: a search that matches nothing says "No matches"; a store with nothing in it says "Nothing on the board" instead of drawing a grid of empty cells. A lane with no tickets still draws — it is a target, not an absence.

## Testing

- **The core, at the existing seam** (`TandemCoreTests`): `TodoBoard.rows` — empty lanes draw, cells keep stored order, the lane order is the list's, archived is the caller's choice; `Store.place` — both axes land in one call, append and index, `completedAt` stamped and cleared, a no-op drop leaves the ticket in place, unknown lane and id rejected with the store untouched.
- **The view, by hand** — the GUI has no test target (ADR 0009's seam rule), and the human is the judge of layout. The build is run against the dev store, which carries tickets in `refined`, `ready` and `in-progress`: statuses the list cannot render and the board must.

## Out of Scope

- **Description in the GUI** (ticket 6) — where the body is shown and edited.
- **The human's inbox** (ticket 5) — the view of `assignee: human`.
- **Archived tickets in the GUI** — seeing the archive and unarchiving from it. New ticket, below.
- **Lane selection for new tickets** (ticket 9) and **the persona registry** (ticket 10).
- **Multi-select, keyboard-driven moves, filters over assignee** — not asked for, and each is a view decision with its own cost.
