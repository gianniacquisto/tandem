---
name: tandem-refine
description: "Run a refine pass: dispatch the refiner to shape backlog tickets into buildable briefs, building nothing. Reach for it when the user says to refine, spec, or shape a ticket, or asks what needs refining."
---

# Tandem refine

A refine pass shapes tickets and builds nothing. It dispatches the `refiner` (`agents/refiner.md`), on the model its tier picks, never the session's; a `scout` picks the tier first.

1. **Show the queue**: `ticket_list` with `filter` open and `status` backlog, in stored order, plus `on-hold` tickets assigned `human` with an open question. No cap: the human picks the tickets.
2. **Scout each picked ticket**: a ticket whose comment log already holds a `scout` comment reuses its latest `SCOUT:` line. Dispatch one `scout` (`agents/scout.md`) per remaining ticket, on `sonnet`, low effort, all at once (each writes only its own comment), or in batches as large as the host's concurrent-subagent limit allows. Show the human each `SCOUT:` line, tier and trigger, and let the human confirm or override each tier.
3. **Pick the mode** before the first refiner: **one at a time** (the default) or **in parallel**. Each refiner interviews the human, so parallel splits the human's attention across interviews; that cost is the human's to accept.
4. **Dispatch** one `refiner` per ticket with the ticket id and its `SCOUT:` line, on its tier's model and effort. Relay each refiner's grilling round to the human and the human's answers back; never answer a round yourself.
5. **Relay** each refiner's one-line report, its ticket named by the Naming tickets rule in `AGENTS.md`.

Done when every ticket the pass took carries the brief shape at the status it earns (`ready`, or `on-hold` with its reason), and no question the human has not answered remains open.
6. **Scribe** each refined ticket as `skills/tandem-scribe/` says, and relay its `SCRIBE:` line beside the refiner's.

## Refine tiers

A brief's mistakes are paid by every pass after it (the build, the gate, up to two fix passes), so refine is where a stronger model pays most. The tier raises the model, not the effort.

| Tier | Refiner |
| --- | --- |
| **standard** | `sonnet`, medium |
| **design** | `opus`, medium |

**Design** fires on any one of these triggers, found by the scout in the ticket or in the code it touches:

- it touches the schema, a migration, or stored data;
- it touches a published contract: an API, CLI output, a wire type, a file format;
- it crosses more than one of the layers the repo's `AGENTS.md` names;
- it asks for something new with no `Done when` and no example of the result;
- it would need splitting: more than one change a reviewer could merge on its own.

No trigger is **standard**; a ticket too thin to tell is **design**. The session running this pass only follows the table: the judgement is the scout's, so the session itself can run on any model, `haiku` included.
