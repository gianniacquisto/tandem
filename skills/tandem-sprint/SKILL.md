---
name: tandem-sprint
description: "The SDLC in one invocation — the whole loop, refine → build → review → retro: orient, triage the lane with refine filling the batch, run the batch through `tandem-build`, retro at the close. Reach for it when a coding session starts, when the human says run a sprint, or asks what the workflow is."
---

# Tandem sprint

One invocation of this skill is the whole SDLC loop — **refine → build → review → retro**: orient → triage, with refine filling the batch → the batch run through build → review → retro → close. The build and review passes live in `tandem-build` (`skills/tandem-build/SKILL.md`); the sprint dispatches it for its build phase. A sprint covers **exactly one project — the session's own**: work in another project is another session, run in that project's directory, on that project's lane.

The roles are the roster's (`skills/tandem-team/SKILL.md`): the session runs the sprint, the `refiner` shapes tickets, the `builder` builds, the `reviewer` gates the merge, the human answers what only a human can.

## 1. Orient

Start the session record (`AGENTS.md` — `session start --agent sprint`, `session link --ticket` for what this sprint carries), then walk the memory protocol (`skills/tandem-memory/SKILL.md`): `Index.md`, the project's **Overview**, the open threads.

## 2. Triage

**The lane is derived, never guessed.** The project is the basename of the directory the session runs in — and a worktree names its project: the `-wt-<slug>` suffix (e.g. `~/projects/tandem-wt-sprint`) strips to `tandem`. Outside a git repo the basename stands on its own. Match that name **case-insensitively** against the existing lane names — discover them with `tandem-cli ticket list --filter all --archived`, reading the `lane` values — and from then on every `--lane` carries the lane's **stored spelling**: the CLI's lane filter is an exact, case-sensitive match. Every `ticket list` read returns one JSON object, `{"tickets": […], "todos": […]}` — jq runs on `.tickets[]` (or `.todos[]`), never the bare `.[]`; the result-shape contract is pinned in `skills/tandem/SKILL.md` (§ Streams and exit codes) and is pointed at, not copied, wherever a read appears here. **No matching lane** → ask the human: `lane add` is a human call. Until it is answered, no sprint opens; the session still files its own session ticket in the default area per `AGENTS.md`.

The batch is drawn here — `tandem-cli ticket list --filter open --lane <lane> --status ready,backlog` (same object shape; jq on `.tickets[]`) — **five, a cap not a quota, in stored order**. The harness shell is **zsh**, not bash: `declare -A`, `${!arr[@]}` and every other bash-only form fails with exit 1 — write multi-item shell loops as paired `id|value` strings split with POSIX parameter expansion (`${pair%%|*}` / `${pair#*|}`), never associative arrays:

- **`ready` tickets take the slots first; `backlog` tickets fill the remaining slots**, and the picked ones are shaped into briefs **as part of the run**, not before the batch is open. A lane with five or more ready runs no refine phase; a lane with no ready refines five. `on-hold` is never drawn: it is parked work, waiting on a person or a blocker, not a slot.
- **The refine rule is per ticket, unchanged** — a picked backlog ticket too big or too undecided for one session → dispatch the `refiner` (`agents/refiner.md`) with the ticket id; otherwise shape it yourself into the brief — Why / What / Done when / Decided against / Open questions — and land it at the status its brief earns: **`ready`** when no question is open and the human owes no decision, **`on-hold`** when it is parked on purpose. The refine session (`skills/tandem-refine/`) stays the front door for refine-heavy work with a chosen model.
- **Ready** = a brief with no open questions, no human decision pending, and it fits one agent session. Bigger → split, file the parts, link them in the description.
- **A human-only decision** → `--assignee human`, status `on-hold`, one question per ticket, described below. That pair is the refine-time hold, before any build exists; an escalated PR is `in-review` with `human` (`agents/reviewer.md`), and the two must not be read for each other.
- **The promotion rule keeps `ready` fed.** Every open ticket in the lane is read on the way in, drawn or not: one whose description is already a finished brief, with no open question and no `human` hold, is promoted to `ready` in the same pass. Only a ticket still missing its brief stays `backlog`, and only a ticket parked on purpose stays `on-hold`; nobody has to remember to move it later.
- **Escalation scope**: a preference, a spend, or an irreversible or outward-facing act. Everything else you decide yourself and record under **Decided against** — a question the environment can answer is not a question.

