---
name: tandem-team
description: "The roster, which agent fills which role, what to write in `assignee` and `author`, and the minutes rule for the ticket comment log. Reach it when you need to know who owns a pass, what to write as a ticket's assignee or a comment's author, or who the team is."
---

# Tandem team

A template: the roster below is this repository's own. Replace it with your own team, and keep the name rule and the minutes rule.

| Role | Who | The pass it owns | Where it lives |
| --- | --- | --- | --- |
| `human` | the human | the decision gate: a preference, a spend, an irreversible or outward-facing act; the release | — |
| `builder` | the `builder` subagent, one per ticket | branch, build, verify, PR | `agents/builder.md` |
| `refiner` | the `refiner` subagent | `backlog` → `ready` (`on-hold` when parked) | `agents/refiner.md`, dispatched by `skills/tandem-refine/` |
| `reviewer` | the `reviewer` subagent | the merge gate, read through the ticket's lenses: `code`, `security`, `ui-design`, `ux`, `architecture` | `agents/reviewer.md`, `skills/review-lenses/` |
| `sprint` | the pass's own agent | runs the loop: refine, build, review, retro, close | `skills/tandem-sprint/` (build phase: `skills/tandem-build/`) |

`sprint` and `human` fill no subagent: the sprint is a procedure the pass runs, the human is the decision. `retro` is not a role: it is a skill (`skills/retro/SKILL.md`) that runs at every batch close and on human request, filing `backlog` tickets into the project's lane, and into `Retro` only when no project owns the finding.

## The specialists

A specialist reader owns one lens, not a ticket. It speaks in the minutes at refine and on the PR at review, and the reviewer keeps the one verdict.

| specialist | the lens it owns | where it lives | the rule |
| --- | --- | --- | --- |
| `architect` | `architecture` | `agents/architect.md` | speaks as `author`; owns no ticket; never merges |
| `security` | `security` | `agents/security.md` | speaks as `author`; owns no ticket; never merges |
| `ux` | `ux` | `agents/ux.md` | speaks as `author`; owns no ticket; never merges |

The lens table in `skills/review-lenses/SKILL.md` is the mapping: it names each lens's owner beside the file it reads.

## The name rule

One vocabulary, two fields. The pass table's owner can be an assignee; a specialist only speaks. A role's name goes in `assignee` when it owns a ticket and in `author` when it speaks on one.

- `assignee`: the owner: `ticket_set` with `id`, `status` in-progress and `assignee` builder. `self` expands to `$TANDEM_AGENT`; `human` hands a decision to the human.
- `author`: the speaker: `ticket_comment_add` with `id`, `author` reviewer and the words in `body`. A specialist sets `author` architect, security or ux, and is never an assignee.
- **An `author` of `self` writes the four letters `self`.** Only `assignee` and `origin` expand it. Write the role out; `self` as an author names nobody.
- **Omitting `author` is the unnamed author `""`**: a gap in the record, not a style choice.

## The minutes

The comment log on a ticket is the team's minutes: append-only, no edit, no delete; a correction is a new comment. The description is the brief: written once and appended to, never overwritten. Evidence a reader must act on goes in the description too; the comment is the conversation. The regular producer is the reviewer's verdict; any role that speaks on a ticket writes as itself. A pass's own handoff comment carries what landed, the outcome and next status, and any issues (`AGENTS.md`, Handoff). The sprint keeps no separate run log: the ticket comments are the record.
