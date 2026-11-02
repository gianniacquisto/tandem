---
name: tandem-team
description: "The roster — which agent fills which role, what to write in --assignee and --author, and the minutes rule for the ticket comment log. Reach it when you need to know who owns a pass, what to write as a ticket's assignee or a comment's author, or who the team is."
---

# Tandem team

| Role | Who | The pass it owns | Where it lives |
| --- | --- | --- | --- |
| `human` | the human | the decision gate: a preference, a spend, an irreversible or outward-facing act; the release | — |
| `builder` | the `builder` subagent, one per ticket | branch, build, verify, PR | `agents/builder.md` |
| `refiner` | the `refiner` subagent | `backlog` → `ready` (`on-hold` when parked) | `agents/refiner.md`, dispatched by `skills/tandem-refine/` |
| `reviewer` | the `reviewer` subagent | the merge gate, read through the ticket's lenses: `code`, `security`, `ui-design`, `ux`, `architecture` | `agents/reviewer.md`, `skills/review-lenses/` |
| `consolidator` | the `consolidator` subagent | the sole writer of the curated bikeshed notes | `agents/consolidator.md`, dispatched by `skills/tandem-consolidate/` |
| `sprint` | the session's own agent | runs the loop — refine, build, review, retro, close | `skills/tandem-sprint/` (build phase: `skills/tandem-build/`) |

`sprint` and `human` fill no subagent — the sprint is a procedure the session runs, the human is the decision. `retro` is not a role: it is a skill (`skills/retro/SKILL.md`) that runs at every sprint close and on human request, filing `backlog` tickets into the project's lane, and into `Retro` only when no project owns the finding. Capture is not a role either: it is the session rule in `AGENTS.md`.

## The name rule

One vocabulary, two fields. A role's name goes in `--assignee` when it owns a ticket and in `--author` when it speaks on one.

- `--assignee <role>` — the owner: `tandem-cli ticket set --id <id> --status in-progress --assignee builder`. `--assignee self` expands to `$TANDEM_AGENT`; `--assignee human` hands a decision to the human.
- `--author <role>` — the speaker: `tandem-cli ticket comment add --id <id> --author reviewer --body …`.
- **`--author self` writes the four letters `self`.** The CLI expands `self` for `--assignee` and `--origin` only. Write the role out; `--author self` names nobody.
- **Omitting `--author` is the unnamed author `""`** — a gap in the record, not a style choice.

## The minutes

The comment log on a ticket is the team's minutes — append-only, no edit, no delete; a correction is a new comment. The description is the brief: written once and appended to, never overwritten. Evidence a reader must act on goes in the description too; the comment is the conversation. The regular producer is the reviewer's verdict; any role that speaks on a ticket writes as itself. A sprint's run log is the record's, not a ticket's, and it is not part of the minutes.
