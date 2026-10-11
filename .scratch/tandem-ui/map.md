# Map: Tandem v2 UI/UX — a modern, cosy, Ligne Claire macOS app

Type: wayfinder:map

## Destination

A ready-for-agent spec for the v2 UI/UX of the Tandem app: a modern native macOS 26 app, visually in the **Ligne Claire** (Hergé-style clear line) + minimalist flat illustration register, carrying a light tandem-bike brand motif ("two riders, one machine" = human + agent, one list). Exactly two faces (Main window + Menu bar item) and a frozen domain (title + done, store schema v1, MCP surface unchanged). The spec itself is written at handoff (planned: `.scratch/tandem-ui/spec.md`); this map only settles the decisions the spec depends on.

## Notes

- Domain: Swift/SwiftUI macOS app — the `Tandem` GUI target only (`Sources/Tandem/`). `TandemCore` and `tandem-mcp` are frozen. macOS 26+, SwiftPM (no Xcode project), Homebrew distribution (no App Store/signing/notarization), single-instance regular app (Dock + standard window).
- Tracker: local markdown — map at `.scratch/tandem-ui/map.md`, tickets at `.scratch/tandem-ui/issues/NN-<slug>.md`. Claim = set `Status: claimed`. Blocking = `Blocked by:` line in the ticket header.
- Skills to consult: `grilling` + `domain-modeling` for grilling tickets; `prototype` for prototype tickets; `research` for research tickets. `GLOSSARY.md` at repo root is the vocabulary authority.
- Prototypes live on throwaway `prototype/<name>` branches (v1 convention: `prototype/tandem-ui`). The v2 prototype should reuse the v1 prototype's approach: a small throwaway SwiftUI package, not the real app.
- **Style brief** (settled at charting, 2026-10-11) — "Ligne Claire (Clear Line) + Minimalist Flat Illustration":
  - Crisp, uniform black linework defining shapes and boundaries (European comic / Bande Dessinée tradition, Hergé).
  - Flat colour fills; shading only as distinct, hard-edged darker tones (cel-shading); no gradients or soft transitions.
  - Muted, desaturated, earthy palette: dusty blues, washed-out reds, sandy beiges — calm, slightly melancholic, vintage atmosphere.
  - Minimalist composition: vast negative space, heavily simplified geometric backgrounds, focus on few foreground subjects.
  - Register: modern indie video game art, contemporary editorial illustration, a panel from an atmospheric graphic novel.
  - Intensity: the theme lives at the **brand surfaces** (app icon, menu bar glyph, wordmark, illustrations, palette); the interface stays native-clean. How far it may reach into the interface is open → ticket 01's variants.
- Settled during charting (conversationally, not as tickets):
  - Plan, don't do: the map produces decisions, not the built UI; the rewrite is a later implementation session.
  - Domain frozen: Todo = title + done + timestamps; single list, creation order; no due dates/priorities/lists/notes/tags/search.
  - Completing a Todo = **explicit checkbox** (a plain row click is never destructive); delete = hover affordance + right-click context menu. The checkbox is **one-way**: un-checking a done todo does nothing (reopen deferred — see Out of scope).
  - Menu bar surface = a **themed `NSPopover`** (SwiftUI content) replacing v1's AppKit `NSMenu` — the menu bar is the most-visible brand surface and should wear the theme.
  - Menu bar glyph = a **tandem-bike mark** in the Ligne Claire style, template-rendered, plain — no count badge. Overrides v1's settled `checklist` SF Symbol.
  - Default UX set: ⌘1/⌘2/⌘3 switch filters, a shortcut to focus quick-add, gentle motion (rows animate in; completing animates; clear-done animates out), cosy on-theme empty states, corrupt-store banner restyled to match. Polish and affordances only — no new features.
  - Standing v1 decisions that carry over: Filter control is segmented Open/Done/All with live counts; quick-add at top, Return commits; live updates from the store; corrupt-store recovery banner.
  - Appearance: follows the system; both light and dark tuned to the muted palette; which mode showcases is judged in the prototypes (light is the natural Ligne Claire register — paper-like — dark is the "night panel" variant).
- 8-bit/pixel-art direction was considered and **replaced** by the Ligne Claire brief at charting (2026-10-11); pixel fonts and sprites are dead.

## Decisions so far

<!-- one line per closed ticket: name (wrapping its link) + one-line gist -->

## Not yet specified

- **Interface reach**: how far the Ligne Claire treatment extends beyond brand surfaces (row styling, filter control treatment, surfaces, motion vocabulary, illustrated moments) — judged on ticket 01's variants; graduates into the design spec once a direction is chosen.
- **Main window IA & chrome**: header/wordmark treatment, quick-add placement, filter control look, row layout (checkbox, title, creation age, delete affordance), title-bar treatment, min size/resizability — judged on ticket 02's prototype.
- **Popover content & layout**: quick-add placement, top-5 open treatment, "… n more" truncation row, "Open Tandem" action, popover sizing — judged on ticket 02's prototype.
- **Type system**: wordmark/display treatment (the 8-bit pixel font is dead) plus body face — candidates surfaced in 01, final in 03.
- **Motion spec**: which events animate, durations/curves, completion animation — judged in 02.
- **Final asset lock**: exact palette values (light + dark), the tandem-bike illustration set (app icon, menu bar glyph, empty-state art), wordmark, microcopy voice — ticket 03.

## Out of scope

- **Reopen of done todos** (un-checking) — deferred by explicit decision (2026-10-11): the v2 checkbox is one-way. A future effort would add a small GUI-only `reopen` op to `TandemCore` (MCP surface unaffected).
- Domain features: due dates, priorities, lists/projects, notes, tags, search — the v2 model is title + done only.
- Any change to `TandemCore` or `tandem-mcp` — store schema stays v1, MCP surface stays `list`/`add`/`complete`.
- New faces: global capture hotkey, notifications, separate capture window.
- Appearance forcing (dark-only or light-only) — the app follows the system.
- Distribution changes: App Store, code signing, notarization — the Homebrew flow stands.
- Multi-machine sync, remote agents, auth — local same-user only.
- 8-bit / pixel-art visual treatment — replaced by the Ligne Claire brief at charting.
