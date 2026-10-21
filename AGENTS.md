# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first.** Load the `tandem-memory` skill, then read `Index.md`, the project's curated note under `projects/<project>/`, and `tandem-cli todo list`. Do this before touching project files.
- **Record captures as you work.** File one with `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> --project tandem` at each non-obvious decision or gotcha, and finish with one `work` note. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Applies to sub-agents too.** A child session receives this section like the root does, so a brief need not restate it — give the child its task, nothing more.
- **Consolidate on request.** Consolidation is deliberate, never automatic: at session close the captures simply keep. When the human asks (or the root was told to close out), hand the saddlebag to a fresh **consolidation agent** — a separate agent whose only job is to fold the captures into the curated notes.

How a particular host wires the consolidation agent — its name, its trigger — belongs to that host's own configuration, not this file. This repo names the agent (`agents/consolidator.md`) and ships the skill that dispatches it (`skills/tandem-consolidate/`).

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Sessions

Every session records itself as a ticket, so the board shows what is running and what each session did. The ticket is work, not a log entry: a board pass refines it and moves it like any other ticket.

- **File it at session start**, with a brief — what this session is for and what done looks like:

  ```
  tandem-cli todo add --title "Session — <project> · <session-id>" --status in-progress [--assignee <agent>]
  tandem-cli todo set --id <ticket-id> --description -
  ```

  `<project>` is the codebase the session works in (`tandem`, `fleet`), `<session-id>` is `$OPENCODE_SESSION_ID` (a short token when the host exports none), and `<agent>` is your agent's name when the session tells you — omit the flag when it does not.

- **One ticket per session.** If `tandem-cli todo list --text "<session-id>"` already shows one, file nothing.

- **Close it at session end**, before your final reply: `tandem-cli todo set --id <ticket-id> --status done --description -`. The ticket id is the `id` `todo add` returned; find it again with `todo list --text "<session-id>"`. A session that resumes reopens it: `tandem-cli todo set --id <ticket-id> --status in-progress`.

- **The description is written once and appended to, never overwritten.** It opens as the brief and gains the outcome at the end, under a heading of its own. Read it, add the outcome below it, write it back — the brief is what a board pass or the next session reads first.

- **Empty stdin clears the description.** `--description -` takes the body on stdin, and an empty pipe writes an empty body, which the store normalises to nothing — the description is gone, exit 0, no warning. So a command that pipes nothing is a command that erases the brief. Never run `--description -` without a body, and check what you are piping. When there is no outcome to record, close the ticket with `tandem-cli todo complete --id <ticket-id>`, which leaves the description alone.

- **Children file their own.** A sub-agent session is a session; it closes its ticket when it finishes.

- **A board pass moves a session ticket like any other.** It refines the brief, and it dispatches or completes it. Two rules distinguish a live session from a dead one, and the session id in the title is what settles both: `opencode api session.list` shows each session's `outcome` and `time.updated`. A session updated seconds ago is running; a finished one carries an `outcome` of `succeeded`, `failed` or `interrupted`. A ticket whose session is live belongs to that session — leave it alone. A ticket whose session has ended is stale work: refine it and move it, so a dead session's work does not die with it. The board skill has the full rule.

Session tickets live in the default area, so they never depend on a lane that may not exist.

## Agents

This repo owns the agent prompts in `agents/` — `consolidator.md`, `refine.md`, `retro.md`, `reviewer.md` — and the procedures in `skills/` that a session runs. They are the passes that run on Tandem, and the contracts they state are the ones this repo ships: the CLI in `Sources/tandem-cli/`, the skill in `skills/tandem/`, the bikeshed protocol in `skills/tandem-memory/`. A prompt that names a contract states the current one — see [ADR 0026](docs/adr/0026-agent-prompts-and-portable-setup.md).

The `tandem-setup` skill wires all of it into an agent host: the skills, the agent prompts and this repo's `AGENTS.md`, linked rather than copied. Nothing here names a path on one machine: a clone of this repo, anywhere, plus the app, is the whole setup.
