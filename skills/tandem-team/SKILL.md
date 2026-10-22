---
name: tandem-team
description: "The team's own page — the roster of roles and the agents that fill them, the name rule for `--assignee` and `--author`, the minutes rule for the ticket comment log, and the map of the ceremonies. Reach it when you need to know which role owns a pass, what to write as a ticket's assignee or a comment's author, where a ceremony lives, or who the team is."
---

# Tandem team

One page, four things: who fills which role, what to write when a role speaks, what a comment on a ticket is for, and where each ceremony lives. Every other pass points here instead of restating it — one vocabulary, one home.

## The roster

| Role | The agent that fills it | The pass it owns |
| --- | --- | --- |
| `human` | — the human | the decision gate: a preference, a spend, an irreversible or outward-facing act; the release. No prompt: the human is the decision, not a pass. |
| `capture` | the session's own agent | the session's own record: the session ticket at the start, the note captures as the work happens. The rule lives in the host's instructions, not in a prompt of its own. `AGENTS.md` (Sessions), `skills/tandem-memory/` |
| `dispatch` | the session's own agent | triage (`backlog` → `refined` → `ready`) and dispatch — claim the ticket, then hand the builder its id and description. `skills/tandem-board/` |
| `refine` | the `refine` subagent | the spec pass: a ticket too big or too undecided for one session becomes a spec and one-session tickets. `agents/refine.md` |
| `build` | the `general` subagent, one per ticket — or a primary session run as `build`, `black` or `blue`; the two builders are not linked on this Mac | the build half of dispatch: branch, build, verify, PR, captures. No prompt of its own. |
| `reviewer` | the `reviewer` subagent | the review gate: merge a two-way door with low blast radius, escalate a one-way one. `agents/reviewer.md` |
| `standup` | the session's own agent | the standup: the status round, posted to the tickets' comments. `skills/tandem-standup/` — not built yet; ticket `64BD463D` builds it. |
| `sprint` | the session's own agent | the sprint cycle: selection, the run, the close, the next session. `skills/tandem-sprint/` — not built yet; ticket `5E3F8DE6` builds it. |
| `retro` | the `retro` subagent | the retro: one session in, `backlog` tickets out; the report is a receipt. `agents/retro.md` |
| `consolidator` | the `consolidator` subagent | consolidation: the sole writer of the curated bikeshed notes, on request. `agents/consolidator.md`, dispatched by `skills/tandem-consolidate/` |

The table is an inventory of roles, not of prompts: `human`, `capture` and `build` own no prompt of their own, and the two ceremony rows name skills that are not built yet — each of the five says where its gap is filled.

Agents that fill no role: `explore` (read-only repo search — a tool the passes reach for, not a pass), `plan` (the host's read-only planner), and `black`/`blue` (primary builders; a session running as one fills `build`).

**The columns.** The board's order, and the role that owns each column — what the handoff rule reads. The order is the board's own: `skills/tandem-board/SKILL.md`'s gate table.

| Column | The role that owns it |
| --- | --- |
| `backlog` | `dispatch` — triage shapes it. |
| `refined` | `dispatch` — triage marks it `ready`; `human` holds it when the decision is theirs; `refine` writes the spec when a ticket is too big for one session. |
| `ready` | `dispatch` — it claims the ticket and hands it to the builder. |
| `in-progress` | `build` — the builder claims it and works it. |
| `in-review` | `reviewer` — the review gate merges it or escalates it. |
| `done` | the actor holding the evidence — the ticket rests here; there is no next assignee. |

## The name rule

One vocabulary, two fields. A role's name is what goes in `--assignee` when the role owns a ticket, and in `--author` when the role speaks on one.

- `--assignee <role>` — the owner: `tandem-cli ticket set --id <id> --status in-progress --assignee build`.
- `--author <role>` — the speaker: `tandem-cli ticket comment add --id <id> --author reviewer --body …`.
- **`--author self` writes the four letters `self`.** Measured on a throwaway home: `TANDEM_HOME=$(mktemp -d)`, add a ticket, `tandem-cli ticket comment add --id <id> --author self --body x`, then `tandem-cli ticket comment list --id <id>` returns `{"at":"…","author":"self","body":"x"}` — a literal, not a name. `--assignee` and `--origin` do expand `self` to `$TANDEM_AGENT`; `--author` takes "a name; omitted is the unnamed author" (`tandem-cli --help`).
- **Write the role out.** `--author build` names the builder; `--author self` names nobody, and the minutes record nobody.
- **A comment with no `--author` is the unnamed author `""`.** That is a defect, not a style choice: the log is the minutes, and a minute with no speaker is a gap in the record.

When the personas answer lands (`C4C590ED`), the role string becomes an identity with its own bikeshed memory. The roster's role column does not change then — the row's agent does.

## The minutes

The comment log on a ticket is the team's minutes.

- **The description is the brief** — why, what, done-when, decided-against. Written once and appended to, never overwritten.
- **The comment log is the conversation** — in order, append-only. There is no edit and no delete: a correction is a new comment.
- **Any role writes as itself.** The standup is the one regular producer; the reviewer's verdict, a builder's progress note, a refiner's question to the human and the human's answer are ad hoc and welcome.
- **What a reader gets.** The app's ticket sheet draws the log in order — author, time, and the body as stored — and the human's own comment is drawn too. `tandem-cli ticket comment list --id <id>` returns the same log as JSON. The minutes are readable without a transcript.
- **Evidence a reader must act on also goes in the description.** The description is the record; the comment is the conversation.

## The ceremony map

Each ceremony is a procedure. The pointer is the instruction; the procedure lives where it points.

| Ceremony | Where it lives |
| --- | --- |
| Capture — the session ticket and the note captures | `AGENTS.md` (Sessions), `skills/tandem-memory/` |
| The standup — the status round | `skills/tandem-standup/` — not built yet; ticket `64BD463D` builds it. |
| The sprint cycle — selection, the run, the close, the next session | `skills/tandem-sprint/` — not built yet; ticket `5E3F8DE6` builds it. |
| Triage, dispatch, escalation | `skills/tandem-board/` |
| Review | `agents/reviewer.md` |
| Retro | `agents/retro.md` |
| Memory — the consolidation pass | `skills/tandem-consolidate/` |
