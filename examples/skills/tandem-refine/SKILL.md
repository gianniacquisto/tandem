---
name: tandem-refine
description: "Run a refine pass: dispatch the refiner to shape backlog tickets into buildable briefs, building nothing. Reach for it when the user says to refine, spec, or shape a ticket, or asks what needs refining."
---

# Tandem refine

A refine pass shapes tickets and builds nothing. It dispatches the `refiner` (`agents/refiner.md`), on the model its tier picks, never the session's.

1. **Show the queue**: `ticket_list` with `filter` open and `status` backlog, in stored order, plus `on-hold` tickets assigned `human` with an open question. Beside each, propose its refine tier (below). No cap: the human picks the tickets and confirms or overrides each tier.
2. **Pick the mode** before the first dispatch: **one at a time** (the default) or **in parallel**. Each refiner interviews the human, so parallel splits the human's attention across interviews; that cost is the human's to accept.
3. **Dispatch** one `refiner` per ticket with the ticket id, on its tier's model and effort. Relay each refiner's grilling round to the human and the human's answers back; never answer a round yourself.
4. **Relay** each refiner's one-line report.

Done when every ticket the pass took carries the brief shape at the status it earns (`ready`, or `on-hold` with its reason), and no question the human has not answered remains open.

## Refine tiers

A brief's mistakes are paid by every pass after it (the build, the gate, up to two fix passes), and a refiner takes few turns (about one per round), so refine is where a stronger model pays most. Effort stays medium: the tier raises the model, not the effort. The tier comes from the backlog ticket as written, since its `Size:` does not exist yet:

| Tier | The ticket | Refiner |
| --- | --- | --- |
| **standard** | one area, a clear ask, likely `S` or `M` | `sonnet`, medium |
| **design** | crosses layers, touches the schema or a published contract, needs a split, or its ask is open-ended | `opus`, medium |

When unsure, propose **design**: a wasted `opus` interview costs less than a build on the wrong brief.
