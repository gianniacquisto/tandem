---
name: tandem-sprint
description: "The SDLC in one invocation — the whole loop, refine → build → review → retro: orient, triage the lane with refine filling the batch, run the batch through `tandem-build`, retro at the close. Reach for it when a coding session starts, when the human says run a sprint, or asks what the workflow is."
---

# Tandem sprint

One invocation of this skill is the whole SDLC loop — **refine → build → review → retro**: orient → triage, with refine filling the batch → the batch run through build → review → retro → close. The build and review passes live in `tandem-build` (`skills/tandem-build/SKILL.md`); the sprint dispatches it for its build phase. A sprint covers **exactly one project — the session's own**: work in another project is another session, run in that project's directory, on that project's lane.

The roles are the roster's (`skills/tandem-team/SKILL.md`): the session runs the sprint, the `refiner` shapes tickets, the `builder` builds, the `reviewer` gates the merge, the human answers what only a human can.

## 1. Orient

Start the session record (`AGENTS.md` — `session start`, `session link --ticket` for what this sprint carries), then walk the memory protocol (`skills/tandem-memory/SKILL.md`): `Index.md`, the project's curated note, the open threads.

## 2. Triage

**The lane is derived, never guessed.** The project is the basename of the directory the session runs in — and a worktree names its project: the `-wt-<slug>` suffix (e.g. `~/projects/tandem-wt-sprint`) strips to `tandem`. Outside a git repo the basename stands on its own. Match that name **case-insensitively** against the existing lane names — discover them with `tandem-cli ticket list --filter all --archived`, reading the `lane` values — and from then on every `--lane` carries the lane's **stored spelling**: the CLI's lane filter is an exact, case-sensitive match. Every `ticket list` read returns one JSON object, `{"tickets": […], "todos": […]}` — jq runs on `.tickets[]` (or `.todos[]`), never the bare `.[]`; the result-shape contract is pinned in `skills/tandem/SKILL.md` (§ Streams and exit codes) and is pointed at, not copied, wherever a read appears here. **No matching lane** → ask the human: `lane add` is a human call. Until it is answered, no sprint opens; the session still files its own session ticket in the default area per `AGENTS.md`.

The batch is drawn here — `tandem-cli ticket list --filter open --lane <lane> --status backlog,refined,ready` (same object shape; jq on `.tickets[]`) — **five, a cap not a quota, in stored order**. The harness shell is **zsh**, not bash: `declare -A`, `${!arr[@]}` and every other bash-only form fails with exit 1 — write multi-item shell loops as paired `id|value` strings split with POSIX parameter expansion (`${pair%%|*}` / `${pair#*|}`), never associative arrays:

- **`ready` tickets take the slots first; `backlog` tickets fill the remaining slots**, and the picked ones are refined **as part of the run**, not before the batch is open. A lane with five or more ready runs no refine phase; a lane with no ready refines five.
- **The refine rule is per ticket, unchanged** — a picked backlog ticket too big or too undecided for one session → dispatch the `refiner` (`agents/refiner.md`) with the ticket id; otherwise shape it yourself into the brief — Why / What / Done when / Decided against / Open questions — and mark it `refined`. The refine session (`skills/tandem-refine/`) stays the front door for refine-heavy work with a chosen model.
- **Ready** = refined, no open questions, no human decision pending, and it fits one agent session. Bigger → split, file the parts, link them in the description.
- **A human-only decision** → `--assignee human`, status `refined`, one question per ticket, described below.
- **Escalation scope**: a preference, a spend, or an irreversible or outward-facing act. Everything else you decide yourself and record under **Decided against** — a question the environment can answer is not a question.

## 3. Open the sprint

The batch is the slots drawn in triage — ready first, the backlog picks filling the rest. **Zero drawn** → no sprint is opened: this is a triage session, it runs no retro, and it says so in its report.

The number `<n>` is derived, never guessed: one more than the highest `n` among the lane's tickets titled `Sprint <n> — <lane>`, `1` when there are none (`--filter all`, because a closed sprint still holds its number; same object shape — jq on `.tickets[]`).

```sh
sprint=$(tandem-cli ticket add --title "Sprint $n — $lane" \
  --status in-progress --assignee self --lane "$lane" --origin self | jq -r '.id')
# batch: the slots drawn in triage — ready first, backlog picks filling the rest
```

