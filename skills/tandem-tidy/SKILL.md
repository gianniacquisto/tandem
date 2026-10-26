---
name: tandem-tidy
description: "Board hygiene pass — sweeps the open tickets against work that already landed, completes what is done or moot with the evidence in a comment, demotes claims whose session has ended, archives the done, and reports the oddities. Reach for it when the human says tidy or clean up the board, asks what on the board is stale, or right after a run of merges."
---

# Tandem tidy

A maintenance pass over the board, not the SDLC kickoff: it opens no sprint and dispatches no refiner — shaping unrefined tickets is the sprint's triage (`skills/tandem-sprint/SKILL.md` §2) and the refiner's (`skills/tandem-refine/`). Tidy owns one write class of its own: **complete with evidence** — a ticket already satisfied by landed work leaves the open set, its proof in a signed comment.

## 0. Orient

Start the session record (`AGENTS.md`), then read in one round:

- the open board — `tandem-cli ticket list --filter open` — every lane; the rows carry each lane's stored spelling;
- the live sessions — `tandem-cli session list --status running` — a ticket's claim is live while its claiming session runs;
- the memory protocol (`skills/tandem-memory/SKILL.md`) — `Index.md`, the project's curated note.

The sweep is board-wide: a repo fact behind another project's ticket is verified in that project's checkout — the board is shared, the evidence is local.

## 1. The liveness line

A ticket whose claiming session is still running is live: leave its status exactly as it is, sweep or no sweep. A ticket claimed by a session that has **ended** is stale work: set it back to `ready` and comment — the ended session named, the demotion dated and signed — so the next sprint's triage picks it up fresh. A sprint record is not a ticket and never appears in this sweep: its dead-sprint clause lives in `skills/tandem-sprint/SKILL.md`, and tidy leaves that write to it.

## 2. The evidence sweep

For every open ticket with no live claim — `backlog` excepted, unrefined capture being triage's input, not tidy's — check the ticket against work that already landed. The evidence sources, in the order they settle most tickets:

- the project's git log — commits and merged PRs carrying the ticket's subject;
- the board's done tickets — `--text` searches descriptions, and a done sibling often did the work;
- the repo, the app and the CLI themselves — a file that exists, a flag that is gone, a version that shipped;
- the ticket's own description — its blockers name the gates; a gate unmet means the ticket waits.

Three verdicts:

- **Resolved** — the landed work satisfies the ticket's done-when. `ticket complete --id`, after a dated, signed comment naming the commit, PR, done ticket or repo fact that settles it.
- **Moot** — the ticket's subject is gone: retired, superseded, answered another way. Same write, the comment carrying the reason instead of the evidence.
- **Unproven** — the ticket stays exactly as it is. Only what you can point at moves; a hunch completes nothing.

A ticket assigned `human` whose question the world has since answered is resolved like any other, the answer named in its comment; one still waiting on the human is the human's queue and stays.

## 3. Done housekeeping

After the sweep, `tandem-cli ticket archive` — every done ticket leaves the default views. Nothing is deleted: the archive is one flag away, and the reply names how many went.

## 4. The oddities

Report-only, each named in the reply for the human to rule on:

- **Duplicates** — two open tickets with one subject; the human keeps one.
- **A session-titled ticket** — `Session — …` on the open board violates ADR 0027 (a session is a record, never a ticket); report it, complete it only when its status is already `done`.
- **The unowned live ticket** — `in-progress` with no assignee and no session behind it; treat as a dead claim (§1) when no record claims it.
- **The gate that landed** — a ticket whose named blocker is now done or merged; it is ripe to draw and the reply says so, its status untouched.

## 5. The report

One block per lane: completed (each with its evidence), demoted, archived `<n>`, and left alone with the reason. Done when **every open ticket appears in the report** — moved with its evidence, or accounted for.
