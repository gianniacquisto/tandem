# Tandem UI prototype (throwaway)

Answers ticket 04: **"What should Tandem look like?"**

Three structurally different variants of the main window, plus a live menu bar
item stub — cycled from the floating bar at the bottom of the window (or `←`/`→`).

**Run:** `cd prototype && swift run TandemPrototype`

## Window variants

- **A — Filter tabs**: segmented Open/Done/All with counts · creation order (store order) · no search
- **B — Sections**: Open/Done sections with pinned headers and count chips · done sorted newest-completed · no search
- **C — Search-first**: search field on top · single flat list of everything · recency order (last touch)

Each variant has the ticket-04 stub contents: the todo list, a quick-add field
(Return commits), tap-to-toggle done, and a clear-done action.

## Menu bar (real status item, top right of the screen)

The stub is a genuine `NSStatusItem`, so judge it where it will live:

- Quick-add field at the top (Return commits)
- Up to 5 open todos in creation order — tap a row to toggle done
- `… n more — open the app` when there are more than 5 open
- **Open Tandem** (⌘T) — activates the app

The **glyph** (circle-check / checklist / bullet-list) and **badge** (count on/off)
are cycled from the floating bar. This is deliberately *surfaced, not picked*:
ticket 04 keeps the icon decision open for a follow-up.

## Decisions this prototype surfaces (all still open)

1. Main window list treatment: filter tabs vs sections vs search-first — and the
   implied sort order (creation / completed / recency) and whether search exists in v1.
2. Menu bar icon: glyph choice × count badge vs plain icon.

## Caveats

In-memory seed data only — no store, no persistence, no MCP. This is a
throwaway for judging layout and interaction; it is not the app.
