---
name: tandem-sprint
description: "Triage, build, review, close in one invocation — orient, draw ready only, run the batch through `tandem-build`, close. Refine and retro are standalone passes: shaping is `skills/tandem-refine/`, learning is `skills/retro/`. Reach for it when a coding session starts, when the human says run a sprint, or asks what the workflow is."
---

# Tandem sprint

One invocation of this skill is the build-and-review loop — **triage → build → review → close**: orient → triage drawing `ready` only → the batch run through build → review → close. Refine and retro are standalone passes, not phases here: shaping is the refine pass (`skills/tandem-refine/SKILL.md`), and retro (`skills/retro/SKILL.md`) is a standalone manual pass the human invokes on a named session, while the sprint neither runs, dispatches, nor prompts one. The build and review passes live in `tandem-build` (`skills/tandem-build/SKILL.md`); the sprint dispatches it for its build phase. A sprint covers **exactly one project — the session's own**: work in another project is another session, run in that project's directory, on that project's lane.

The roles are the roster's (`skills/tandem-team/SKILL.md`): the session runs the sprint, the `builder` builds, the `reviewer` gates the merge, the human answers what only a human can. The `refiner` shapes tickets in the standalone refine pass (`skills/tandem-refine/SKILL.md`), outside the sprint.

## 1. Orient

Start the session record (`AGENTS.md` — `session start --agent sprint`), then walk the memory protocol (`skills/tandem-memory/SKILL.md`): `Index.md`, the project's **Overview**, the open threads. The sprint does not link its tickets by hand here: §3's batch write carries them.

## 2. Triage

**The lane is derived, never guessed.** The project is the basename of the directory the session runs in — and a worktree names its project: the `-wt-<slug>` suffix (e.g. `~/projects/tandem-wt-sprint`) strips to `tandem`. Outside a git repo the basename stands on its own. Match that name **case-insensitively** against the existing lane names, discovered with `lane_list` (one small object, never the whole board), and from then on every `lane` carries the lane's **stored spelling**: the lane filter is an exact, case-sensitive match. **A human-named lane overrides the derivation.** When the human names the lane this run works (a retro-fix sprint in Retro, say), the batch draws from that lane and the sprint record carries it: `lane` is that name's stored spelling everywhere, and the directory bounds the work, not the lane. Every `ticket_list` read returns one JSON object — the rows are read directly, never through `jq`; the result-shape contract is pinned in `skills/tandem/SKILL.md` (§ Streams and exit codes) and is pointed at, not copied, wherever a read appears here. **No matching lane** → ask the human: `lane_add` is a human call. Until it is answered, no sprint opens; the session still starts its record, `session_start` with `agent` sprint (§1), so the pass leaves a work record even when no sprint runs.

The batch is drawn here — `ticket_list` with `filter` open, the lane, and `status` `ready` (same object shape) — **five, a cap not a quota, in stored order**. The board calls go through the tools now, so the shell's quoting traps no longer apply to them: no `jq`, no associative-array loops, no `echo ===` separator to misquote. The harness shell is still **zsh**, not bash, for what remains shell (git, file reads): `declare -A`, `${!arr[@]}` and every other bash-only form fails with exit 1. The `tandem-cli` spellings beside each tool call are the fallback the skill documents, and only there do the old shell rules apply:

- **Triage draws `ready` only**: five, a cap not a quota, in stored order. `backlog` is never drawn, shaped, promoted, or dispatched here: the sprint shapes, promotes, and dispatches nothing. Shaping is the standalone refine pass's work (`skills/tandem-refine/SKILL.md`), and only refine moves a ticket to `ready`. `on-hold` is never drawn: it is parked work, waiting on a person or a blocker, not a slot.
- **A blocked row is skipped whatever its status**, because the edge, not the column, is the block (ADR 0035). The draw keeps only rows whose `blocked` is false. **The skip is reported, not silent**: triage prints one line per skipped row, `skipped TAN-217: blocked by TAN-210 (on-hold)`, and the same line goes into the sprint's first `sprint run` note, so the record carries it after the transcript is gone. The skip is not an error (exit 0), and the batch is short by however many were skipped. A blocked ticket is not reordered, re-parented or unblocked here: `ticket_unblock` is a write for the session that finds the block gone, or for `tandem-tidy` (`skills/tandem-tidy/SKILL.md`).
- **Backlog is reported, never drawn**: triage prints one line per backlog row: `TAN-217 needs a standalone refine pass`, and the same line goes into the sprint's first `sprint run` note, mirroring the blocked-row skip reporting above, so the record carries it after the transcript is gone. A backlog ticket is not reordered, promoted, or dispatched here: shaping is the standalone refine pass's work.
- **Ready** = a brief with no open questions, no human decision pending, and it fits one agent session. Bigger → split, file the parts, link them in the description.
- **A human-only decision** → `--assignee human`, status `on-hold`, one question per ticket, described below. That pair is a hold with two shapes: the refine-time hold, a human-only decision before any build exists, and refine's park (`agents/refiner.md`), a brief waiting on the human's answers. Neither is an escalation: that one is `in-review` with `human` (`agents/reviewer.md`), and the three must not be read for each other.
- **Escalation scope**: a preference, a spend, or an irreversible or outward-facing act. Everything else you decide yourself and record under **Decided against** — a question the environment can answer is not a question.
- **A decision worth keeping gets its capture here.** Triage is where the run decides: a blocked row skipped, backlog rows reported, a lane name settled, a scope call. File one capture at the moment the decision is made, `note_capture` with kind `decision` and the one-to-three-line body; the bar is `AGENTS.md`'s (a real re-derivation cost, and not already a session record, a ticket, an ADR or the code), so a triage that decides nothing worth keeping files none.