## 3. Open the sprint

The batch is the slots drawn in triage — ready first, the backlog picks filling the rest. **Zero drawn** → no sprint is opened: this is a triage session, it runs no retro, and it says so in its report.

The record derives the number, never the session: `sprint open` answers `1` on a lane with no records and one more than the lane's highest number otherwise, and a second open on a lane whose sprint is still open returns that record unchanged — a re-run of this step cannot fork the sprint. Pass the lane's stored spelling, as §2 discovered it.

```sh
lane="Tandem"                                    # the stored spelling, from triage
tandem-cli sprint open --lane "$lane" --mode serial
for id in <the slots drawn in triage>; do
  tandem-cli sprint ticket --lane "$lane" --ticket "$id"
done
```

`--mode parallel` is the human's call before the first dispatch; the record stores it. The open replies with the record: its `number` is the handle's second half, and `tandem-cli sprint show --lane "$lane" --number <n>` reads it back later. The opening session is `$OPENCODE_SESSION_ID`, captured by `sprint open` — that id, not a title, is what the dead-sprint rule reads.

The batch lives on the record, in stored order, and nowhere else. The batch comment that named the sprint on each ticket is gone: the record is the machine truth, and a comment would be a second copy of it.

## 4. Build

The build phase is the `tandem-build` skill (`skills/tandem-build/SKILL.md`), run per batch ticket: **claim → build → review → record**, each ticket ending in a `sprint run` entry on this sprint's record — `tandem-cli sprint run --lane "$lane" --note -`, dated by the core. The mode — one at a time (the default) or in parallel, the human's call before the first dispatch, chosen at `sprint open --mode` and stored on the record — and the turn budget are the build skill's, with the per-ticket contracts (`agents/builder.md`, `agents/reviewer.md`). A drawn backlog ticket is shaped into a brief first, in this phase, by the refine rule in §2: it lands `ready`, or `on-hold` when it is parked, and then it runs the build pass like any batch ticket.

## 5. Human answers

A decision ticket's body is the channel — there is no comment field for it. The human answers in the app, from the CLI, or in chat to an agent, who records it. The answer is its own block, and one verb appends it and moves the pair with it:

```sh
tandem-cli ticket answer --id "$id" --status ready --assignee "" \
  --body "B — <what was chosen, and any condition>"
```

`--status` is the status the answer opens — `ready` if buildable, `backlog` if it needs re-refining, `done` if settled — and `--assignee` is the next owner, omitted (or `""`) when nobody owns it yet. The block and the pair are one write: an answered ticket cannot sit in the inbox wearing `human`.

