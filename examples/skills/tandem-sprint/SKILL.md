---
name: tandem-sprint
description: "Run a sprint: draw a batch of ready tickets, build each through the builder, gate each through the reviewer, and close with every batch ticket in a known exit state. Reach for it when the user says run a sprint, asks what the workflow is, or starts a coding pass on this board."
---

# Tandem sprint

A sprint is one invocation that takes a batch of `ready` tickets to an exit state: **scope → plan → build → gate → close**. It is a procedure, not a stored entity: the board's statuses, assignees and comment logs are its record (ADR 0045). Shaping is the standalone refine pass (`skills/tandem-refine/`), learning is the standalone `retro`; a sprint neither runs nor prompts them.

The sprint's runner is the `sprint` role. It dispatches the `builder` (`agents/builder.md`) and the `reviewer` (`agents/reviewer.md`) and writes no code itself.

## 1. Scope

A sprint covers **one project**: the basename of the directory the pass runs in, with a `-wt-<slug>` worktree suffix stripped, matched case-insensitively against `label_list`. A label the human names overrides it. Use the label's stored spelling from then on. No match: ask the human, because adding a label is the human's call.

Read the board in one round: `ticket_list` for the label's open tickets, and `ticket_list` with `assignee` human. Another project's ticket belongs to that project's sprint.

## 2. Plan the batch

**Recover first.** An `in-progress` ticket whose pass has ended is a **dead claim**: its log has no live handoff and no green PR. Set it back to `ready` with the owner cleared and a comment naming the evidence, then treat it as any other ready ticket. An open PR from the dead pass is reused by the next build, not reopened.

**Draw `ready` only**, at most five, in stored order:

- skip any row whose `blocked` is true, whatever its status (the edge is the block, ADR 0035), with one line naming the blocker;
- report `backlog` rows as `TAN-217 needs a refine pass`, never drawn: only refine moves a ticket to `ready`;
- leave `on-hold` alone: it is parked on purpose;
- a ticket with an open question, a pending human decision, or more than one pass of work is not ready: report it for refine.

**Order the batch.** A ticket another batch ticket blocks on goes first. For parallel mode, read each brief's `## What` for the files it names: two tickets that touch the same files run one after the other, never side by side.

**State the batch before the first dispatch**, in one message to the human: the drawn tickets in order, the skipped ones with their reason, and the mode, **one at a time** (the default) or **in parallel** (the human's call). Zero drawn is a triage-only sprint: report it and stop.

## 3. Build

Per ticket, in batch order:

1. **Claim**: `ticket_set` with `status` in-progress and `assignee` builder.
2. **Dispatch** the `builder` with the ticket id, its description (`ticket_show`) and its comment log (`ticket_comment_list`). The builder works in its own worktree and hands to `in-review` only on a green head.
3. **Check the handoff.** Re-read the ticket: `in-review` with a PR number in the log is a build. Anything else is a failed dispatch: remove its worktree, then one fresh dispatch. A second failure makes it a dead claim (§2) with the failure quoted, and the ticket slips.

## 4. Gate

Dispatch the `reviewer` with the ticket id and the PR. It answers one `VERDICT:` line:

- **`merged`**: the reviewer completed the ticket. Every other open batch PR is now behind `main` and its next gate will say so.
- **`escalated`**: accept it only after `ticket_show` reads status `in-review` and assignee `human`. Anything else is a failed gate: re-dispatch the reviewer once, then hand it over yourself. An escalated ticket leaves the active batch; the sprint does not wait for the human.
- **`blocked`, behind its base**: bring it up to date yourself (`gh pr update-branch <n>`, a merge), wait for green on the new head (`gh pr checks <n> --watch`), and gate again. A conflict the update cannot merge is a fix pass.
- **`blocked` on a red check, or `changes-requested`**: a **fix pass**, a fresh `builder` dispatch with four things and nothing else: the ticket id, the PR number, the head sha the gate read, and the finding quoted. Then gate again; a re-gate may resume its own reviewer session.

**The fix loop has a ceiling: two fix passes per ticket.** A third failing verdict escalates: a comment quoting the three findings, then `ticket_set` with `status` in-review and `assignee` human. A branch update or a capacity-cancelled CI rerun (`agents/builder.md`, step 6) is not a fix pass and does not count.

## 5. Close

The close runs when every batch ticket has an exit state. There are four, and every batch ticket ends in exactly one:

| Exit | The board reads | Who acts next |
| --- | --- | --- |
| **landed** | `done`, merge commit in the log | nobody |
| **escalated** | `in-review`, assignee `human`, the risk on the PR | the human |
| **slipped** | `ready`, owner cleared, a comment saying why | the next sprint |
| **parked** | `on-hold`, a `## Blocked on` section or the open question at the top | whoever the hold names |

Settle the label as well: a dead claim outside the batch goes back to `ready` with a comment; `human` stays only where a question is open. One comment per ticket the close moves, signed `sprint`, carrying what landed, the outcome and next status, and any issues; none where nothing moved.

**The close report**, the last thing the sprint says:

```
SPRINT: <label>: <n> landed, <n> escalated, <n> slipped, <n> parked
```

Then one line per batch ticket (`TAN-123 landed: PR #456`), one line per skipped ticket with its reason, and what waits on the human. A sprint that ends in the middle (the session dies, the human stops it) leaves dead claims, and the next sprint's §2 recovers them.

## Release

**A sprint cuts no release.** A release cuts only on the human's explicit words naming the version, and the sprint never prompts for one. When asked, follow `docs/releasing.md`: tag the landed `main` head, push the tag, watch the Release workflow publish, and generate the notes with `Scripts/release-notes.sh vX.Y.Z`.