## 3. Open the sprint

The batch is the `ready` slots drawn in triage. **Zero drawn** → no sprint is opened: this is a triage session, and it says so in its report.

The record derives the number, never the session: `sprint open` answers `1` on a lane with no records and one more than the lane's highest number otherwise, and a second open on a lane whose sprint is still open returns that record unchanged — a re-run of this step cannot fork the sprint. Pass the lane's stored spelling, as §2 discovered it.

```sh
lane="Tandem"                                    # the stored spelling, from triage
tandem-cli sprint open --lane "$lane" --mode serial
for id in <the slots drawn in triage>; do
  tandem-cli sprint ticket --lane "$lane" --ticket "$id"
done
```

`--mode parallel` is the human's call before the first dispatch; the record stores it. The open replies with the record: its `number` is the handle's second half, and `tandem-cli sprint show --lane "$lane" --number <n>` reads it back later. The opening session is the session record `sprint open` resolves (`--session`, else `$OPENCODE_SESSION_ID` at open time, the way `session end` reads it) — that record id, not the harness pointer and not a title, is what the dead-sprint rule reads.

`sprint ticket` writes the batch entry and the session link together: each drawn ticket is appended to the opening session's record in the same transaction, so the sprint links nothing by hand and `sprint open` plus one `sprint ticket` per drawn slot is the whole opening act.

The batch lives on the record, in stored order, and nowhere else. The batch comment that named the sprint on each ticket is gone: the record is the machine truth, and a comment would be a second copy of it.

## 4. Build

The build phase is the `tandem-build` skill (`skills/tandem-build/SKILL.md`), run per batch ticket: **claim → build → review → record**, each ticket ending in a `sprint run` entry on this sprint's record — `tandem-cli sprint run --lane "$lane" --note -`, dated by the core. The mode — one at a time (the default) or in parallel, the human's call before the first dispatch, chosen at `sprint open --mode` and stored on the record — and the turn budget are the build skill's, with the per-ticket contracts (`agents/builder.md`, `agents/reviewer.md`). Every batch ticket is already `ready`: shaping happened in the standalone refine pass before the sprint. **Both handoffs are per ticket, and neither is a batch gate**: a build dispatches when its own ticket is drawn, and a review dispatches when its own PR is ready, first-come, so a slow build holds its own review only. The batch is a set of independent tickets, not a unit: nothing in either handoff counts what else is still moving.

## 5. Human answers

A hold closes via a comment plus `set`: the human decides in the app, from the CLI, or in chat to an agent, who records the decision in the comment log and moves the pair with it:

```sh
tandem-cli ticket comment add --id "$id" --author <role> --body "B: <what was chosen, and any condition>"
tandem-cli ticket set --id "$id" --status ready --assignee ""
```

`--status` is the status the decision earns: `ready` if buildable, `backlog` if it needs re-refining, `done` if settled, and `--assignee` is the next owner, omitted (or `""`) when nobody owns it yet. The inbox is the assignee alone, with no Answer-block check.

**Every pass starts with `ticket list --assignee human`.** That read is the queue, whole-board on purpose: a decision left unread is a worse failure than a read that spans lanes, but acting on another lane's ticket belongs to that project's own sprint.

**An escalated merge is already the human's act.** An `in-review` ticket assigned `human` whose comment log names a PR has been merged by the human outside any pass — `gh pr view <n> --json state,mergedAt,mergeCommit` answers `MERGED` — so the pass completes it: `ticket complete --id <id>` with the merge commit in a signed comment. A PR still open leaves the ticket in the human's queue, untouched.

