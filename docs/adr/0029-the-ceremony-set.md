# The ceremony set — a roster, a standup, a sprint cycle, and the minutes

Six passes run on a Tandem board — capture, triage, dispatch, review, retro, consolidate — and no ceremony stands between them. The passes do their own job; nothing gathers the team, nothing stands it up, nothing plans a batch, and nothing posts to the log. This ADR records the ceremony set that fills the gap: a roster of the roles, a standup, a sprint cycle, and the ticket comment log as the minutes. The set adds no field, no verb and no surface — it names what already runs, and it points at the two sibling tickets that build the standup and the sprint cycle.

## Why

A ceremony is the thing that makes a set of agents read as a team. Six passes working the same board produce six outputs and no conversation: the board shows what is claimed and what is done, never what each role thinks, and a session that wants to know what the other sessions did reads every ticket's description and the git log. The cost is already in the store — the comment log exists, the app draws it and the CLI reads it, and the only regular author is the review gate, so the minutes record a gate and nothing else.

The roster is the other half. The roles are named across four prompts, a dozen skills and a mirror repo's own Roster table, and nothing says which role fills which agent or which pass each role owns. So every pass that speaks or claims picks a name out of the air: `reviewer`, `refine`, `board-build`, and once `self` — a value the CLI does not expand for `--author`, so the minutes recorded nobody. A reader cannot tell a discussion between roles from one agent talking to itself.

## Decision

- **The ceremony set exists** — a roster, a standup, a sprint cycle, and the ticket comment log as the minutes. The roster is one page, `skills/tandem-team/SKILL.md`, and no other pass restates it: the board skill points at it, and the standup and sprint skills do too.
- **The sprint's batch is a ticket, not a field.** A `Sprint <n> — <lane>` ticket carries the batch in its description. The store gains nothing — no schema change, no migration — and the board shows the batch where the work lives, in the lane.
- **The standup reports and the dispatcher claims.** The `ready` set has one claimant, the board skill's dispatch, and one reporter, the standup. They are different roles on purpose, so a ticket is never picked up twice and the status round never claims anything.
- **No new agent per ceremony.** A ceremony is a procedure the session's own agent runs — a skill, not a prompt. A prompt would add a config and a model choice without adding behaviour behind it, and the personas answer (`C4C590ED`) is still open.
- **What changes when personas land.** The role strings become identities, each with its own memory in the bikeshed. Nothing else in the set moves: the roster's role column stays as it is, the row's agent changes, and the name rule keeps one vocabulary across `--assignee` and `--author`.

## Decided against

- **A roster of personas** — `C4C590ED` is pinned and the human answered "leave it as a string for now". This ADR names the agents that exist and says what changes when that answer lands; it does not open the pinned ticket.
- **A new agent per ceremony** (`agents/standup.md`, `agents/sprint.md`) — a ceremony is a procedure, not a model. The host already serves four prompts, and the personas answer is open, so a prompt buys a config and a model choice with no behaviour behind it.
- **The roster in the fleet's `WORKFLOW.md`** — that file's Roster table is the closest existing home, and the fleet repo's retirement is an open human question (`4B419A55`). **The fleet copy is stale**: it is an inventory of agents, not of roles, and it cannot name a ceremony. Nothing in that repo is edited by this ADR.
- **Three copies of the roster** — one page, and the board skill points at it. A second copy in each ceremony skill is the "stated twice, differently" failure this board has already paid for twice, over the completion rule and the retro marker.
- **A glossary entry for each new word** — `GLOSSARY.md`'s ticket vocabulary belongs to `3BC6ACD3`; `C5D4F345` renames the store's path in the agent text and puts the glossary out of scope. This ADR defines the terms it uses, in the terms it uses.

## Consequences

- **The store, the CLI and the app are untouched.** No field, no verb, no surface, no migration, no release. The comment log that already holds the minutes gains nothing; what changes is who writes in it and under what name.
- **One vocabulary, one home.** The name rule — a role's name in `--assignee` when it owns a ticket and in `--author` when it speaks on one — turns on a measured fact: `--author` takes a name and never expands `self`, while `--assignee` and `--origin` do. A comment with an empty author is a defect in the record, not a style choice.
- **The two ceremony skills are sibling tickets.** `skills/tandem-standup/` (ticket `64BD463D`) and `skills/tandem-sprint/` (ticket `5E3F8DE6`) are built — the standup as `25dfb2e`, the sprint as `990e177`, both on 2026-10-22; the roster points at the two files.
- **The number.** 0029 is the next free number: 0027 is claimed by `CEB105DA` and 0028 by `FE0140EB`, and no open pull request adds an ADR. The repo's rule settles a collision — the older ticket keeps the number and the newer branch renumbers on its own branch — so a branch that lands a 0029 first sends this one to 0030.

## Amendment, 2026-10-23: the ceremony set is a sprint, a roster, and a retro

**The change.** `skills/tandem-sprint/` is the SDLC's single entry point — orient, triage, batch, per-ticket build and review, close — and absorbs the board skill's gates and dispatch (`skills/tandem-board/` deleted; the brief shape is now the refiner's contract, the dispatch is the sprint's). `skills/tandem-standup/` is deleted: the sprint ticket's `## Run` sections and the per-ticket comments already record the round, and the minutes rule lives in the roster. `skills/tandem-handoff/` is deleted: "hand me the next sprint" is the handoff.

**The retro leaves the cycle.** The retro is a human-invoked skill (`skills/retro/`, adapted from the vendored one) that files `backlog` tickets into the Retro lane. Sprint's close is close, not a retro catch-up: the `## Retro` marker and the catch-up step are gone, and nothing is retro'd automatically.

**Why.** The human's call: a clear, opinionated SDLC — sprint as kickoff, refine as its own deliberate mode with its own model choice, retro on human request only. Six passes and three ceremonies had become twelve skills and four agents; the terse pass cut the ones no one was reaching for.

**Consequences.** The ceremony set is now: roster, sprint, retro. The store, the CLI and the app are still untouched — no field, no verb, no surface, no migration. The roster's `standup` and `sprint` role rows are redefined: `sprint` is the session's own agent running the skill, `standup` is gone.
