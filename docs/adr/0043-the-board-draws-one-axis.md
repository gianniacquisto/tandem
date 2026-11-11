# The board draws one axis: the workflow's stages

The board carried two axes. Lanes were rows and the active workflow's stages were columns ([ADR 0041](0041-workflows-are-data.md)), so five lanes by six stages was 30 cells and most of them were empty dashed drop targets; a lane was also a filter in the header, so one fact was drawn twice. The board now draws one column per stage and nothing else: a lane is a chip on the card and a filter in the header, and the project is the focus the columns are narrowed by.

Decided 2026-11-11 (UI-33 built it).

## Decision

- **The columns are the stages.** The board draws one column per stage of the active workflow, in stage order, Done included. Each is an independent scroll pane with a sticky head carrying the stage's label and the number of cards in it, and each foot carries the column's add. A seventh stage arrives through the workflow read with no code change.
- **The board's set is every non-archived ticket.** It is narrowed by the project focus, the lane filter, the assignee filter and the search. The Open/Done/All scope and the status filter are the list's: the columns are the statuses, so a done ticket stands in Done whatever the scope says.
- **A lane is a label.** The lane draws as a chip on the card, coloured by a hash of the lane name over the palette tokens, so no colour is stored and the same lane keeps its tone across sessions. Clicking the chip sets the lane filter and does not open the sheet. The lane is also a filter in the header, never an axis of the board, and a ticket with no lane draws no chip.
- **The project is the focus.** `active_project` resolves against the board's projects, not the declared lanes. A stored value that names no project reads as no focus, which is the existing stale-focus rule, so no migration and no schema change. The show-all escape draws every project without clearing the stored focus.
- **A card is its own component.** The lane chip, the title, and a footer of ref, `@assignee` and the blocked mark. No completion checkbox, no status pill (the column is the status), no lane text line, no strikethrough for done. One click opens the sheet, and a drag starts on the card and carries the ticket id.
- **A drop is one write.** A drop on another column is one `ticket.set` carrying that column's stage. A drop in the column the card already stands in writes nothing, because the store's order is per lane and a lane-agnostic column has no order to write.
- **The list keeps the lane axis.** It groups by lane, keeps its scopes, its status filter and its quick-add, and is narrowed by the same project focus.

## Amendments

- **[ADR 0017](0017-the-ticket-model.md): the lane stays the third axis; the board stops drawing it as rows.** The cycle lane is still the area of concern, which the list groups by; the board now draws it as a chip on the card. The sentence "which the list groups by and the board draws as its rows" is overruled in its board half only.
- **[ADR 0039](0039-projects-own-lanes.md): the board groups by stage, not by project header.** The model stands unchanged: a project holds lanes, and a ticket's project is derived via its lane. The bullet "The board groups lanes under project headers in project order, default area first and ungrouped, empty projects not drawn" and the phrase "cross-project capture, first on the board" are overruled: the board draws stages, a project is the focus the columns are narrowed by, and the default area is a card with no chip.

## Why

- **Two axes drew one fact twice.** The lane was a row and a header filter at once, and the empty cells were most of the grid: a 78-ticket home needed a 1330 px window to show one screen.
- **A lane-agnostic column has no order.** The store's order is per lane (`ticket.move` takes a lane and an index in it), so a column that mixes lanes has no order a drop could write. The list owns ordering, and the board's drop is a status write alone.
- **A chip is the lane's whole drawing.** The lane's colour is a hash of its name, so the board gains a readable label without a stored colour and without a second truth.

## Consequences

- A board with an empty stage still draws that stage's column and its add foot, because the columns are the workflow's stages and exist with zero tickets. A first read that has not answered draws its message alone.
- The board's copy names the per-column add, and `⌘N` reaches the first column's add; `⌥↑↓` orders in the list only, and `⌥←→` steps the status.
- A stored `active_project` written by the lane-era build reads as no focus. The reader-first rule applies: no migration is needed because the stale value hides nothing.
