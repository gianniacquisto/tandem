# Spec: the ticket sheet — open a ticket, then edit it

Type: spec
Status: implemented
Source: `.scratch/board/map.md` (ticket 12), `.scratch/board/description-spec.md`, ADR [0017](../../docs/adr/0017-the-ticket-model.md)

## Problem Statement

Every field of a ticket could be changed somewhere — title inline, stage by dragging or the menu, lane by the menu, body by the description sheet — and none of it could be *read as a whole*, in one place. A ticket was not openable. The description sheet (map ticket 6) was the only "detail" surface and it showed one field.

## The Decisions

1. **A ticket opens in a sheet: title, Status, lane, body, assignee (read-only).** One place to see what a ticket is and to change any of it. "Openable" means a gesture, not a menu item — the menu item is the fallback for discoverability.

2. **Double-click opens — the whole ticket, title included.** One rule on every face: double-click a row or a tile, anywhere, and the ticket sheet opens. Inline rename stays available as the menu's **Rename** item (and, of course, as the sheet's title field); it is no longer bound to double-click, because the title is most of a row and most of a tile, and binding the common gesture to the rare one made "open" a small, hidden target. This reverses an earlier convenience deliberately.

3. **Status and lane are settable here, not only by dragging.** The sheet is the keyboard path the board never had: a person on a narrow window, or one who does not want to drag, can move a ticket's stage and lane with two clicks. Status uses the same five values in workflow order; the lane list is every lane the face can draw, plus "Uncategorized".

4. **Assignee is read-only.** Assignment is an agent's instruction (`--assignee`), and the inbox is where the human answers it. A GUI assignee picker would invite the human to push work onto agents, which is the opposite of the model's direction (ADR 0017, inbox spec).

5. **Save writes only what changed, in at most two operations.** Title, Status and body travel in one `set`; a changed lane is its own `move` because a lane change also decides the ticket's position in that lane. Opening a ticket, reading it, and pressing Save writes nothing. An empty title cannot be saved (the button is disabled) — the store would reject it anyway.

6. **The description sheet becomes the ticket sheet.** There is one detail surface, not two; the standalone "Edit description…" item is gone. The body is still raw markdown in a monospaced field (description spec, decision 2) and Save is still the CLI's instruction.

## Testing

- **The core:** nothing new — `set` and `move` are covered, and the sheet calls them.
- **The view:** by hand (no GUI test target, ADR 0009): double-click a ticket from the list, the board and the inbox; confirm the sheet opens with its fields; change title, stage, lane and body; Save; confirm all three faces agree. Reopen and Save without changes to confirm nothing is written (`todos.json` mtime).

## Out of Scope

- **Rendered markdown** — still the follow-up named in the description spec.
- **Editing the link or the archive flag in the sheet** — the row/tile menu owns those, and both are one click from it.
- **Assignment from the GUI** — see decision 4.
- **A detail *pane* (side-by-side with the list)** — a sheet is the smallest surface that answers "open it"; a pane is a layout decision of its own.
- **Autosave** — see the description spec.
