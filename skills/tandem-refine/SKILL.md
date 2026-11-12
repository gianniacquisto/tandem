---
name: tandem-refine
description: "Dispatch the refiner to shape a backlog ticket into a buildable brief — the front door for a refine session, where the human wants to spend time refining rather than building. Reach for it when the user says to refine, spec, or shape a ticket, or asks what needs refining."
---

# Tandem refine

A refine session shapes tickets and builds nothing. This door dispatches the `refiner` subagent (`agents/refiner.md`) — so the human can pick a different model for the slow interview than for the build.

1. Show the queue. Read the active workflow (`tandem-cli workflow show --id agile`, or `workflow list`), then its intake stages in stage order. The intake stage nearest Ready is the queue: `tandem-cli ticket list --filter open --status <queue-key>` shows its open tickets in stored order as this pass's batch. When the queue has tickets, show the backlog as a count and reach it only on the human's explicit ask; when the queue is empty, it falls back to the backlog as today. On-hold tickets with open questions or `--assignee human` still show. There is no cap: the queue is the human's plan, not a quota. Promotion into the queue is human-only: the board or the CLI moves a ticket in, while the refiner drains the stage and never fills it. When a `selected` ticket splits, its parts are filed in `selected` so they stay in the queue; a backlog parent's parts stay `backlog`.
2. **Pick the mode before the first dispatch** — the human in the session chooses: **one at a time** (the default) or **in parallel**. A refiner interviews the human, so parallel dispatches divide the human's attention across simultaneous interviews; in parallel, that cost is the human's accepted one, not a silent one.
3. Dispatch `refiner` with the ticket id. One ticket per dispatch. In parallel mode, dispatch one refiner per batched ticket at once.
4. **Every ticket gets its human round.** The refiner grills the human per ticket (`agents/refiner.md` step 1) before writing any brief, and a short confirmation round counts for trivial or fully specified tickets. A dispatched refiner relays its frontier through this session and waits; the pass writes no brief until the human answers. The only opt-out is the human's live per-case delegation, quoted in the brief. Only refine moves a ticket to `ready`: a finished brief the sprint finds still goes through refine, with its short round.
5. Relay the refiner's one-line report; hand any open human decision straight to the human.

Done when: every ticket the pass shaped carries the brief shape and sits at the status the brief earns (`ready`, or `on-hold` when it is parked), and no open question the human has not answered remains.
