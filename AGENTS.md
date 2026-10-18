# AGENTS.md

## Memory

This project's memory is the Tandem **bikeshed** (`~/.tandem/bikeshed/`), reached through the `tandem-memory` skill. Before you read or edit project files, and as you work:

- **Orient first.** Load the `tandem-memory` skill, then read `Index.md`, the project's curated note under `projects/<project>/`, and `tandem-cli todo list`. Do this before touching project files.
- **Record raw notes as you work.** File one with `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> --project tandem` at each non-obvious decision or gotcha, and finish with one `work` note. The curated bikeshed notes have one writer — the consolidation pass — so never edit them.
- **Brief every sub-agent the same way.** A child session starts fresh with no skill, so put the orient-and-record instruction in its brief and have it finish with one `work` raw note.
- **Consolidate at close.** Dispatch a consolidation pass to fold the raw notes into the curated notes.

The full protocol is in the skill: `SKILL.md`, `WRITE-PROTOCOL.md`, `CONSOLIDATION.md`.
