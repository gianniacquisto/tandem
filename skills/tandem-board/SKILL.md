---
name: tandem-board
description: "Move Tandem tickets through their workflow — triage the board, refine a backlog ticket into a description an agent can build from, mark it ready, dispatch a sub-agent, and escalate the genuinely human decisions back to the human. Reach for it when the user says to work the board, move tickets, triage the list, ask what is ready, or refine a ticket."
---

# Tandem board

The board is the same ticket list, read through **Status**: `backlog` → `refined` → `ready` → `in-progress` → `in-review` → `done`, one **cycle lane** per project. The store validates the status value, never the transition — the gates below are the convention, not the schema.

A ticket is the unit of work. The board is where an agent is told *what to do next* and a human is told *what needs them*. Moving a ticket is only honest when the ticket itself carries the reason — a status with an empty description is a lie the next session inherits.

## The gates

| Status | Means | Opening it |
| --- | --- | --- |
| `backlog` | Captured, not shaped. A title and maybe a hunch. | `todo add` |
| `refined` | Shaped: why, what, done-when, and the decisions already taken. A human could approve it without a meeting. | A grilling pass wrote the description |
| `ready` | Refined **and** unanswered questions are empty **and** no human-only decision is pending **and** it fits one agent session. | The gate at the bottom |
| `in-progress` | An owner is on it — an agent name or `human` in `assignee`. | Claiming it |
| `in-review` | The work exists and its PR is open, green, and waiting for a reader. | The author opened the PR and recorded it in the ticket |
| `done` | The outcome exists: a merged PR, a changed file, or a recorded answer. | Evidence, not intention |

Transitions are free-form and `done` is a status like any other; a reopen lands in `backlog` because completion erases the stage. Use the gates anyway: skipping `refined` is how a ticket arrives at an agent with a title and no decision.

## Definition of refined

The `description` carries, in this order:

- **Why** — the problem, and what breaks or stays expensive if it is not done. One paragraph.
- **What** — the change, named concretely enough to disagree with.
- **Done when** — observable acceptance: a command, a test, a screenshot, a file that says the thing.
- **Decided against** — the alternatives already rejected and why, so the next session does not re-litigate them.
- **Open questions** — empty for `refined`; a non-empty list means the ticket is still `backlog`.

## Definition of ready

All of these, or it is not ready:

1. No open questions and no human-only decision pending.
2. One agent session's work. Bigger → split, file the parts, and link them in the description.
3. The code area is named, or the first step is explicitly a wayfinder.
4. Verification is named — the test, the command, or the screen that proves it.

A ticket that needs a decision from the human is `refined` with `assignee human`, never `ready`.

## Triage loop

1. `tandem-cli todo list --filter open` — the board in stored order.
2. Skip any title beginning `PINNED —`. Pinned tickets wait for their own wayfinder; do not refine, move, or reassign them except to hand them to the human.
3. A ticket whose title begins `Session —` is handled by the **Session tickets** section below, not skipped.
4. Read each description. A ticket with no description is raw material, not work.
5. Refine by grilling — the round below — until the description passes **Definition of refined**.
6. Write the description with `todo set --description -`, then set the status that matches the gates.
7. `ready` → **Dispatch**. Human-only → **Escalate**. Neither → leave `refined` and say so.
8. Record one capture for the pass: what moved, what waits, and why.

## Session tickets

A session ticket is work, not a log entry. Every session files one for itself (the convention is in `AGENTS.md`), and the board moves it like any other ticket — so a session that died mid-flight leaves behind a brief someone can build from, not a row nobody can pick up.

The session id in the title — `Session — <project> · <session-id>` — is the handle that settles which case this is. `opencode api session.list` lists the sessions on this machine, each with an `outcome` and a `time.updated`:

- **The session is live** — the ticket belongs to that session. Leave it alone: it is claimed, and the claim is not a stall (see the dispatch section). Do not refine it, do not re-dispatch it.
- **The session has ended** — the ticket is stale work. Refine it and move it: grill the brief against what the session actually did (its captures, its PR, its branch, the repo and the bikeshed), then set the status the gates call for. If the outcome shows the work landed, complete it. If work remains, `refined` → `ready` and dispatch it to a successor. An outcome alone does not say the work landed — a session that was cut off carries `interrupted` and left work behind, and a session that idled and was resumed keeps the outcome it had. Read the brief, the captures and the board before deciding.

Two signals, and neither is exact alone. A finished session carries an `outcome` of `succeeded`, `failed` or `interrupted` — `interrupted` being the one that matters most here, since a session cut off mid-work is exactly the case that leaves stale work behind. But `null` proves nothing, because a live session shows `null` too; and an outcome alone does not prove death, because a session that idled and was resumed keeps the outcome it had. `time.updated` is what settles a live session — updated seconds ago means it is running — and the outcome says whether it finished. Read both.

