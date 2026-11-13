---
name: tandem-tidy
description: "Board hygiene pass: sweeps the open tickets against work that already landed, completes what is done or moot with the evidence in a comment, reports stale claims, archives the done, and reports the oddities. Reach for it when the human says tidy or clean up the board, asks what on the board is stale, or right after a run of merges."
---

# Tandem tidy

A maintenance pass over the board, not the SDLC kickoff: it opens no sprint and dispatches no refiner. Shaping tickets that have no brief is the standalone refine pass (`skills/tandem-refine/`) and the refiner's (`agents/refiner.md`). Tidy owns one write class of its own: **complete with evidence**, where a ticket already satisfied by landed work leaves the open set with its proof in a signed comment.

## 0. Orient

Read in one round:

- the open board, `ticket list` with `filter` open, every lane; the rows carry each lane's stored spelling;
- each open ticket's comment log where its state is in doubt, `ticket comment list` with `id`: the log is the work record (ADR 0045);
- `git log` in the project the ticket belongs to.

The sweep is board-wide: a repo fact behind another project's ticket is verified in that project's checkout, because the board is shared and the evidence is local.

## 1. The stale claim

A claim is live while its comment log shows a recent handoff in flight. A ticket whose log records its pass's end without a landing, or that sits `in-progress` with no PR in the log, is **stale work**: report it, and set it back to `ready` with a dated, signed comment naming the evidence, so the next sprint's triage picks it up fresh.

This is the same rule the sprint's close runs over its own lane before the close is written (`skills/tandem-sprint/SKILL.md` §5), so the two agree by construction.

## 2. The evidence sweep

For every open ticket with no live claim, `backlog` excepted (a ticket still missing its brief is triage's input, not tidy's), check the ticket against work that already landed. The evidence sources, in the order they settle most tickets:

- the project's git log: commits and merged PRs carrying the ticket's subject;
- the board's done tickets: `--text` searches descriptions, and a done sibling often did the work;
- the repo and the CLI themselves: a file that exists, a flag that is gone, a version that shipped;
- the ticket's own description: its blockers name the gates, and a gate unmet means the ticket waits.

Three verdicts:

- **Resolved**: the landed work satisfies the ticket's done-when. `ticket complete` with `id`, after a dated, signed comment naming the commit, PR, done ticket or repo fact that settles it.
- **Moot**: the ticket's subject is gone, retired, superseded or answered another way. Same write, the comment carrying the reason instead of the evidence.
- **Unproven**: the ticket stays exactly as it is. Only what you can point at moves; a hunch completes nothing.

A ticket assigned `human` whose question the world has since answered is resolved like any other, the answer named in its comment; one still waiting on the human is the human's queue and stays.

**An escalated merge is not waiting.** An `in-review` ticket assigned `human` whose comment log names a PR is settled by the PR itself: read `gh pr view <n> --json state,mergedAt,mergeCommit`; `MERGED` means **Resolved**, the merge commit in a signed comment, because the human's act already happened outside any pass. Still open means the ticket stays the human's queue, untouched.

## 3. Done housekeeping

After the sweep, `ticket archive` with no `id`: every done ticket leaves the default views. Nothing is deleted: the archive is one flag away, and the reply names how many went.

## 4. The oddities

Report-only, each named in the reply for the human to rule on:

- **Duplicates**: two open tickets with one subject; the human keeps one.
- **The unowned claim**: `in-progress` with no assignee and no PR in the log; treat as a stale claim (§1) when nothing in the log claims it.
- **The gate that landed**: a ticket whose named blocker is now done or merged; it is ripe to draw and the reply says so, its status untouched.
- **A lifted block**: a ticket whose `blockedBy` names an edge whose blocker is `done` (the derived `blocked` is already false, so the draw takes it; the edge reads as history). Report it and lift it with `ticket unblock` carrying `id` and `on` the blocker, so the face stops naming a wait that is over. Never lift one whose blocker is still open: that is a block, not an oddity.

## 5. The report

One block per lane: completed (each with its evidence), stale claims reported, archived `<n>`, and left alone with the reason. Done when **every open ticket appears in the report**, moved with its evidence or accounted for.
