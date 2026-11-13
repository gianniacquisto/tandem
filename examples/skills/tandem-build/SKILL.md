---
name: tandem-build
description: "The sprint's build phase, per batch ticket: claim, build in its own worktree, review through the gate, record on the ticket. The sprint (`skills/tandem-sprint/SKILL.md`) runs this skill for its build phase; readable on its own for the build and review contracts."
---

# Tandem build

The build phase of the sprint loop: the batch is drawn, and this runs each ticket through build → review → merge. Every write is a ticket verb or a ticket comment; there is no parallel sprint record.

The human in the pass picks the mode before the first dispatch, **one at a time** (the default) or **in parallel**. Serial is the default for turn budget and simplicity, not merge safety: each build is isolated in its own worktree, so parallel builds cannot share a checkout.

## 1. Claim

`ticket set` with `status` in-progress and `assignee` the pass. A ticket still `ready` with its brief and no open question is the only thing that gets claimed. A claim whose pass ended is reclaimed by triage, never by resuming the dead pass.

## 2. Build

Dispatch the `builder` subagent (`agents/builder.md`) with the ticket id and its description (`ticket show` with `id`, since `ticket list` rows are summaries without the body) and its comment log (`ticket comment list` with `id`). The builder works in its own git worktree, branches fresh from `origin/main`, verifies with the scoped cargo command, opens the PR with its `## Merge Danger` claim, and appends one comment naming what landed. A dispatch that fails leaves a dead claim, not a done ticket: reclaim it with `ticket set` (back to `ready`, owner cleared), a loud comment, and a fresh dispatch after removing the dead worktree.

## 3. Review

The gate needs two facts, and only its own ticket supplies either: a green head, and a branch not behind its base.

First resolve the fetch remote: `gh pr view <n> --json isCrossRepository,headRepositoryOwner,headRefName`. When `isCrossRepository` is true, the branch has no `origin` ref, so add the fork remote once and fetch it; the behind-count reads the fork ref, and a throwaway worktree checks out that ref. When false, fetch `origin` and count `git rev-list --left-right --count origin/main...origin/<branch>`. A branch that went stale while the gate queued is brought up to date (`gh pr update-branch <n>`) and the new base head is named in the brief. Before any push, grep the rebased diff for a symbol the base renamed away, and stop before the push if one survives.

Then dispatch the `reviewer` subagent (`agents/reviewer.md`) with the ticket id and PR. It merges a two-way/low door and completes the ticket; a one-way door is escalated to the human and **leaves the batch**, but the sprint waits for it. An `escalated` report is accepted only after reading the ticket: status `in-review` and assignee `human` mean handed over; anything else is a failed gate, so re-dispatch or hand it over in this pass and say so in the comment.

   **A fix pass** is a fresh `builder` dispatch, never a resume of the build's. Dispatch it when the gate answers `blocked` on the build's own head (a red check) or posts a finding that must change first. The brief carries four things and nothing else: the ticket id, the PR number, the head sha the gate read, and the gate's finding quoted. The fix claims the ticket back (`ticket set` with `status` in-progress and `assignee` builder) and hands it to review again only on a green head bound to the new sha. Its worktree carries the PR's branch, fetched (`git fetch origin board/<slug>`, then a worktree on it), not a fresh branch from trunk. A re-gate may resume its own reviewer session.

## 4. Record

Append one comment on the ticket: what landed, the PR number, and any fix pass's outcome. The comment is the work record, and the ticket's status move is the separate act. No usage report, no session record and no sprint run entry sits beside it (ADR 0045).

A turn re-reads the whole window, so a pass's bill is its turn count times its context. The number lives once, in `agents/builder.md` step 4: **three tool calls per turn or more**, with the batched shapes named there. This skill points at it rather than restating it.