On a host that exports no `TANDEM_AGENT`, `self` cannot resolve — pass the sprint's agent name for both `--assignee` and `--origin` (e.g. `--assignee sprint --origin sprint`), and never fall back by omitting `--origin` (the default stamps `human`).

Take the batch before anything moves it. One comment per batch ticket naming the sprint: `ticket comment add --id <id> --author sprint --body "Sprint $n — $lane: in the batch, on sprint ticket $sprint."`

The sprint ticket's description is **written once and appended to**: `## The batch` first (ids and titles in stored order, the session that opened it), then a `## Run — <date>` per landing, then `## Close`. An append is read → add → write back:

```sh
current=$(tandem-cli ticket show --id "$sprint" | jq -r '.description // ""')
printf '%s\n\n%s\n' "$current" "$section" | tandem-cli ticket set --id "$sprint" --description -
```

## 4. Build

The build phase is the `tandem-build` skill (`skills/tandem-build/SKILL.md`), run per batch ticket: **claim → build → review → record**, each ticket ending in a `## Run — <date>` entry on this sprint's description. The mode — one at a time (the default) or in parallel, the human's call before the first dispatch, recorded under `## The batch` — and the turn budget are the build skill's, with the per-ticket contracts (`agents/builder.md`, `agents/reviewer.md`). A drawn backlog ticket is refined first, in this phase, by the refine rule in §2 — then it runs the build pass like any batch ticket.

## 5. Human answers

A decision ticket's body is the channel — there is no comment field for it. The human answers in the app, from the CLI (`ticket set --description -` with the answer appended), or in chat to an agent, who appends it. The answer is its own block:

```
## Answer
B — <what was chosen, and any condition>
```

Then in one write: keep the answer, reset `assignee`, set the status the answer opens — `ready` if buildable, `backlog` if it needs re-refining, `done` if settled. Every pass starts with `ticket list --assignee human`: a human ticket never goes back to an agent unread. The read is whole-board on purpose — an answer left unread is a worse failure than a read that spans lanes — but acting on another lane's ticket belongs to that project's own sprint.

## 6. Retro

After the last `## Run` entry, dispatch the retro (`skills/retro/SKILL.md`) on the sprint session's own transcript, then write `## Close`. The close is the session's final act — nothing is dispatched after it begins — so the retro precedes it, never follows it. A zero-batch (triage-only) session opens no sprint and runs no retro. The retro's bar, categories, cap and lane are the retro skill's, unchanged: `--origin retro`, the Retro lane, capture-shaped bodies, the five-ticket cap, and a retro that files nothing is a valid result.

## 7. Close

Append `## Close` — what landed, what slipped, what waits on the human — then `tandem-cli ticket complete --id "$sprint"`. Write the close before completing: a done sprint with an empty close is a lie the next session inherits. The close is the session's final act: nothing is dispatched after it begins, so no child session can spend the turn budget the close needs. The report names `Sprint <n+1>` as the next step, opened in a **new** session — never this one.

**A dead sprint is stale work.** An `in-progress` ticket titled `Sprint <n> — <lane>` whose opening session — the one named in its `## The batch` — has ended (its session record says `ended` — `tandem-cli session show --session <id>`, the record's two states are `running` and `ended`; when the record still says `running`, the host's session list settles it: an `outcome` **and** a stale `time.updated`, both signals needed and neither exact alone) died before it could write `## Close`, and nothing on the board separates it from a sprint still running. The next sprint's triage closes it **from what the board shows**: append a `## Close` that names the recorded `## Run` entries and any escalations left with the human, says it was written by triage from the board, then `ticket complete`. It writes nothing the dead session did not leave on the board. A sprint whose session is live is left alone — the "a claim is not a stall" rule governs the borderline.

## The board

Statuses `backlog → refined → ready → in-progress → in-review → done`, one **cycle lane** per project; the store validates the value, never the transition — these gates are the convention, not the schema. A lane must exist before `--lane` accepts it; `lane add` is a human call — ask. `--lane ""` is the default area, where untriaged capture lands. The board is not due dates, priority, labels, or a lock: a human may drag anything anywhere, and the next triage reconciles it. Long-form design lives in a linked bikeshed note, never in the ticket.
