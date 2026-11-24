---
name: tandem-interview
description: "Walk the human through every ticket awaiting their decision, one ticket per round, and record each decision on its ticket. Reach for it when the human asks to be interviewed about the tickets that need their attention, or to clear the human's queue."
---

# Tandem interview

A pass that settles the human's decisions and nothing else: building, refining and reviewing the settled tickets stay the other passes' work. Rounds use the `grilling` skill's format. Sign comments as the role running the pass.

1. **Find the queue**: `ticket_list` with `filter` open and `assignee` human, plus `on-hold` tickets whose open question names the human.
2. **Order by cost**: single-decision tickets first, open-ended ones last.
3. **One ticket per round.** Read the brief whole (`ticket_show`), then put that ticket's frontier to the human in one message. Never mix two tickets in a round. When an answer spawns sub-decisions, ask only what it unblocked before moving on.
4. **Record as you go**: one `ticket_comment_add` per settled ticket with the decision, the cost of any override of the brief's recommendation, a correction to any false premise, and what done now means.
5. **Move the pair** with one `ticket_set`: `ready` when the brief survives the decision, `backlog` when the decision contradicts it (the refiner rewrites from the comment), with the next owner, or none.
6. **File follow-ups** a decision creates, with the decision in their description.
7. **Close with a table**: every queue ticket, its decision, its new status, and its `SCRIBE:` line (a scribe per settled ticket, as `skills/tandem-scribe/` says); flag the answers that overrode a recommendation.

Done when every queue ticket carries its decision as a comment and has left the human's queue or says what it still waits on.
