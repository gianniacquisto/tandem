# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first** — load the `tandem-memory` skill, then read `Index.md`, the project's **Overview** (`projects/<project>/<project>.md`, the anchor of its fixed scaffold), and `tandem-cli ticket list`. Do this before touching project files.
- **Record captures as you work** — `tandem-cli note capture --kind <decision|gotcha|fact|thread> --project tandem`, but only when the information would be lost at session end and re-deriving it costs real effort. There is no floor `work` note: the summary at `session end` (ADR 0027) is the work record. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Consolidate on request** — the captures keep at session close; when the human asks, `skills/tandem-consolidate/` dispatches the pass.
- **Applies to sub-agents too** — a child session receives this section like the root does, so a brief need not restate it.

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Sessions

A session is a record, not a ticket (ADR 0027): one record per session, keyed by `$OPENCODE_SESSION_ID`, in its own file — `sessions.json` at Tandem's home — and never a ticket on the board. The CLI's five verbs are `session start`, `session link`, `session end`, `session list` and `session show`; the two states are `running` and `ended`.

- **Start** — `tandem-cli session start --agent <pass> --brief -`, the brief on stdin: what this session is for and what done looks like. `<project>` defaults to the working directory's last path component; `--agent` is never omitted — every session knows its pass, and a record without it cannot attribute the pass and signs the usage comment its end posts `usage`.
- **Carry tickets** — `tandem-cli session link --ticket <id>` for each ticket the session carries. A child session is a session: it starts its own record and links what it carries.
- **End** — before your final reply: `tandem-cli session end --outcome <succeeded|failed|interrupted|unknown> --summary -`, one line on what landed. On this host, end through the usage wrapper instead: `Scripts/usage-report.sh --outcome <…>` (summary on stdin) — it reads the session's tokens, cost and model from opencode's database, reports them on the record, and the CLI posts one usage comment — signed by the session's agent, `usage` when the record carries no agent — on every ticket the session linked. When the wrapper reports no row (exit 1), fall back to the plain `session end`.
- **A session that dies leaves a `running` record and nothing to reclaim.** The board holds only work, so no pass reclaims the rows of a dead session — what the session carried, its record's `tickets` names.
- **Stale work is still work.** A ticket claimed by a session that has ended is picked up by the next sprint's triage; the live-vs-ended rule and the dispatch live in `skills/tandem-sprint/SKILL.md`.

Tickets keep their own conventions: a ticket's description is written once and appended to, never overwritten. `--description -` takes the body on stdin, and an empty or whitespace-only body is a domain failure: exit 1, the description left as it was. Pass `--description ""` when you mean to clear it. A session that resumes reopens its carried tickets: `ticket set --id <id> --status in-progress`.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee — instead of leaving the next move to a later pass. The names are the roster's: `skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`; the sprint session dispatches and records.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `agents/` — `builder`, `consolidator`, `refiner`, `reviewer`, one markdown file each, named as nouns — and the procedures they run in `skills/`. A prompt that names a contract states the current one; see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md). `skills/tandem-setup/` wires it all into an agent host: the skills, the agent prompts and this `AGENTS.md`, linked rather than copied. Nothing here names a path on one machine: a clone of this repo, anywhere, plus the app, is the whole setup.