## 6. Close

**The precondition: every batch ticket is merged and `done`, with its PR in the log.** No close runs on a batch still in flight: each batch ticket's PR has merged into `main`, and the ticket is back on the board as `done`. An escalation to the human does not lift it: the sprint stays open and waits on the human, and the close comes after the merge and the board write, never before. A batch ticket that did not finish is a dead sprint's to settle, not a live close's.

**The lane's open set is settled before the close is written.** Walk every open ticket in the lane (`tandem-cli ticket list --lane "$lane" --filter open`) and leave each in a status a reader can trust: a claim whose session has ended goes back to `ready`, or to `on-hold` when it is parked on purpose, the move dated and signed in a comment naming the ended session; a ticket this sprint drew but did not finish says so in a comment, instead of sitting `in-progress` behind a dead claim; an `in-review` ticket keeps its PR number in the log, so the merge or the escalation is the only move outstanding; `human` stays only where a question is actually open, and refine's park is one (`agents/refiner.md`): it closes with a comment plus `ticket set`, never with a `ticket set` that would clear the owner and swallow the window without the comment. One comment per ticket the close moves, none where nothing moved, signed as the role that owned the pass (`builder`, `reviewer`) when it reports that role's work and as `sprint` when the sprint settles a claim it inherited; a pass that resumes its own ticket reopens it rather than commenting on itself. The close note names what was settled, so the record shows the sweep and the next session does not redo it.

**A renderer ticket's deployed worktree is reaped here.** A renderer diff leaves its worktree in place for the human's test and sign-off (`agents/builder.md`); once its ticket is merged, the close removes it (`git worktree remove <path>`), so the deployed scratch home and worktree do not outlive the batch. The worktree path is on the ticket's `## Test this head` comment.

**The close cuts no release by default.** No tag, no release notes, no cask bump: a release cuts only on the human's explicit per-sprint words naming the version, and the sprint never prompts for one. When the human asks, the manual steps are today's, from a checkout on the landed `main` head: tag it and push the tag (`git tag vX.Y.Z origin/main && git push origin vX.Y.Z`, a minor bump unless the batch is fixes only), watch the [Release workflow](https://github.com/gianniacquisto/tandem/blob/main/.github/workflows/release.yml) until the GitHub release publishes, then watch its tap step to green. A red tap step means the secret is missing or expired (it names `TAP_TOKEN`); the fix is `gh secret set TAP_TOKEN` (`docs/releasing.md` step 5), never a hand bump of the cask. Then file the notes from a checkout with the tags (`Scripts/release-notes.sh vX.Y.Z --capture`), so they reach the bikeshed. A sprint the human asked no release of cuts no tag, and the close says so in one line. The close note names a tag only when the human asked for one.

`sprint close --lane "$lane" --note -` — what landed, what slipped, what waits on the human. Write the close before closing the record: a closed sprint with no close is a lie the next session inherits, and the stamp is once, so a second close keeps the first text. The close is the session's final act: nothing is dispatched after it begins, so no child session can spend the turn budget the close needs. The report names the next sprint as the next step, opened in a **new** session — never this one.

**A dead sprint is stale work.** A record still `open` — `tandem-cli sprint list --status open` — whose opening session has ended (its session record says `ended` — `tandem-cli session show --session <id>`, the record's two states are `running` and `ended`; when the record still says `running`, the host's session list settles it: an `outcome` **and** a stale `time.updated`, both signals needed and neither exact alone) died before it could close, and nothing on the board separates it from a sprint still running. The next sprint's triage closes it **from what the record shows**: `tandem-cli sprint close --lane <lane> --note -` with a close that names the recorded `sprint run` entries and any escalations left with the human, and says it was written by triage from the record. It writes nothing the dead session did not leave on the record. A sprint whose session is live is left alone — the "a claim is not a stall" rule governs the borderline.

## The board

Statuses `backlog → ready → in-progress → in-review → done`, with `on-hold` the parked slot beside that line (a blocker, a human decision pending); several **cycle lanes** per project with one parent each; the store validates the value, never the transition — these gates are the convention, not the schema. A lane must exist before `--lane` accepts it; `lane add` is a human call — ask. `--lane ""` is the default area, where untriaged capture lands. The board is not due dates, priority, labels, or a lock: a human may drag anything anywhere, and the next triage reconciles it. Long-form design lives in a linked bikeshed note, never in the ticket.
