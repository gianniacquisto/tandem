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

A brief's mistakes are paid by every pass after it (the build, the gate, up to two fix passes), so refine is where a stronger model pays most. The tier raises the model, not the effort.

| Tier | Refiner |
| --- | --- |
| **standard** | `sonnet`, medium |
| **design** | `opus`, medium |

**Design** fires on any one of these triggers, read from the ticket as written. Each is a fact, not a judgement, so any model running this pass proposes the same tier:

- it names `rust/core/src/store/schema.rs`, a migration, or stored data;
- it names a published contract: an MCP tool, CLI output, a wire type, a file format;
- it names more than one layer (`rust/core`, `rust/cli`, `rust/core/src/serve`);
- it asks for something new with no `Done when` and no example of the result;
- it would need splitting: more than one change a reviewer could merge on its own.

No trigger is **standard**. The proposal names the trigger it matched (`TAN-217: design, names schema.rs`), so the human checks a fact, not a feeling, and confirms or overrides every tier. When the ticket is too thin to tell, propose **design**.

**The tripwire** catches what the ticket text hid: a standard refiner that finds a trigger in the code stops before its first round and reports `needs design tier: <trigger>`. Re-dispatch that ticket on the design tier; the human is not asked twice.
