---
name: tandem-interview
description: "The human's decisions in one pass — walks the human through every ticket awaiting their decision, one ticket at a time: the queue, the cost order, the per-ticket grilling rounds, the per-ticket comments, the statuses the decisions earn, the spawned follow-ups, and the closing table. Reach for it when the human asks to be interviewed about the tickets that need their attention, or to clear the human's queue."
---

# Tandem interview

A pass that walks the human through every ticket awaiting their decision, one ticket at a time. The 2026-10-24 interview settled all eleven human-assigned tickets in one sitting on exactly this procedure, improvised from the `grilling` skill's round format and the board half of the `tandem` skill; this skill pins it. It settles decisions only — building, refining and reviewing the settled tickets stay the other passes' work.

The round format is the grilling skill's (`skills/grilling/SKILL.md`): number the questions, give a recommended answer each, word so "yes" accepts. The board mechanics are the `tandem` skill's. The runner signs its comments by its own role, the roster's (`skills/tandem-team/SKILL.md`).

## The pass

1. **Find the queue** — `tandem-cli ticket list --filter open --assignee human` is the starting set; skim the open board for `on-hold` + open questions that name the human even when unowned.
2. **Order by cost** — single-decision tickets first, the open-ended reviews last — a ticket whose brief says it is its own grilling session, e.g. `4147A3ED`.
3. **One ticket per round.** Read the brief whole (`tandem-cli ticket show --id <id>`), then put that ticket's frontier to the human in one message — the grilling skill's round format: number the questions, give a recommended answer each, word so "yes" accepts. Never queue two tickets' questions into one round.
4. **A decision may reshape the tree.** When the human's answer spawns sub-decisions (e.g. `BE30574F`'s `lane-number` ref scheme), ask the spawned frontier before moving on — but only what that answer unblocked.
5. **Record per ticket, as you go** — one `tandem-cli ticket comment add` per settled ticket, signed by the runner's role, carrying the answer, the stated cost of overriding a brief's recommendation, and what the done-state now reads. A correction to a false brief premise goes in the same comment.
6. **Close the ticket's human hold** — `tandem-cli ticket answer --id <id> --status <next> [--assignee <owner>] --body -` records the decision in the body and moves the pair in one write: the status the decision earns (`ready` when the brief survives the decision; **`backlog` when the decision contradicts the written brief**, so the refiner rewrites from the comment) and the next owner, omitted when nobody owns it yet. The comment from step 5 stays the record of the round; the answer block is the question's exit.
7. **Spawn follow-ups** — file the tickets a decision creates, with the decision embedded, before ending.
8. **Close with the table** — one summary of every ticket, its decision, and its new status; flag the answers that overrode recommendations.

Done when: every queue ticket carries its decision as a comment, its hold is closed or deliberately kept (a parked ticket says what it waits on), the spawned follow-ups are filed, and the closing table stands in the session — every ticket of the queue named in it.