**Every pass starts with `ticket list --assignee human`.** That read still lists answered tickets, and that is the bookkeeping debt: a ticket carrying a `## Answer` block while `human` still owns it is answered but unsettled, and the pass that finds one owes it an `answer` call (the next owner is the pass's own judgment; `""` when there is none). The read is whole-board on purpose — an answer left unread is a worse failure than a read that spans lanes — but acting on another lane's ticket belongs to that project's own sprint.

**An escalated merge is already the human's act.** An `in-review` ticket assigned `human` whose comment log names a PR has been merged by the human outside any pass — `gh pr view <n> --json state,mergedAt,mergeCommit` answers `MERGED` — so the pass completes it: `ticket complete --id <id>` with the merge commit in a signed comment. A PR still open leaves the ticket in the human's queue, untouched.

## 6. Retro

After the last `sprint run` entry, dispatch the retro (`skills/retro/SKILL.md`) on the sprint session's own transcript, then close the record (§7). The close is the session's final act — nothing is dispatched after it begins — so the retro precedes it, never follows it. A zero-batch (triage-only) session opens no sprint and runs no retro. The retro's bar, categories, cap and lane are the retro skill's, unchanged: `--origin retro`, the Retro lane, capture-shaped bodies, the five-ticket cap, and a retro that files nothing is a valid result.

## 7. Close

**The precondition: every batch ticket is merged and `done`, with its PR in the log.** No close runs on a batch still in flight: each batch ticket's PR has merged into `main`, and the ticket is back on the board as `done`. An escalation to the human does not lift it: the sprint stays open and waits on the human, and the close comes after the merge and the board write, never before. A batch ticket that did not finish is a dead sprint's to settle, not a live close's.

**The lane's open set is settled before the close is written.** Walk every open ticket in the lane (`tandem-cli ticket list --lane "$lane" --filter open`) and leave each in a status a reader can trust: a claim whose session has ended goes back to `ready`, or to `on-hold` when it is parked on purpose, the move dated and signed in a comment naming the ended session; a ticket this sprint drew but did not finish says so in a comment, instead of sitting `in-progress` behind a dead claim; an `in-review` ticket keeps its PR number in the log, so the merge or the escalation is the only move outstanding; `human` stays only where a question is actually open. One comment per ticket the close moves, none where nothing moved, signed as the role that owned the pass (`builder`, `reviewer`) when it reports that role's work and as `sprint` when the sprint settles a claim it inherited; a pass that resumes its own ticket reopens it rather than commenting on itself. The close note names what was settled, so the record shows the sweep and the next session does not redo it.

**Cut the release here, because a tag can only be cut once the batch is merged and done.** The sprint that lands the work is the pass that ships it. From a checkout on the landed `main` head, tag it and push the tag: `git tag vX.Y.Z origin/main && git push origin vX.Y.Z`, a minor bump unless the batch is fixes only, and watch the [Release workflow](https://github.com/gianniacquisto/tandem/blob/main/.github/workflows/release.yml) until the GitHub release publishes. The tag is the release: `main` takes the merges, so `main`'s head is exactly what ships, and there is no release line to move. File the notes from a checkout with the tags (`Scripts/release-notes.sh vX.Y.Z --capture`), so they reach the bikeshed. The tap bump is the one step this close cannot take while it is a hand act: the human bumps the cask (`docs/releasing.md`), and this paragraph stops naming a hand act once CD ships with `TAP_TOKEN` (TAN-224), when the tag alone ships it. A sprint that landed nothing cuts no tag, and the close says so in one line. The close note names the tag its batch shipped in.

`sprint close --lane "$lane" --note -` — what landed, what slipped, what waits on the human. Write the close before closing the record: a closed sprint with no close is a lie the next session inherits, and the stamp is once, so a second close keeps the first text. The close is the session's final act: nothing is dispatched after it begins, so no child session can spend the turn budget the close needs. The report names the next sprint as the next step, opened in a **new** session — never this one.

**A dead sprint is stale work.** A record still `open` — `tandem-cli sprint list --status open` — whose opening session has ended (its session record says `ended` — `tandem-cli session show --session <id>`, the record's two states are `running` and `ended`; when the record still says `running`, the host's session list settles it: an `outcome` **and** a stale `time.updated`, both signals needed and neither exact alone) died before it could close, and nothing on the board separates it from a sprint still running. The next sprint's triage closes it **from what the record shows**: `tandem-cli sprint close --lane <lane> --note -` with a close that names the recorded `sprint run` entries and any escalations left with the human, and says it was written by triage from the record. It writes nothing the dead session did not leave on the record. A sprint whose session is live is left alone — the "a claim is not a stall" rule governs the borderline.

## The board

Statuses `backlog → ready → in-progress → in-review → done`, with `on-hold` the parked slot beside that line (a blocker, a human decision pending); one **cycle lane** per project; the store validates the value, never the transition — these gates are the convention, not the schema. A lane must exist before `--lane` accepts it; `lane add` is a human call — ask. `--lane ""` is the default area, where untriaged capture lands. The board is not due dates, priority, labels, or a lock: a human may drag anything anywhere, and the next triage reconciles it. Long-form design lives in a linked bikeshed note, never in the ticket.
