# Tandem epic lanes

The Tandem project was one lane (`Tandem`) plus leftovers (`tandem-ui`, `Port`,
`Retro`). Triage had nowhere to land: UI work, core work, CLI work and MCP work
all filed to the same lane, and the lane a batch drew from was never the epic
that owned it. This note defines the epic lane set, the move map that put the
open rows where they belong, and the journey a cold pass runs end to end.

## The lane set

All seven live under the `Tandem` project. `Tandem` itself stays as the triage
inbox until the open batch closes (see Retire below).

- `ui`: renderer faces (Tickets, Workflow, Sheet, Board) and their tests. Not
  shell chrome, CLI text, or domain rules.
- `core`: Rust domain, store, schema, sprints, sessions and usage derivation.
  Not CLI grammar or wire text.
- `cli`: `tandem-cli` verbs, usage texts and goldens. Not domain rules.
- `mcp`: MCP server, tools and registry. Not general rpc or app reads.
- `shell`: Electron shell, rpc stream, harness wiring and packaging. Not
  renderer faces.
- `skills`: agent prompts, skills, sprint loop docs and the team roster. Not
  product code.
- `docs`: ADRs, guides, release notes and curated memory pointers. Not code.

Keys are the first alphanumerics of each name (`UI`, `COR`, `CLI`, `MCP`,
`SHE`, `SKI`, `DOC`), unique across lanes, frozen once a lane holds a number.

## Retire and remap

No lane is silently dropped. Every old home has an explicit next step.

- `Tandem` (the lane) stays as the triage inbox. Untriaged work still files
  here, and the in-progress Sprint 17 batch rows stay here until they land. The
  follow-up after the batch closes is to file new work straight to an epic lane
  and leave `Tandem` for triage only.
- `tandem-ui` (project and lane) stays. It holds no open rows today, and no row
  moves in or out in this pass. Future UI milestones land in `Tandem/ui`; the
  `tandem-ui` project remains for standalone UI cuts.
- `Port` (project and lane `Port` with `PRT-1`, `PRT-32`, `PRT-40`) stays.
  Platform artifacts (one binary, three platforms, signing, release) remain
  there. Renderer chrome work lives in `Tandem/ui`; `TAN-245` keeps its edge to
  `PRT-1` across lanes because edges are UUID based and survive a lane move.
- `code-rank` and `Hosting` are untouched. This pass is Tandem epics only.
- `Retro` is untouched. Findings stay where the retro filed them.

## Move map

Every move below ran as `tandem-cli ticket move --id <ref> --lane <epic>` with a
dated comment on the ticket naming the old lane and the reason. Parked rows
(on-hold, human) kept their hold and owner across the move: a lane move never
touches status or assignee.

- `TAN-304` (Eager auto-migrate for non-additive schema changes): `Tandem` to
  `core`. Store and schema seam work.
- `TAN-303` (no way to add notes in bikeshed): `Tandem` to `ui`. Bikeshed face
  needs a note write entry.
- `TAN-302` (auto migrate on deploy): `Tandem` to `core`. Store migrate trigger.
  Kept on-hold with the human across the move.
- `TAN-283` (Sprint runner part 4): `Tandem` to `core`. Sprint record domain.
- `TAN-280` (app agent runtime): `Tandem` to `shell`. Embedded harness vs
  control surface. Kept on-hold with the human across the move.
- `TAN-271` (packaged verification exercises harness binary): `Tandem` to
  `shell`. Packaged app and harness wiring.
- `TAN-245` (port chrome and Tickets face): `Tandem` to `ui`. Renderer chrome
  and face hierarchy. Edge to `PRT-1` preserved.
- `TAN-237` (usage skills drill-down): `Tandem` to `core`. Usage derivation;
  the Usage face reads it.
- `TAN-236` (marketing agent): `Tandem` to `skills`. Agent shape, not product
  code. Kept on-hold with the human across the move.
- `TAN-217` (screenshot catalogue): `Tandem` to `ui`. Committed pictures of
  faces.
- `TAN-28` (assignee identity and personas): `Tandem` to `skills`. Personas and
  memory placement.

Not moved in this pass (left for their owners, no collision):

- `TAN-297`, `TAN-298`, `TAN-299`, `TAN-300`, `TAN-301`, `TAN-305` and this
  ticket `TAN-306`: in-progress in `Tandem`. They stay until they land, then
  file to their epic lane next.

## Journey

A cold pass runs the same acts from the CLI and the app. Lanes are explicit:
no auto-sorting, no WIP limits, placement is a verb.

CLI (cold pass, fresh home first):

- `tandem-cli lane add --name Tandem`: the first lane names its project.
- `tandem-cli lane add --name <epic> --project Tandem`: every epic lane after
  the first names the project so all epics share it.
- `tandem-cli lane list`: declared lanes in render order, empty ones included.
- `tandem-cli lane move --name <name> --index <n>`: reorder lanes, 0-based,
  clamped, omitted appends.
- `tandem-cli lane rename --from <a> --to <b>`: rename in place, tickets follow.
- `tandem-cli lane delete --name <name>`: empty the lane to the default area,
  never a delete of tickets.
- `tandem-cli ticket move --id <ref> --lane <epic> [--index <n>]`: file a ticket
  in its epic lane.
- `tandem-cli session start --agent builder --brief - < brief.txt`: open the
  record the sprint names.
- `tandem-cli sprint open --lane <epic> --mode serial|parallel --session <id>`:
  open the epic lane record.
- `tandem-cli sprint ticket --lane <epic> --ticket <id>`: draw one epic ticket
  into the batch, in stored order.
- `tandem-cli sprint run --lane <epic> --note <text>`: land one dated entry.
- `tandem-cli sprint close --lane <epic> [--note <text>]`: stamp the close.
- `tandem-cli sprint list --lane <epic>` and `sprint show --lane <epic>` read
  the lane record back.

App:

- Workflow face: lane picker, Add lane, Rename, Move up, Move down, Delete;
  sprint opener with mode picker; DAG and batch chips; run log.
- Ticket sheet: lane picker moves the ticket (`ticket.move`); lane create and
  rename from the sheet (`lane.add`, `lane.rename`).
- Batch draw, run entries and close stay CLI and MCP acts (`sprint ticket`,
  `sprint run`, `sprint close`): the sprint session stays the run log only
  writer, so the app opens the record and reads the batch and runs back.
- The Workflow surface test pins the wire set so no UI-only write appears:
  `lane.list`, `lane.add`, `lane.rename`, `lane.move`, `lane.delete`,
  `sprint.list`, `sprint.show`, `sprint.open`.

Errors name the lane and the fix:

- Unknown lane: `Unknown cycle lane "X". Create it with lane add, or list lanes
  with lane list.`
- No open sprint: `No sprint is open in lane "X". Run sprint open --lane X
  first, or check the lane.`
- Unknown sprint: names `Sprint <n> in lane "X"` plus the numbers the lane
  holds.
