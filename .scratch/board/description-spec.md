# Spec: description in the GUI — shown, edited, searched

Type: spec
Status: ready
Source: `.scratch/board/map.md` (ticket 6), the ticket model spec (`.scratch/board/spec.md`), ADR [0017](../../docs/adr/0017-the-ticket-model.md)

## Problem Statement

A ticket's description is where the work is explained — the session that refines a ticket writes the design into it, and an agent that picks the ticket up reads it instead of asking. The model carries it (`description`, markdown, `todo set --description`), the store and the CLI can write it, `TodoQuery` already searches it — and the app cannot show it. A human reading the board sees titles only, and a human who wants to change a body has to open a terminal.

## The Decisions

1. **The body is visible where the ticket is.** The list row draws the description's first lines under the title, dimmed, two lines at most (`lineLimit(2)`); a board tile draws one line. It is the ticket's substance, not a detail behind a click — but a row stays a row: no expansion, no bubble, no hover card.

2. **One surface for reading and editing: a sheet with the markdown as text.** The row and tile menus gain "Edit description…" ("Add description…" when the body is empty), which opens a sheet: the ticket's title as its header, a monospace `TextEditor` with the raw markdown, Save and Cancel. What you open is what you read; what you save is what the CLI would have written.
   *Why not rendered markdown:* the bikeshed already renders markdown in a `WKWebView`, but reusing that renderer for tickets is a design decision of its own (fidelity, link handling, editing model), not a side effect of giving the body a face. Recorded as the follow-up that would upgrade this sheet; the stored format is markdown either way, so the upgrade costs no data.

3. **Editing is the same instruction as the CLI's.** Save is `set(id, description: .set(text))` in the core, which trims the edges and stores empty as `nil` — the CLI's `todo set --description` and the GUI's Save produce the same file. No new core operation, no new field, no new file shape.

4. **Search already covers it, and is not re-implemented.** `TodoQuery.text` matches title *and* description; the list's field and the board's field both run that query. The empty-state wording says so ("No ticket title or description matches your search").

5. **No autosave.** Save is explicit; Cancel and closing the sheet leave the body as it was. A description is prose — a half-typed paragraph is not a state worth persisting, and an agent may be writing the same body from the CLI.
   *The cost, recorded:* two writers of the same description are last-writer-wins. `set` writes one field under the store lock, so nothing else is clobbered; only concurrent edits to the same body can lose one. Acceptable for a single human; revisit if personas make shared editing real.

6. **The sheet belongs to the face that opened it.** The list and the board each own their presentation state (a `Todo?`), sharing one `DescriptionSheet` view; the store call goes through `TandemModel`, as every other GUI edit does.

## Testing

- **The core:** nothing new — `set(description:)` and the query's description matching are already covered (`TicketModelTests`, `QueryTests`). The GUI's Save is that operation, not a parallel one.
- **The view:** by hand (ADR 0009 — no GUI test target): open a ticket with a body, edit, save, confirm the list and the board show it; confirm the search finds a word that appears only in a description; cancel leaves the old text.

## Out of Scope

- **Rendered markdown** — the follow-up named in decision 2.
- **Editing the title in the sheet** — renaming stays inline, where it already is. *(Reversed by the ticket sheet: it carries a title field, because a ticket that opens should be editable in every field; inline rename stays as the quick path.)*
- **A description-only view or preview pane** — the sheet is the reading surface for now.
- **History, diffs, or "edited by"** — the store keeps no revision history.
- **The human's inbox** (ticket 5) — a view, not a field.
