# Session tickets are work, not records: the board refines and moves them like any other ticket

A session ticket is the one kind of ticket the board refused to look at. `skills/tandem-board/SKILL.md` skipped every title beginning `Session —`, and `AGENTS.md` called them "records, not work" — so they never passed the refined gate, never got dispatched, and a board pass could not see that one of them had died mid-flight. The cost was already on the board: three `Session —` tickets sitting `in-progress`, one with no description at all, none of them reachable by the workflow meant to move work. A dead session's work died with it, because the ticket recording the session was the one thing the board would not pick up.

## Decision

- **A session ticket is a work item.** It carries a brief, it is refined, and it is moved through the same gates as any other ticket. The `Session —` prefix keeps the row identifiable; it no longer buys an exemption.
- **The description is written once and appended to, never overwritten.** It opens as the brief — what the session is for and what done looks like — and gains the outcome at the end, under a heading of its own. A board pass or the next session reads the brief first, so overwriting it would destroy the thing the ticket exists to carry.
- **Two liveness rules, one handle.** The session id in the title — `Session — <project> · <session-id>` — is the handle: `opencode api session.list` shows each session with an `outcome` and a `time.updated`. A finished session carries an `outcome` of `succeeded`, `failed` or `interrupted`; a live one shows `null` and was updated seconds ago. Both signals are needed: a session that died without recording an outcome still shows `outcome: null`, so `null` alone proves nothing. A ticket whose session is **live** belongs to that session and the board leaves it alone. A ticket whose session has **ended** is stale work, and the board refines it and moves it: `refined` → `ready` → dispatched to a successor, or `done` when the outcome already exists. The board's own "a claim is not a stall" rule governs the borderline: a quiet session is not a dead one.
- **A brief that was never written is raw material.** A session that died before writing one leaves a hole; the board reconstructs the brief from the transcript, the captures and the board itself, and says in the description that it reconstructed it.

## Why

- **An exemption is a hole by construction.** The skip was a reasonable reading of "a session ticket is a record of a session that ran" — and it made that record unreachable by the only mechanism meant to move work. The work a session did outlived the session; the ticket did not.
- **The id in the title is enough.** No schema change, no new field, no new verb. `--text` already finds the ticket by session id, and `opencode api session.list` already lists the sessions with a freshness stamp — the two meet exactly where the liveness question is asked.
- **Append, don't overwrite.** The board's own rule is that moving a ticket is honest only when the ticket carries the reason. A brief that is replaced by a one-line outcome is a ticket that has forgotten what it was for.
- **Reconstruct rather than skip.** A session that died before writing a brief is the case the exemption was supposed to protect and did not: the work is real, the record is thin, and an agent can fill the gap from the transcript.

## Decided against

- **A separate ticket kind for session records** — the store has no such axis, and inventing one is what created the gap.
- **Refining a session ticket whose session already closed** — a closed session's brief is already the brief; the board's job is stale work, not editing history.
- **A `session` field on the ticket** — the id is in the title, `--text` finds it, and a field is a schema change for no gain here.
- **Leaving the stale tickets where they are** — three were already wrong on the board. The change is what lets them be moved.

## Consequences

- The board's triage loop has one fewer exemption, and a session ticket is now the ordinary thing it looks like.
- A board pass can pick up a dead session's work and dispatch a successor to it. That is new behaviour, and the first pass that does it will show whether the reconstructed brief is good enough to build from.
- Sessions write a brief at the start and append an outcome at the end. That is more writing per session than a one-liner, and it is the price of a ticket the next session can act on.
- The store is untouched: no field, no enum, no migration. Everything here is prose in three files, plus this record.
