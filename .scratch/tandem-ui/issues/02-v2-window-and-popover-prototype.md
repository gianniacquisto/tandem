# 02 - Prototype: the v2 Main window and menu bar popover, in full

Type: prototype
Status: open
Blocked by: 01

## Question

In the direction chosen by "Prototype: how far does the Ligne Claire theme reach?", what exactly do the v2 **Main window** and **themed menu bar popover** look like and behave like? Build a throwaway SwiftUI prototype at decision fidelity (not pixel-perfect) covering:

**Main window**
- Header/wordmark treatment + title-bar treatment (and min size / resizability behaviour)
- Quick-add field (Return commits, focus retained for rapid entry) + ⌘-shortcut to focus it
- Filter control — segmented Open/Done/All with live counts, ⌘1/⌘2/⌘3 — restyled per the chosen direction
- Rows: **explicit one-way checkbox** (un-checking a done todo does nothing — the checkbox on a done row must read as "done", not as broken), title, creation age, hover delete + right-click context menu; a plain row click is never destructive
- Empty states, on-theme (illustrated, per the chosen reach)
- Corrupt-store recovery banner, restyled to match
- Motion: rows animate in; completing animates; clear-done animates out (specify which events, durations, curves)

**Menu bar popover** (themed `NSPopover` with SwiftUI content, replacing the v1 `NSMenu`)
- Quick-add field (Return commits)
- Up to 5 open todos, tap to complete (one-way)
- "… n more — open Tandem" truncation row / "All clear" empty state
- "Open Tandem" action
- Popover sizing + the tandem-bike glyph in the menu bar (template, plain, no badge)

Both faces must reflect the settled UX: creation order, live store updates, muted palette, no gradients, native-clean body type.

**Verdict (record on resolution):** the window + popover design the spec will describe — layout, row anatomy, control treatments, motion spec, empty states, shortcuts. Assets stay draft (final lock is ticket 03).
