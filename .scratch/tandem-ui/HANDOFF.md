# Handoff: ticket 01 — redo the v2 prototype with clean execution

For: the agent picking up ticket 01 after the human's first look at the v0
prototype. Map, brief, and settled decisions live in
`.scratch/tandem-ui/map.md` (on `main` — read with
`git show main:.scratch/tandem-ui/map.md` while on the prototype branch).

## Human verdict so far (2026-10-11, verbatim)

> "it's going in the right direction but i definitely want the popover and the
> deeper theme integration but the style looks a bit crappy and not at all
> clean — especially in the main app."

Keep: **reach C (deeper)** and the **themed menu bar popover**. Redo: the
visual execution — the **main window is the failure point**.

## The task

One-shot a clean version of the prototype. Not another round of rough
variants: make the calls, execute, deliver one polished artifact. The main
window (reach C) and the popover get the effort; reach A/B stay as cheap
toggleable references in the switcher.

## Why v0 looked bad (diagnosis from the previous agent — trust it)

1. **Hand-rolled Canvas vector art everywhere**: the two-rider bike, the
   line-lettered wordmark, the header band, the icon — all hand-tuned
   geometry. At UI scale that reads amateurish. The two-rider bike is the
   hardest asset in the set; the **bare bike with the double saddle reads
   better as the brand mark** than the full riders, which should appear at
   most once, large, where they can actually read (empty state).
2. **Over-decoration**: reach C stacked header band + panel treatment +
   custom controls, and it read busy instead of clean. Ligne Claire "clean"
   is restraint — uniform hairlines, flat fills, negative space, **native
   body typography** — not more custom chrome.
3. **No shared scale**: line weights, corner radii, and spacing were ad-hoc
   per control. Define the constants once; use them everywhere.

## What stays (don't rebuild)

The v0 scaffold on branch `prototype/tandem-ui-v2` builds and works: the
SwiftPM package, the reach/face/appearance switcher, the **real
`NSStatusItem` + themed `NSPopover`**, the in-memory model with the one-way
checkbox, and `--export-previews`. Rewrite the visual layer in place. All
settled decisions in the map stand (popover = top-5 open + "… n more",
filter segmented with counts, quick-add at top, delete = hover + context
menu, gentle motion, follows system appearance).

## Quality bar (checkable)

- The main window looks like a **shipped indie macOS app**, not a wireframe
  or a comic panel. Body text is the system face with native metrics.
- One shared line-weight + radius + spacing scale, used by every control.
- Illustration appears in **at most two places** in the main window (slim
  header band + empty state), each with generous negative space. The header
  band is quiet: sky, one or two geometric hills, a small bare bike — not a
  scene.
- Light and dark both look intentional (night is a tuned palette, not an
  inversion).
- Verify before handing back: run `--export-previews` and give the renders
  a vision pass (the previous agent's model could not see images — yours
  may; if not, say so in the handback instead of guessing). The bar is the
  human saying "clean".

## Protocol

- Work on `prototype/tandem-ui-v2`; commit the result there. Losing
  variants/assets stay on the branch.
- Leave ticket 01 `Status: claimed` — the human records the final verdict
  (reach, what lands, what dies, palette lock) at resolution.
- Run: `cd prototype && swift run TandemPrototype` ·
  Export: `cd prototype && swift run TandemPrototype --export-previews`
- Palette candidates (named, day + night):
  `prototype/Sources/TandemPrototype/Palette.swift`.
- Dead reference, same scaffold shape: v1 prototype on branch
  `prototype/tandem-ui` (pixel direction — dead, do not revive).
