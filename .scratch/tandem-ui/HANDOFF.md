# Handoff: ticket 01 resolved — next is the implementation session

Ticket 01 (theme reach prototype) closed 2026-10-11: the human judged the
v2 clean redo and chose **reach B — subtle Ligne Claire** ("ok i like option
b of the prototype"). The themed menu bar popover is kept. Main keeps only
the verdict — it is recorded in the map's "Decisions so far" and on
`issues/01-theme-variants-prototype.md`.

Everything else lives on branch **`prototype/tandem-ui-v2`**:

- the working prototype (B default face; A/C toggleable in the switcher;
  real `NSStatusItem` + themed `NSPopover`; `--export-previews` →
  `prototype/preview/`)
- the full implementation handoff at `.scratch/tandem-ui/HANDOFF.md`
  **on that branch** — read it before starting the port: what B is
  control-by-control, the don'ts, the `ImageRenderer` export gotcha,
  and the open items.

Next steps per the map: port B + the popover into the real app
(`Sources/Tandem/`); confirm IA/chrome + motion details (ticket 02 — the
v2 prototype already answers most of it at decision fidelity); lock the
type system and final assets (ticket 03).

(The previous handoff here — the v2 redo brief — is in git history; see
`git log -- .scratch/tandem-ui/HANDOFF.md`.)
