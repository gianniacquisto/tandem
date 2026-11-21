---
name: tandem-tidy
description: "Board hygiene pass: sweeps the open tickets against work that already landed, completes what is done or moot with the evidence in a comment, recovers dead claims, archives the done, and reports the oddities. Reach for it when the human says tidy or clean up the board, asks what on the board is stale, or right after a run of merges."
---

# Tandem tidy

A maintenance pass over the whole board. It opens no sprint and refines nothing. Its one write of its own is **complete with evidence**: a ticket already satisfied by landed work leaves the open set with its proof in a signed comment. Only what you can point at moves; a hunch completes nothing.

## 1. Orient

In one round: `ticket_list` with `filter` open across every project; `ticket_comment_list` for each ticket whose state is in doubt; `git log` in each project's checkout, since the board is shared and the evidence is local.

## 2. Dead claims

Recover each dead claim as the sprint defines it (`skills/tandem-sprint/SKILL.md`, §2): back to `ready`, owner cleared, a dated, signed comment naming the evidence.

## 3. The evidence sweep

For every open ticket past `backlog` with no live claim, look for the work that already landed: the git log and merged PRs, the board's done tickets (`ticket_list` with `text`), and the repo itself (a file that exists, a flag that is gone). Three verdicts:

- **Resolved**: landed work meets its `Done when`. A comment naming the commit, PR, done ticket or fact, then `ticket_complete`.
- **Moot**: its subject is gone, superseded or answered another way. Same write, the reason in the comment.
- **Unproven**: untouched.

An escalated ticket (`in-review`, `human`, a PR in the log) is settled by the PR: `gh pr view <n> --json state,mergeCommit` reading `MERGED` is Resolved; still open stays the human's.

## 4. Housekeeping

`ticket_archive` with no `id` archives every done ticket; the reply names how many. Lift each block whose blocker is `done` with `ticket_unblock`; never one whose blocker is open.

## 5. Report

Report-only, for the human to rule on: duplicates (two open tickets, one subject), and tickets whose named blocker landed and are ripe to draw.

Then one block per project: completed with evidence, claims recovered, archived count, and left alone with the reason. Done when every open ticket appears in the report.
