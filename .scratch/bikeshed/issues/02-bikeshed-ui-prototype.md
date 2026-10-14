# 02 - Bikeshed surface: main-window tab, tree, editor, search, backlinks

Type: prototype
Status: open
Blocked by: 01

## Question

What should the bikeshed surface look like and feel like? Raise the fidelity with a cheap, rough, concrete artifact to react to — a SwiftUI stub in the `prototype/` package (the existing pattern; see `prototype/` at repo root) showing the Bikeshed tab in the main window:

- nested file tree sidebar (folders + `.md` files)
- editor pane with the chosen technology's live preview (per the editor-technology verdict)
- search affordance and where results appear
- backlinks affordance ("linked mentions" of the open note) and where it lives
- wikilinks rendered inline (provisional behavior is fine — the wikilink-model ticket owns the model)
- empty state / no note selected

As the human reacts to the artifact, settle: search semantics (matching rules, result presentation) and backlinks panel behavior (what it lists, where it lives). Link the prototype as an asset (e.g. branch `prototype/bikeshed-ui`), the way earlier prototype tickets did.
