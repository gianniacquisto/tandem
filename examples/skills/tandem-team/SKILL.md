---
name: tandem-team
description: "The roster: which agent fills which role, and what to write in a ticket's `assignee` and a comment's `author`. Reach for it when you need to know who owns a pass, what to write as a ticket's assignee or a comment's author, or who the team is."
---

# Tandem team

A template: the roster below is this repository's own. Replace it with your team, and keep the name rule.

| Role | Owns | Lives in |
| --- | --- | --- |
| `human` | the decisions: a preference, a spend, an irreversible or outward-facing act; the release | |
| `refiner` | `backlog` to `ready` (or `on-hold`) | `agents/refiner.md`, run by `skills/tandem-refine/` |
| `builder` | one ticket: branch, build, verify, PR, `in-review` | `agents/builder.md` |
| `reviewer` | the merge gate: merge, escalate or block | `agents/reviewer.md` |
| `sprint` | the batch: draw, dispatch, gate, close | `skills/tandem-sprint/` |

`retro`, `tidy` and `interview` are skills the human invokes, not roles: each signs as the role that runs it, and `retro` files with `origin` retro. The review lenses (`skills/review-lenses/`) are checklists the refiner and reviewer read, not roles.

## The name rule

A role's name goes in `assignee` when it owns a ticket and in `author` when it speaks on one.

- `assignee`: `self` expands to the caller's actor; `human` hands the ticket to the human, and only while a question is open.
- `author`: write the role out. `self` is not expanded here and names nobody; an omitted `author` is an unnamed gap in the record.

The comment log is the minutes: append-only, a correction is a new comment. The description is the brief: written by the refiner and the human, appended to and never overwritten.
