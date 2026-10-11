# 02 - Prototype: the v2 Main window and menu bar popover, in full

Type: prototype
Status: done
Claimed: 2026-10-11 (agent)
Resolved: 2026-10-11 (human approved the running port; the v2 prototype already answered the design at decision fidelity)

## Verdict (2026-10-11)

**Both faces as prototyped in reach B, ported into the real app** (`Sources/Tandem/`, commit "Port reach B + themed popover into the real app"); the human approved the running build ("this is great"). The v2 prototype branch answered the design at decision fidelity, so this ticket resolved as the port, not a new prototype.

- **Main window**: standard title bar (min 360×240, default 470×600, content-min resizability). Paper background (day/night by system appearance). Header row: "Tandem" in Avenir Next Demi Bold 15pt + "Clear done" text button (disabled when nothing done). Quick-add: flat parchment field, 1pt ink hairline, SF Symbol plus, ember border when focused, Return commits, ⌘N focuses. Filter: flat segmented 240×30, Open/Done/All with live counts, dusty-blue selected fill, no inner dividers, ⌘1/⌘2/⌘3. List: 40pt rows, sand hairline dividers, line-drawn one-way checkbox 18pt/5pt radius (checking animates to moss; a plain row click is never destructive), body-face title (dimmed + strikethrough when done), right-aligned monospaced age (now/m/h/d), hover delete in a fixed 22pt slot + right-click context menu (GUI-only). Empty states: "All clear" (open filter) = two riders, large (220pt) on a ground hairline; done/all = small bare bike; each with title + subtitle. Banners restyled to the theme (no illustration): recovery = parchment card, 1pt ink hairline, ember accent bar, triangle SF symbol, monospaced backup path, Restore (reveals the backup in Finder — the GUI never writes the store), dismiss; error = same card, xmark symbol, no accent bar.
- **Menu bar**: template bare-bike glyph (double saddle, plain, no badge) on a real `NSStatusItem`. Left-click toggles a **themed transient `NSPopover`** (replaces the v1 `NSMenu`), following the system appearance: 280pt paper card, system shadow, header = bare bike 24pt + wordmark 13pt, quick-add (28pt, hairline, Return commits), sand hairline dividers, up to 5 open todos (the checkbox is the only completing action), "… n more" in ember (opens the Main window) when truncated, "All clear" when empty, dusty-blue "Open Tandem" button (30pt). Right-click = utility menu (Open Tandem / Quit).
- **Motion**: insert easeOut 0.20, toggle easeInOut 0.18, remove easeIn 0.15; rows fade on list changes.
- **One fix found while approving**: the status item button had no target, so the popover never opened — set `button.target`.
- **Assets stay draft** (final lock is ticket 03): palette values are the prototype's day/night candidates; the app icon is still the v1 icon — the Ligne Claire icon candidate is not in yet.

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
