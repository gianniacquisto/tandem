# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first.** Load the `tandem-memory` skill, then read `Index.md`, the project's curated note under `projects/<project>/`, and `tandem-cli todo list`. Do this before touching project files.
- **Record captures as you work.** File one with `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> --project tandem` at each non-obvious decision or gotcha, and finish with one `work` note. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Applies to sub-agents too.** A child session receives this section like the root does, so a brief need not restate it — give the child its task, nothing more.
- **Consolidate at close.** Hand the saddlebag to a fresh **consolidation agent** — a separate agent whose only job is to fold the captures into the curated notes.

How a particular host names and launches the consolidation agent — its agent name, its command — belongs to that host's own configuration, not this file.

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.

## Sessions

Every session records itself as a ticket, so the board shows what is running and what each session did.

- **File it at session start**, then say what the session is for:

  ```
  tandem-cli todo add --title "Session — <project> · <session-id>" --status in-progress [--assignee <agent>]
  tandem-cli todo set --id <ticket-id> --description "<one line: what this session is for>"
  ```

  `<project>` is the codebase the session works in (`tandem`, `fleet`), `<session-id>` is `$OPENCODE_SESSION_ID` (a short token when the host exports none), and `<agent>` is your agent's name when the session tells you — omit the flag when it does not.

- **One ticket per session.** If `tandem-cli todo list --text "<session-id>"` already shows one, file nothing.

- **Close it at session end**, before your final reply: `tandem-cli todo set --id <ticket-id> --status done --description "<one line: what this session did>"`. The ticket id is the `id` `todo add` returned; find it again with `todo list --text "<session-id>"`. A session that resumes reopens it: `tandem-cli todo set --id <ticket-id> --status in-progress`.

- **Children file their own.** A sub-agent session is a session; it closes its ticket when it finishes.

- **These are records, not work.** A board pass skips any ticket whose title starts with `Session —`.

Session tickets live in the default area, so they never depend on a lane that may not exist.
