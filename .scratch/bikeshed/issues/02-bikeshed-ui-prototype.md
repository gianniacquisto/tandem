# 02 - Bikeshed surface: main-window tab, tree, editor, search, backlinks

Type: prototype
Status: resolved (2026-10-15, human verdict on the prototype)

## Question

What should the bikeshed surface look like and feel like? Raise the fidelity with a cheap, rough, concrete artifact to react to — a SwiftUI stub in the `prototype/` package (the existing pattern; see `prototype/` at repo root) showing the Bikeshed tab in the main window:

- nested file tree sidebar (folders + `.md` files)
- editor pane with the chosen technology's live preview (per the editor-technology verdict)
- search affordance and where results appear
- backlinks affordance ("linked mentions" of the open note) and where it lives
- wikilinks rendered inline (provisional behavior is fine — the wikilink-model ticket owns the model)
- empty state / no note selected

As the human reacts to the artifact, settle: search semantics (matching rules, result presentation) and backlinks panel behavior (what it lists, where it lives). Link the prototype as an asset (e.g. branch `prototype/bikeshed-ui`), the way earlier prototype tickets did.

## Verdict

Human verdict on the prototype (2026-10-15): "keep search on the left and backlinks on the bottom for now."

- **Search** lives in the left sidebar: a field above the file tree; while typing, the tree is replaced by full-vault results. The matching rules as prototyped stand for v1: case-insensitive substring over title + body, one row per note (not per match), sorted by vault path; title hits lead with the note's first line, body hits with a snippet around the first match; clicking a result opens the note and clears the query.
- **Backlinks** live in a collapsible "Backlinks" dock at the bottom of the editor column: each row is the linking note plus the line containing the wikilink that resolves to the open note; the dock header carries the count; empty reads "Nothing links here yet." Broken links contribute nothing (ticket 03).
- The other placements (title-bar search with drop-down card, dedicated search panel, side-rail backlinks) stay toggleable in the prototype as references, and remain open for the spec to revisit.
- Left loose for the spec (noted, not decided): shared-window sizing across the two tabs (the prototype uses a 1060pt-wide window with the Todos column centred at 540pt), and whether search should also get a ⌘K-style palette entry point.

Prototype state: branch `prototype/bikeshed-ui` — throwaway, never merged, no PR. Run: `cd prototype && swift run TandemPrototype`; previews: `--export-previews`.