Apply the **claim is not a stall** rule before treating a ticket as stale: a quiet session is not a dead one. Look for the running child, the branch and the PR, and only call a session ended when there is no sign of life. When in doubt, leave the ticket alone — a ticket nobody picked up is still on the board, and a ticket stolen from a live session is work done twice.

Read the brief before the transcript: it is what the session set out to do, and the transcript shows what it did. A brief that never got written (a session that died before writing one) is raw material — reconstruct it from the transcript, the captures and the board, and say in the description that you reconstructed it.

A session ticket that a live session holds is the one case the board does not touch. Every other session ticket is a ticket.

## Refine by grilling

Work a design tree in rounds. The frontier is every question whose prerequisites are settled.

- **Answer from the environment first.** Repo, ADRs, the bikeshed, the installed app, the CLI. Finding a fact is the agent's job, never the human's.
- Only three things are genuinely the human's: a **preference** (two defensible shapes), a **spend** (time, tokens, money, a release), an **irreversible or outward-facing act** (branch policy, a published API, deleting data).
- Put the surviving questions in the description with your recommendation and the consequence of each choice. That is the escalation payload; the ticket is the meeting.
- A question you can answer with evidence is not a question — decide it, write it under **Decided against** when it was contested.

## Dispatch

A ready ticket is claimed by the agent that takes it: `--status in-progress --assignee <agent-name>`.

- The brief is the ticket **id and description**, nothing more. The child orients from `AGENTS.md` and the bikeshed on its own; restating memory in the brief is how two versions of the truth start.
- Dispatch one ticket per claim. The board's one-at-a-time rule is convention, not the runtime: `max_concurrent_agents: 1` in `~/.config/opencode/opencode.json` caps **local** models only (`gianni-desktop`), so a DeepSeek subagent runs alongside a local one. Parallel agents on one repo still collide in the working tree, not in the store.
- The child branches, builds, verifies, opens a PR, files captures, and ends with one `work` capture.
- The root verifies the evidence, then `todo complete`. Completion without a PR, a diff, or a recorded answer is a claim, not a completion.
- **A claim is not a stall.** Before reclaiming an `in-progress` ticket, look for the live child: `opencode api session.list` shows it RUNNING with a `time.updated` seconds old, even when no branch is pushed yet. A local model can hold a claim for half an hour before its first commit. Only reclaim when there is no running session, no branch, and no PR.

## Escalate to the human

`--assignee human` and status `refined`. The description ends with:

```
## Decision needed
<the one question>

## Options
- A — … (recommended): consequence
- B — …: consequence

## If it waits
<what stays blocked or expensive>
```

One question per ticket; a ticket holding three decisions gets three tickets. Never leave a human ticket `in-progress`, and never let an agent's recommendation stand in for a human's answer.

## How a human answers

A decision ticket has no comment field — the ticket body is the channel. The human answers in one of three places; an agent does the bookkeeping, because the ticket sheet cannot change `assignee`:

1. **In the app** — open the ticket from the human inbox, and write the answer into the description under the `## Decision needed` block, then set Status and Save.
2. **From the CLI** — `tandem-cli todo set --id <id> --description -` with the answer appended, plus `--status` and `--assignee`.
3. **In chat** — tell the agent the decision; it appends the answer and reassigns in one write.

Write the answer as its own block so the next pass can parse it without guessing:

```
## Answer
B — <what was chosen, and any condition>
```

Then, in one write: keep the answer in the description, reset `assignee`, and set the status the answer opens — `ready` if it is now buildable, `backlog` if it needs re-refining, `done` if the answer settles it. A human ticket never goes back to an agent unread: the next pass starts with `todo list --assignee human` and reads every `## Answer`.

## Lanes

A lane is where the work lives — one per project, the board's row. Status cuts across every lane.

- `todo add --lane <name>` and `todo move --lane <name>` require the lane to exist. The lifecycle is on the CLI: `lane add --name <name>`, `lane rename --from <name> --to <name>`, `lane delete --name <name>` — the same core operations the app's tree calls, so trimming and the empty/duplicate/unknown rejections are one implementation (ADR 0006's amendment). `lane delete` moves members to the default area, never deletes them; reordering rows (`moveLane`) stays GUI-only.
- An empty lane is not a signal to invent one. Cutting a lane is a decision about how a project is organised, so ask the human before you add one, even though the verb exists.
- `--lane ""` moves a ticket to the default area, which is where untriaged capture lands.

## What the board is not

- Not a second store: same `todos.json`, same ticket object, same CLI.
- Not due dates, priority, labels, checklists, comments, or attachments — a recorded no. Do not smuggle them in through descriptions.
- Not a lock. A human may drag any ticket anywhere in the app; status is what is true, and the next triage pass reconciles it.
- Not a place for long-form design. A ticket links a bikeshed note for that, and decisions still land as ADRs.
