---
name: tandem-refine
description: "Dispatch the refiner to shape a backlog ticket into a buildable brief — the front door for a refine session, where the human wants to spend time refining rather than building. Reach for it when the user says to refine, spec, or shape a ticket, or asks what needs refining."
---

# Tandem refine

A refine session shapes tickets and builds nothing. This door dispatches the `refiner` subagent (`agents/refiner.md`) — so the human can pick a different model for the slow interview than for the build.

1. Show the candidates — `tandem-cli ticket list --filter open --status backlog`, plus `on-hold` tickets with open questions or `--assignee human` — with your pick.
2. **Pick the mode before the first dispatch** — the human in the session chooses: **one at a time** (the default) or **in parallel**. A refiner interviews the human, so parallel dispatches divide the human's attention across simultaneous interviews; in parallel, that cost is the human's accepted one, not a silent one.
3. Dispatch `refiner` with the ticket id. One ticket per dispatch. In parallel mode, dispatch one refiner per batched ticket at once.
4. Relay the refiner's one-line report; hand any open human decision straight to the human.

Done when: every ticket the pass shaped carries the brief shape and sits at the status the brief earns (`ready`, or `on-hold` when it is parked), and no open question the human has not answered remains.
