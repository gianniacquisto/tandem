---
name: tandem-sprint
description: "Triage, build, review and close a batch of tickets in one invocation: orient on the board, draw ready tickets only, run each through `tandem-build`, and settle the lane at the close. Refine and retro are standalone passes: shaping is `skills/tandem-refine/`, learning is `skills/retro/`. Reach for it when the user says run a sprint, asks what the workflow is, or starts a coding pass on this board."
---

# Tandem sprint

One invocation is the build-and-review loop: **triage → build → review → close**. A sprint is a human's batch, not a stored entity: the board's statuses, assignees and comment logs are its record (ADR 0045). Refine and retro are standalone passes, not phases here, and the sprint neither runs nor prompts them.

A sprint covers **exactly one project, the one whose directory the pass runs in**. The project is the directory's basename with a `-wt-<slug>` worktree suffix stripped, matched case-insensitively against the existing lane names. A human-named lane overrides the derivation; the lane's stored spelling is used everywhere after that. No matching lane: ask the human, because `lane add` is a human call.

## 1. Orient

Read the board in one round: the project's lanes, the open tickets, and the tickets assigned to `human`. That read is the queue; acting on another project's ticket belongs to that project's own sprint.

## 2. Triage

Draw **`ready` only**: five, a cap not a quota, in stored order.

- **A blocked row is skipped whatever its status**, because the edge, not the column, is the block (ADR 0035). Keep only rows whose `blocked` is false, and report each skip with one line naming the blocker.
- **Backlog is reported, never drawn**: one line per row, `TAN-217 needs a standalone refine pass`. Only the refine pass moves a ticket to `ready`.
- **`on-hold` is never drawn**: it is parked work, waiting on a person or a blocker.
- **Ready** means a brief with no open questions, no human decision pending, and it fits one pass. Bigger means split it and file the parts.

A decision worth keeping is a ticket comment on the work it belongs to, or a new `backlog` ticket. There is no capture layer.

## 3. Open the batch

Zero drawn means no batch: this is a triage pass, and its report says so. Otherwise the batch is the drawn tickets, claimed as they are dispatched: `ticket set` with `status` in-progress and `assignee` the pass. The tickets are the record; nothing is opened on the lane.

## 4. Build

Run the batch through `tandem-build` (`skills/tandem-build/SKILL.md`), per ticket: claim, build in its own worktree, review through the gate, merge. One at a time is the default; parallel is the human's call before the first dispatch. Every batch ticket is already `ready`: shaping happened in the standalone refine pass.

## 5. Close

**The precondition: every batch ticket is merged and `done`, with its PR in the comment log.** No close runs on a batch still in flight. An escalation to the human keeps the ticket in the batch until the human answers and the work lands.

Settle the lane's open set before the close is written. Walk every open ticket in the lane and leave each in a status a reader can trust:

- a claim whose pass has ended goes back to `ready`, or to `on-hold` when it is parked on purpose, with a comment naming the ended pass;
- a ticket drawn but not finished says so in a comment instead of sitting `in-progress` behind a dead claim;
- an `in-review` ticket keeps its PR number in the log;
- `human` stays only where a question is actually open, closed with a comment and a status move in the same act.

One comment per ticket the close moves, none where nothing moved, signed as the role that owned the pass. The close report names what landed, what slipped and what waits on the human.

**The close cuts no release by default.** A release cuts only on the human's explicit words naming the version, and the sprint never prompts for one. When the human asks, follow `docs/releasing.md`: tag the landed `main` head, push the tag, and watch the Release workflow publish the three platform archives, `install.sh` and `SHA256SUMS`. Generate the release body with `Scripts/release-notes.sh vX.Y.Z`; it writes nothing else. A sprint the human asked no release of cuts no tag, and the close says so in one line.

**A dead claim is stale work.** A ticket in-progress whose pass ended is reclaimed by the next triage: back to `ready`, owner cleared, a loud comment, then a fresh claim. There is no session table to read; the comment log is the evidence.
