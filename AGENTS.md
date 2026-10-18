# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first.** Load the `tandem-memory` skill, then read `Index.md`, the project's curated note under `projects/<project>/`, and `tandem-cli todo list`. Do this before touching project files.
- **Record captures as you work.** File one with `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> --project tandem` at each non-obvious decision or gotcha, and finish with one `work` note. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Applies to sub-agents too.** A child session receives this section like the root does, so a brief need not restate it — give the child its task, nothing more.
- **Consolidate at close.** Hand the saddlebag to a fresh **consolidation agent** — a separate agent whose only job is to fold the captures into the curated notes.

How a particular host names and launches the consolidation agent — its agent name, its command — belongs to that host's own configuration, not this file.

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.
