# 04 - UI prototype: main window + menu bar dropdown

Type: prototype
Status: open

## Question

How should Tandem look? Build a cheap, rough, concrete artifact to react to — a throwaway SwiftUI stub (not the real app), enough fidelity to judge layout and interaction, not pixel-perfect:

1. **Main window stub**: the todo list, a quick-add field, done toggles, clear-done action.
2. **Menu bar item stub**: dropdown with ~5 open todos (tap to toggle done), a quick-add field, "Open app".

While building, make these fog decisions concrete enough to react to (they stay open until this ticket resolves them):

- Main window list treatment: filter (open/done/all) vs sections, sort order, whether search exists in v1.
- Surface the menu bar icon question (glyph choice, count badge vs plain icon) for a follow-up decision — don't silently pick.

## Prototype (built 2026-10-09)

Branch: `prototype/tandem-ui` — run with `cd prototype && swift run TandemPrototype` (see `prototype/README.md`).

Three structurally different window variants, cycled with ←/→ or the floating bar:

- **A — Filter tabs**: segmented Open/Done/All with counts · creation order · no search
- **B — Sections**: pinned Open/Done headers with count chips · done newest-completed · no search
- **C — Search-first**: search field on top · single flat list · recency order

Menu bar stub is a real `NSStatusItem` (quick-add, ≤5 open todos tap-to-toggle, "… n more", Open Tandem). Glyph (circle-check / checklist / bullet-list) × badge (on/off) is cycled from the floating bar — surfaced, not picked.

Awaiting reaction: which window treatment (or a mix), and the icon choice.
