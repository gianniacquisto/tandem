# Map: Tandem's todos become tickets — the board

Type: wayfinder:map

## Destination

A todo is a **ticket**: it carries a description, an assignee, a workflow **status**, an **archived** flag, and a **cycle lane**, and every one of those is manageable from `tandem-cli`. The app renders the same data as a **board** — columns by status, rows by cycle lane — so a design session can file a ticket whose description carries the design, move it to `ready`, and another agent can pick it up and drive it to `in-progress` and `done` without a human in the loop. The human is the escalation path: a ticket assigned to `human` is how an agent asks for one.

Settled by grilling, 2026-10-19. The model is decided (this map); the view, the verb table, and the migrator are the open work.

## Notes

- **Ticket store: the Tandem list itself.** This epic is its first customer — its tickets are Tandem todos, and `.scratch/<topic>/issues/NN-*.md` stops being a ticket store. The repo keeps the *record*: this map, a spec, ADRs, `GLOSSARY.md`. Cost accepted: the store is not in git, so a ticket needing long-form design links a bikeshed note.
- **Vocabulary authority: `GLOSSARY.md`** — `Status`, `Archived`, `Cycle lane` (was "section"), `Todo`, `Open`, `Done`, `Filter`. Board rendering borrows them: a column is a status, a row is a cycle lane.
- **Skills:** `grilling` + `domain-modeling` to chart; the tickets themselves are worked by CLI.
- **Origin:** todo 7ED2E8DE (*todo should actually be a ticket board — kanban … we'll need a wayfinder*) and DAE56C33 (*descriptions/details, assigned to:, workflow like backlog, refined, ready, in progress, done*).

## Decisions so far

- **Three independent axes** (ADR [0017](../../docs/adr/0017-the-ticket-model.md)) — **Status** (`backlog` default / `refined` / `ready` / `in-progress` / `done`), **Archived**, **Cycle lane**. Status is where the work is; archived is whether it is still in the way; the lane is what area it belongs to. Transitions are free-form — the value is validated, never the transition — and because `done` is a status value, completing a ticket erases the stage it came from: a reopen lands in `backlog`.
- **Done is a status value**, and the legacy `done` boolean stays on disk as a compatibility mirror (it wins on read; writes reconcile) because an older reader treats a missing required field as corruption. The envelope stays `version: 1`.
- **Two new fields** — `description` (markdown) and `assignee` (a human, or an agent id). Deliberately not: due date, priority, labels, checklists, comments. Origin ≠ assignee.
- **Agent-first** means tickets are CLI-manageable: write one, move its status, complete it, edit its description. The refinement flow is the reason — a session's summary becomes a description, the ticket becomes `refined`, then `ready`, and another agent picks it up.
- **The agent surface grows** — `todo add --status`; `list` filters; and a new `todo set` (title / status / assignee / description), which absorbs the standing todo *move and rename todos from cli* (F1056565). A description travels by stdin (`--description -`), the `note capture` precedent.
- **One query in the core** — filtering on text, assignee, status, cycle lane, archived is a single predicate parsed in `TandemCore`, so the GUI's filter box and the CLI's flags cannot diverge (the "drag arithmetic lives in the core" rule). `--filter open|done|all` survives as sugar; it is a shipped contract in `skills/tandem/SKILL.md`.
- **A migrator, gated on the install** — it retires the `done` mirror and may rename the stored `section` key to `lane`. Its precondition is that no binary older than this change can read the store, which is a reachability question, not a code one.
- **`assignee` stays a plain string** — the persona registry that would define agent identities is pinned below.
- **The board sits beside the list, as a third tab** (2026-10-20) — columns are the five Statuses in workflow order, rows are the Cycle lanes with empty lanes drawn as drop targets, a drop is one write (`Store.place`), Done draws and Archived does not, and the grouping lives in the core (`TodoBoard`) so the list and the board cannot disagree about a lane. Capture and lane chrome stay on the list. The decisions and their reversal costs are in `.scratch/board/board-spec.md`.

## Tickets

Chart these here; the live list is the board. (Order is not a sequence — most are independent.)

1. **The store carries the three axes** — `status`, `archived`, and the cycle-lane rename in `TandemCore`, with the `done` mirror maintained in one place (`normalize`), plus the core tests.
2. **The CLI ticket surface** — `todo set` (title / status / assignee / description-by-stdin), `todo add --status`, and the `list` filters. Absorbs F1056565 (move/rename from the CLI).
3. **The query in the core** — one filter parser shared by the CLI flags and the GUI's filter row, with `--filter` kept working as sugar.
4. **The board view** — columns by status, rows by cycle lane, tickets dragged between columns (absorbs the kanban todo 7ED2E8DE). Decides whether the board replaces the filter control or sits beside the list.
5. **The human's inbox** — the view of everything assigned to `human`: the escalation surface an agent uses to ask for a decision.
6. **Description in the GUI** — how a ticket body is shown, edited, and searched.
7. **Archive replaces "Clear done"** — the button stops deleting; single-item delete stays destructive.
8. **The migrator** — retires the `done` mirror (and the `section` key, if it moves), gated on the install.
9. **Cycle lane selection** — a cached default lane for new tickets (the standing todo 34150D95).
10. **PINNED — assignee identity and personas** — `agent.md`-defined specialists ("UI", "core", "CLI"), each with memory under its own part of the bikeshed, the human as the escalation target. Needs its own wayfinder: it touches the memory protocol, the CLI, and the app. `assignee` is a string until then.
11. **Archived is reachable in the GUI** — the archived view (the list is the natural home, as an "Archived" state beside Open/Done/All) and the way back: unarchive a ticket. The board excludes archived by decision, so without this an archived ticket has no face at all. `Store.set(id, archived:)` already exists; this is the view.

Built so far: **1, 2, 3, 7** (PR #38) and **4** (the board). The live list is still the board.

## Out of scope

- Due dates, priority, labels/tags, checklists, comments, attachments — the original wayfinder out-of-scoped due dates and priorities, so reversing that is its own decision, not a side effect of this one.
- Ticket hierarchy — "epic" as a parent ticket is a tree, not a board row.
- User-defined workflow columns — the five statuses are fixed, and the cycle lane is already the user's own naming axis.
- Permissions and auth — single user, no auth stays an invariant, so any writer may set any assignee.
