# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first** — load the `tandem-memory` skill, then read `Index.md`, the project's curated note under `projects/<project>/`, and `tandem-cli ticket list`. Do this before touching project files.
- **Record captures as you work** — `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> --project tandem` at each non-obvious decision or gotcha, and finish with one `work` note. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Consolidate on request** — the captures keep at session close; when the human asks, `skills/tandem-consolidate/` dispatches the pass.
- **Applies to sub-agents too** — a child session receives this section like the root does, so a brief need not restate it.

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Sessions

Every session records itself as a ticket, so the board shows what is running and what each session did.

- **File it at session start**, with a brief — what this session is for and what done looks like:

  ```sh
  tandem-cli ticket add --title "Session — <project> · <session-id>" --status in-progress [--assignee <agent>]
  tandem-cli ticket set --id <ticket-id> --description -
  ```

  `<project>` is the codebase the session works in, `<session-id>` is `$OPENCODE_SESSION_ID`, and `<agent>` is your agent's name when the session tells you — omit the flag when it does not.

- **One ticket per session.** Check `ticket list --filter all --text "Session — <project> · <session-id>"` first; file nothing if one exists.

- **Close it at session end**, before your final reply: read the description, append the outcome below the brief, write it back — `ticket set --id <ticket-id> --status done --description -`. The description is written once and appended to, never overwritten. With no outcome to record, `ticket complete --id <ticket-id>`, which leaves the description alone.

- **An empty stdin is refused.** `--description -` takes the body on stdin, and an empty or whitespace-only body is a domain failure: exit 1, the description left as it was. Pass `--description ""` when you mean to clear it. A session that resumes reopens its ticket: `ticket set --id <id> --status in-progress`.

- **Children file their own.** A sub-agent session is a session; it closes its ticket when it finishes.

- **Stale work is work.** A ticket whose session has ended is picked up by the next sprint's triage; the live-vs-ended rule and the dispatch live in `skills/tandem-sprint/SKILL.md`.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee — instead of leaving the next move to a later pass. The names are the roster's: `skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`; the sprint session dispatches and records.

## Agents

The prompts for the passes live in `agents/` — `builder`, `consolidator`, `refiner`, `reviewer`, one markdown file each, named as nouns — and the procedures they run in `skills/`. A prompt that names a contract states the current one; see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md). `skills/tandem-setup/` wires it all into an agent host: the skills, the agent prompts and this `AGENTS.md`, linked rather than copied. Nothing here names a path on one machine: a clone of this repo, anywhere, plus the app, is the whole setup.
