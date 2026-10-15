# Tandem prototype — the Bikeshed surface (bikeshed map, ticket 02)

**PROTOTYPE — throwaway. Not the real app, not the spec.**

The main window gains a second tab — **Todos | Bikeshed** (settled on the map;
the menu bar item stays todos-only). This prototype judges the **Bikeshed
surface**: where the file tree, the editor, the search, and the backlinks
live, and how they feel.

## Run

```sh
cd prototype && swift run TandemPrototype
```

Opens on the Bikeshed tab (the Todos tab is one click away in the window
header, so the shared window can be felt). A floating black pill at the
bottom switches the variant axes — it is obviously not part of the design.

Export the surface PNGs to `preview/` (all 6 combos × day/night):

```sh
swift run TandemPrototype --export-previews
```

## The two axes under judgment

The floating bar (and ←/→, which cycle the first) switches:

- **Search placement** — where the search affordance lives and where results appear:
  - **in tree** — a field at the top of the tree sidebar; typing swaps the
    tree for full-vault results.
  - **in bar** — a field in the editor's title bar; results drop from the
    bar as a floating card over the preview (click away or Esc to dismiss).
  - **panel** — a "Search" toggle in the title bar opens a dedicated right
    panel with its own field and a full results list.
- **Backlinks placement** — where the "linked mentions" of the open note live:
  - **bottom** — a collapsible "Linked mentions" dock under the editor.
  - **side** — a right rail beside the editor.

With **panel + side** the two share the right rail: the search panel takes
it while open, backlinks own it while the panel is closed.

## What the prototype does (provisional semantics to react to)

- **Tree** — nested folders + `.md` notes, folders first, alphabetical;
  attachments and hidden files never appear (ticket 04). `+` creates a note
  at the vault root.
- **Editor pane** — the live-preview mock from `Markdown.swift`: headings,
  paragraphs, nested lists, **interactive task checkboxes**, blockquotes,
  fenced code (language tag, no syntax highlighting), tables, inline +
  display math (styled stand-in, not KaTeX), whole-line images (flat Ligne
  landscape for a present local file; dashed placeholder for missing /
  never-fetched remote). This stands in for the settled tech (ticket 01:
  CM6 in a WKWebView) — the face is judged for surface, not editing.
  The buffer is read-only here; typing is ticket 01/05 territory.
- **Wikilinks** (ticket 03, provisional): resolved links are dusty blue and
  open the target; **broken links are ember and create the note on click**.
  Resolution is derived — a broken link starts resolving the moment the
  target note exists.
- **Backlinks** — notes containing a wikilink that resolves to the open
  note; each row shows the linking note plus the line that mentions it.
  Broken links contribute nothing. Zero backlinks reads "Nothing links here
  yet."
- **Search** (provisional rules — the exchange settles these): case-
  insensitive substring over **title + body**, **one row per note** (not per
  match), sorted by vault path; title hits lead with the note's first line,
  body hits with a snippet around the first match. Click opens the note and
  clears the query.
- **Empty states** — "wipe vault" (floating bar) shows the designed
  empty-vault state (ticket 04: no welcome note, lazy creation); deselecting
  a note isn't wired, but the "no note selected" placeholder renders when
  `openPath` is nil. "reset data" restores the seed.

## Caveats

- In-memory seed vault only — no disk, no watcher, no persistence.
- The v2 base app is intact (reach A/B/C, popover, assets face) as the
  chrome reference; reach B is the default and the one the bikeshed face
  is designed for.
- Window: the shared window opens at 1060×680; the Todos column is centred
  at 540pt. **Window sizing per tab is an open question** — react to it.
- The in-memory model (`BikeshedModel.swift`) had two compile-blocking
  fixes applied to make the package build: the search snippet now slices
  from the original-case body (it was lowercasing the snippet text), and
  `imageExists` unwraps the optional path index. Semantics unchanged.

## Decisions this prototype surfaces

1. Search placement + result presentation (the three placements above, and
   the provisional matching rules).
2. Backlinks placement + row content (context line yes/no, count, collapse).
3. Shared-window sizing: one window for both tabs, and how the Todos face
   reads at the wider width.
4. Whether the in-title-bar search (bar style) should be the affordance at
   all, versus ⌘K-style (not modelled — add it if the reaction wants it).
