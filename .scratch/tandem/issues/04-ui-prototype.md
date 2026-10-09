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
