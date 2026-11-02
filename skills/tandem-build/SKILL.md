---
name: tandem-build
description: "The sprint's build phase — per batch ticket: claim, build in its own worktree, review through the gate, record. The sprint (`skills/tandem-sprint/SKILL.md`) runs this skill for its build phase; readable on its own for the build and review contracts."
---

# Tandem build

The build phase of the sprint loop (`skills/tandem-sprint/SKILL.md`): the batch is drawn and the sprint record is open — this runs each batch ticket through build → review → merge. The behavior is the build passes the sprint skill carried before the 2026-10-24 rename; nothing new.

The roles are the roster's (`skills/tandem-team/SKILL.md`): the `builder` builds, the `reviewer` gates the merge, the human answers what only a human can.

## The mode

The human in the session picks the mode before the first dispatch — **one at a time** (the default) or **in parallel** — and the answer is handed to `sprint open --mode serial|parallel` (the sprint skill §3); the record stores it, so nothing is written under a description heading.

**One at a time** — the default — runs the per-ticket pass below end to end before the next claim.

**In parallel** — the human's call — dispatches every batch ticket's build at once, each in its own worktree on its own branch, and from there each ticket flows on its own. A review dispatches when its own PR is ready, first-come: the batch's order is not the gate's, and one ticket's build does not delay another ticket's review, so a fast ticket's merge is not held by the batch's slowest build. Reviews still gate every merge; parallel changes what builds at once, never what merges unreviewed.

**Why serial is the default.** The 2026-10-20 collision, nine branches sharing one checkout of this repo, `BikeFace.swift` alone touched by six, is what builds look like without worktree isolation; it stands as history, the failure isolation ends. With every build in its own worktree, merge safety no longer argues for serial: the default rests on turn budget and simplicity, one dispatch at a time keeps each turn's cost legible. Velocity is tickets per sprint, not concurrency. A ticket that escalates leaves the batch, but the sprint waits for it; the close comes after the merge and the board write.

## The per-ticket pass

0. **Grill**: a batch ticket with no `## Grill` comment gets the grill first, and the build after it as a second dispatch: read the log (`tandem-cli ticket comment list --id <id>`), and when no comment is headed `## Grill`, dispatch the `reviewer` subagent (`agents/reviewer.md`) with the ticket id and no PR. It posts the comment and parks the ticket `on-hold` / `human`, which ends this ticket's turn in the batch until the human's write resumes it `ready`. Never both at once: the grill parks a ticket the build would claim, and the park and the claim in one turn is a ticket in two states. The grill and the build are two dispatches, always.
1. **Claim** — `tandem-cli ticket set --id <id> --status in-progress --assignee builder`.
2. **Build**: dispatch the `builder` subagent (`agents/builder.md`) with the ticket id and its description (`tandem-cli ticket show --id <id>`, since `list` rows are summaries without the body), its comment log (`tandem-cli ticket comment list --id <id>`: the `## Grill` comment and every comment the human's window added, which the brief names as part of the builder's input), and the instruction to start its session record before its first tool call (`tandem-cli session start`, then `tandem-cli session link --ticket <id>`): a child with no record leaves `session end` answering `Unknown session id` and the ticket carries no usage report. The build runs in the builder's own git worktree, branched fresh from `origin/main`, serial or parallel alike, so no two builders ever share a working tree and the host's checkout stays on `main` until the merges land. The builder verifies, opens the PR with its `## Merge Danger` claim, and ends its session record with a summary: the record is the work record, and a capture is filed only for a real cost of re-derivation. A dispatch that fails, or whose child has ended (`opencode api session.list`: a non-null `outcome` settles liveness, and the tandem record stays `running` by design), leaves a dead claim, not a done ticket: reclaim per the dead-claim rule (status `ready`, owner cleared, a loud comment, re-claim, then the new dispatch), reusing the dead branch only if it holds no commits, and removing the dead worktree.
3. **Review**: the gate needs two facts, and only its own ticket supplies either: a green head, and a branch not behind its base. Neither reads another ticket: no batch position, no sibling build, no count of what is still running. First check the branch against its base: `git rev-list --left-right --count origin/main...origin/<branch>`. A branch that went stale while the gate queued would cost the pass its whole read before it learns it cannot merge; behind, bring the branch up to date (`gh pr update-branch <n>`) and name the new base head in the brief. A rebase can auto-merge the symbol the base renamed away (`todo.` / `todo.id` against the `Ticket` rename, for one); before the push, grep the rebased diff for it (`git diff origin/main...HEAD | grep <old-symbol>`) and stop before the push if it finds one, naming the line. Then dispatch the `reviewer` subagent (`agents/reviewer.md`) with the ticket id and PR. It merges a two-way/low door and completes the ticket; a one-way door is escalated to the human and **leaves the batch**, but the sprint waits for it: the close comes after the merge and the board write. An `escalated` report is accepted only after you read the ticket yourself (`tandem-cli ticket show --id <id>`): status `in-review` and assignee `human` mean handed over; anything else is a failed gate: re-dispatch the reviewer, or do the handover in this session and say so in the run note.
4. **Record** — `tandem-cli sprint run --lane "$lane" --note -` on the sprint's record: the ticket, what landed, the PR. The core dates the entry.

**The gate brief carries only what the gate cannot look up.** The contract's path (`agents/reviewer.md`), the ticket id, the PR number, the base branch, and the pre-flight the gate needs — whether the branch is behind its base. At most one clause may name what is not the gate's to judge; nothing more. A PR that has been gated before is not re-briefed with a history of its passes: what each earlier pass established goes on the PR as a review comment (`gh pr comment`), and the brief points at it in one line — "earlier passes are on the PR; read the head on its own." The verdict comes from the gate's own reading of the head, never from the brief's summary of what earlier passes found — a brief that retells prior passes anchors the fresh reading it asks for, and its cost is paid again on every re-gate.

## Budget the turns

A turn re-reads the whole window, so a session's bill is its turn count times its context — the same rule the terse style states (`terse/TERSE.md`, Budget). One turn per dispatch, with the calls it needs in it: gather every fact the brief needs in one message — a turn that needs three facts fetches three facts in one message, and a loop over a list is one call, not N. Watch it drift: calls per turn is countable from the host's store, `~/.local/share/opencode/opencode.db` — read-only:

```sh
sqlite3 "file:$HOME/.local/share/opencode/opencode.db?mode=ro" '
WITH turns AS (SELECT session_id, COUNT(*) n FROM session_message
               WHERE type="assistant" GROUP BY session_id),
     calls AS (SELECT m.session_id, COUNT(*) n FROM session_message m,
               json_each(json_extract(m.data,"$.content")) je
               WHERE m.type="assistant" AND json_extract(je.value,"$.type")="tool"
               GROUP BY m.session_id)
SELECT ROUND(1.0*COALESCE(c.n,0)/t.n,2) FROM turns t
LEFT JOIN calls c ON c.session_id=t.session_id WHERE t.session_id="<session-id>";'
```

Below three means the next dispatch briefs the batch's shape — the reads named, the repeats cut — before it claims.
