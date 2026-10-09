# 04 - UI prototype: main window + menu bar dropdown

Type: prototype
Status: resolved (2026-10-09)

## Question

How should Tandem look? Build a cheap, rough, concrete artifact to react to — a throwaway SwiftUI stub (not the real app), enough fidelity to judge layout and interaction, not pixel-perfect:

1. **Main window stub**: the todo list, a quick-add field, done toggles, clear-done action.
2. **Menu bar item stub**: dropdown with ~5 open todos (tap to toggle done), a quick-add field, "Open app".

While building, make these fog decisions concrete enough to react to (they stay open until this ticket resolves them):

- Main window list treatment: filter (open/done/all) vs sections, sort order, whether search exists in v1.
- Surface the menu bar icon question (glyph choice, count badge vs plain icon) for a follow-up decision — don't silently pick.

## Prototype

Branch: `prototype/tandem-ui` — run with `cd prototype && swift run TandemPrototype` (see `prototype/README.md` there). Three structurally different window variants (A: filter tabs, B: sections, C: search-first) cycled with ←/→ or the floating bar, plus a real `NSStatusItem` menu bar stub whose glyph (circle-check / checklist / bullet-list) × badge (on/off) was cycled live.

## Verdict (2026-10-09)

- **Window treatment: A — Filter tabs.** Segmented Open/Done/All with counts · creation order (store order) · no search in v1 · clear-done in the header.
- **Menu bar icon: `checklist` glyph, plain — no count badge.**
- Losing variants (B, C) and the switcher stay on branch `prototype/tandem-ui`; main keeps only the verdict.
