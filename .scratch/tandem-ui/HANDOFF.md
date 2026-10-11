# Handoff: ticket 01 resolved — reach B (subtle Ligne Claire) is the direction

For: the agent starting the **implementation session** that ports the chosen
direction into the real app (`Sources/Tandem/`), plus the follow-up tickets
(IA/chrome + motion, type system, final asset lock — see the map's
"Not yet specified"). The prototype on this branch is the reference; the map
(`git show main:.scratch/tandem-ui/map.md`) holds all settled decisions.

## Verdict (2026-10-11, verbatim)

Human, after the v0 verdict ("…the style looks a bit crappy and not at all
clean — especially in the main app") and the v2 clean redo:

> "ok i like option b of the prototype"

**Chosen: reach B — subtle Ligne Claire.** The theme lives in the chrome:
flat-filled paper surfaces, uniform 1pt hairlines, line-drawn controls,
illustration only in the empty state. No illustrated header band (that was C).
The **themed menu bar popover stays** (kept from the original brief).

## What B is (as built, in the prototype)

Main window (reach B face):
- Paper background (`#F5F0E6` day / `#23262B` night), system body type.
- Header row: "Tandem" wordmark in **Avenir Next Demi Bold** 15pt +
  "Clear done" text button (disabled when no done todos).
- Quick-add at top: flat parchment field (`#EAE2D0` / `#2C3037`), 1pt ink
  hairline border, SF Symbol plus, ember border when focused, Return commits.
- Filter: flat segmented control 240×30, Open/Done/All with **live counts**,
  selected segment dusty blue (`#7490A6` / `#92AAC0`), no inner dividers.
- List: 40pt rows, hairline dividers, line-drawn **one-way checkbox** (18pt,
  5pt radius; checking animates to moss; un-checking does nothing), title
  truncates, age string right-aligned in dim ink, delete on hover (fixed
  22pt slot) + context menu.
- "All clear" empty state: the **two riders, large (220pt)**, on a sand
  ground hairline — the one place riders appear. Other empty states: text +
  small bare bike.
- Corrupt-store recovery banner: parchment, 1pt ink border, ember accent bar,
  SF Symbol triangle, monospace backup path, "Restore" button. No
  illustration in the banner.
- Gentle motion: insert easeOut 0.20, toggle easeInOut 0.18, remove easeIn
  0.15. Shortcuts: ⌘1/⌘2/⌘3 switch filters, ⌘N focuses quick-add.

Popover (unchanged from the v2 redo — the human wanted it kept):
- 280pt wide, paper card with **soft shadow, no ink outline**, up-arrow.
- Header: bare bike mark (24pt) + "Tandem" Avenir Next Demi Bold 13pt.
- Quick-add (28pt tall), hairline, top **5 open** todos (checkbox is the
  only clickable action; row click is not destructive), "… n more" in ember
  when truncated, dusty-blue "Open Tandem" button.

Brand assets (drafts — final lock is the asset ticket):
- **Bare bike with double saddle** = the brand mark (app icon, menu bar
  template glyph, non-hero empty states). No timing-chain clutter.
- Two-rider scene = empty-state hero only.
- Wordmark: display face (Avenir Next Demi Bold) in the UI; the custom line
  lettering survives only as a draft candidate in the assets face.
- Palette: `Palette.swift` day + night values (paper/parchment/sand/ink/
  inkDim/dustyBlue/ember/moss/sky/hill/skin) — muted, desaturated, earthy.

## Don'ts (learned the hard way)

- **No cel drop-shadows or comic outlines in the main window.** v0's hard
  offset shadows + ink outline on every panel read busy/comic. Restraint is
  the style: uniform hairlines, flat fills, negative space.
- **No hand-rolled vector chrome in the UI.** Illustration is limited to the
  empty state (B) — and even there, one moment, large, where it reads.
- **No line-lettered wordmark in the UI.** Native display face.
- **The two riders are not the brand mark.** Bare bike is.
- One shared scale — `prototype/Sources/TandemPrototype/Scale.swift`
  (Line 1pt hairline UI / 3.5 illustration design-space; Radius 5/9/12;
  Space gutter 20, row 40, control 34, popover 28/32/12; Motion above).
  Every constant in one place, used everywhere.

## Export gotcha (do not chase this as a bug)

`ImageRenderer` draws offscreen **live `TextField`s and the native segmented
`Picker` as solid system-yellow selection bands with red carets** — the
running app is never affected; only `--export-previews` was. The exporter
sets `.environment(\.tandemExport, true)` (see `Scale.swift` for the key):
in export mode the quick-add renders its placeholder text and reach A's
picker renders a flat approximation. If you add native controls to the
prototype's export paths, gate them the same way.

## Branch & verification

- Branch `prototype/tandem-ui-v2`: `b487805` v0, `86fe45a` v2 redo,
  then the B-default + verdict commits.
- `cd prototype && swift run TandemPrototype` — live app, **B is the
  default face**; the floating switcher still cycles A/B/C, face, appearance,
  banner, data reset. The menu bar item is a real `NSStatusItem` +
  `NSPopover` following the system appearance.
- `cd prototype && swift run TandemPrototype --export-previews` → PNGs in
  `prototype/preview/` (reach A/B/C × day/night, popover × day/night,
  assets gallery). The human judged from these.

## Open (next tickets, per the map)

1. **Port B + the popover into the real app** (`Sources/Tandem/`, macOS 26,
   SwiftPM target `Tandem`): replace v1's `MainView` IA and AppKit `NSMenu`
   with the themed face. Domain frozen: `TandemCore` + `tandem-mcp`
   untouched (title + done, store schema v1, ops list/add/complete).
   The v1 corrupt-store recovery path must keep working, banner restyled
   per the prototype.
2. **IA/chrome + motion spec** (map: "judged on ticket 02's prototype") —
   the B face is now the reference for row layout, title-bar treatment,
   min size/resizability, and the motion vocabulary; confirm details
   conversationally, don't re-open settled decisions.
3. **Type system**: display face = Avenir Next Demi Bold is the working
   choice; confirm/replace at final lock. Body stays the system face.
4. **Final asset lock** (map: ticket 03): exact palette values, bike
   illustration set (app icon candidate, menu bar glyph, empty-state art),
   wordmark, microcopy voice. The prototype's assets face
   (`prototype/preview/assets.png`) is the starting point.
