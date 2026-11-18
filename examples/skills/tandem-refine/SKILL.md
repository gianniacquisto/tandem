---
name: tandem-refine
description: "Run a refine pass: dispatch the refiner to shape backlog tickets into buildable briefs, building nothing. Reach for it when the user says to refine, spec, or shape a ticket, or asks what needs refining."
---

# Tandem refine

A refine pass shapes tickets and builds nothing. It dispatches the `refiner` (`agents/refiner.md`), so the human can pick a different model for the slow interview than for the build.

1. **Show the queue**: `ticket_list` with `filter` open and `status` backlog, in stored order, plus `on-hold` tickets assigned `human` with an open question. No cap: the human picks.
2. **Pick the mode** before the first dispatch: **one at a time** (the default) or **in parallel**. Each refiner interviews the human, so parallel splits the human's attention across interviews; that cost is the human's to accept.
3. **Dispatch** one `refiner` per ticket with the ticket id, on `sonnet`: the interview and the sizing are judgement, the writing is not long. Relay each refiner's grilling round to the human and the human's answers back; never answer a round yourself.
4. **Relay** each refiner's one-line report.

Done when every ticket the pass took carries the brief shape at the status it earns (`ready`, or `on-hold` with its reason), and no question the human has not answered remains open.
