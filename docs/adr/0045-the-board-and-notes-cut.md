# The board and notes cut: two surfaces before one binary

Tandem grew a board, a notes store, sessions, sprints, workflow runs, a harness bridge, an agent installer, a block editor and a screenshot catalogue. The next step is one HTTP server in a single binary, and carrying every surface into it would multiply the work that migration cannot pay for. The product is cut to two surfaces: the one-axis board and the folder-and-wikilink notes store. Every later ticket in the `cut` lane deletes a slice of the rest; this record is the decision the deletions answer to.

Decided 2026-11-13 (CUT-1).

## What is retired

- **Sessions.** The `sessions` table, the five `session_*` verbs and the session and usage faces go. A pass is recorded in the ticket comment log, not in a parallel record.
- **Usage.** The usage report, token and cost attribution, and the harness session reader that filled them go with the session record.
- **Sprints and workflow runs.** The `sprints` and `workflow_runs` tables, the sprint and run verbs, and the close ceremony go. A sprint is a human's batch, not a stored entity.
- **Preflight.** The preflight domain, its checks and its suite go. The build and review contract lives in prose and in CI.
- **The harness reader.** The loopback read of an editor's session database, `session end --usage-from-harness` and its fixtures go. The app ships no agent loop and no reader of one.
- **`setup install` and the bundled agent face.** The installer verbs, the `agent/` resource in the artifact, the host symlinks, the face diagnostics and the setup suite go. The prompts and skills become examples; they are no longer an installed product surface.
- **Workflows as data.** The `workflows`, `workflow_stages` and `workflow_runs` tables, the workflow verbs, the active-workflow setting and `Status::Custom` go. `Status` returns to six fixed stages.
- **Settings and roster.** The settings face and the roster of pass roles go. Board state that a reader needs stays in the renderer; the store keeps work, not preferences.
- **Completion undo.** The undo affordance around `ticket complete` goes. Completion is a normal status write, corrected by another status write.
- **Ticket-to-note promotion.** `tickets.link`, the paperclip and the promotion of a ticket into a Bikeshed note go. A note is a note and a ticket is a ticket.
- **The Ticket List UI face.** The list, its scopes, its status filter, its lane grouping and its quick add go. The board and its filter bar are the one working face.
- **The Bikeshed memory layer.** The per-project `.raw/` capture folders, `note capture`, `note prune`, `session_notes`, the fixed scaffold (`front`, `daily`) and the capture fixtures go. The notes store keeps notes and wikilinks, not a memory protocol.
- **The React block editor.** The block editor and its dependencies (`@lezer/markdown`, `katex`, `mermaid`), the note image assets and the editor's preview pipeline go. A `<textarea>` with a preview replaces it.
- **The screenshot catalogue.** The committed shots catalogue, its goldens and the capture workflow go. Screenshot evidence is a scratch artifact, never a product surface.

## What stays

- **The 7-table SQLite store**: `tickets`, `ticket_dependencies`, `comments`, `lanes`, `projects`, `store_meta` and `schema_migrations`. No settings, sessions, workflows or usage tables remain beside them.
- **The 6 fixed `Status` stages**: `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done`. The `selected` stage added as workflow data goes with the workflow tables.
- **The board and its filter bar**: one column per stage, a lane chip on the card, and the project, lane, assignee and search filters.
- **The folder-and-wikilink notes store**: notes as markdown files in folders, `[[wikilink]]` resolution and backlinks, edited in a `<textarea>` with a preview.
- **The CLI**: the board and notes verbs, with the retired groups removed.
- **The MCP server**: the board and notes tools, expanded to expose the normal note verbs now that the capture layer is gone.
- **The ticket model and dependency graph**: the three axes and the `ticket_dependencies` table stand; the edge is written in the ticket transaction, and the block is derived on read.

## Prior records

- **[ADR 0012](0012-editor-formatting.md)** is superseded. The formatting toolbar and inline live preview described there belong to the editor that this cut retires.
- **[ADR 0016](0016-note-capture.md)** is superseded. The capture verb and the hidden `.raw/` layer are retired; notes are written through the normal note verbs.
- **[ADR 0026](0026-agent-prompts-and-portable-setup.md)** is superseded. The prompts and skills survive as examples under `examples/`, not as a face a setup command installs.
- **[ADR 0027](0027-session-records.md)** is superseded. A pass is recorded in its ticket comments; the session table and its verbs are retired.
- **[ADR 0029](0029-the-ceremony-set.md)** is superseded. The roster, sprint cycle and retro ceremonies go; the ticket comment log survives as the work record.
- **[ADR 0032](0032-sprint-records.md)** is superseded. The sprint row and its close go with the sprint surface.
- **[ADR 0034](0034-agent-face-in-the-artifact.md)** is superseded. The artifact carries the core and its UI, not an agent face.
- **[ADR 0035](0035-ticket-dependencies.md)** is amended. The `ticket_dependencies` table, the derived block and the write-time refusals stand; its `tickets.link` reference and its sprint-draw clause retire with the surfaces that owned them.
- **[ADR 0038](0038-the-harness-home.md)** is superseded. The app ships no harness, so the harness home and its reader retire.
- **[ADR 0041](0041-workflows-are-data.md)** is superseded in its data half. The workflow and stage tables, the workflow verbs and `Status::Custom` retire; the board draws the six fixed stages again. [ADR 0043](0043-the-board-draws-one-axis.md)'s one-axis board stands, with fixed stages in place of the active workflow's stages.
- **[ADR 0042](0042-the-bikeshed-editor-is-a-block-editor.md)** is superseded. The block editor and its dependencies retire; the `<textarea>` and preview take the editor's place.
- **[ADR 0044](0044-agent-skill-lens.md)** is amended. The lens table, the specialist readers and their comments stand as examples under `examples/`; the repo-root prose pin suite (`rust/core/src/tests/scaffold_convention.rs`) retires with this cut, so the drift it caught is a reviewer's read.

The records not named here stand, including [ADR 0017](0017-the-ticket-model.md)'s ticket model, [ADR 0031](0031-one-database-the-store.md)'s one store and [ADR 0040](0040-no-embedded-agent-loop.md)'s no embedded agent loop.

## Why

- **Two surfaces are one migration.** The single-binary server has to carry the board and the notes store. Every extra surface is a table, a verb, a wire type and a test that the migration must preserve or delete later.
- **A record that cannot disagree.** Sessions, usage and sprints each held a fact the ticket comments already held. Retiring them removes a second truth rather than a capability.
- **The notes store is the memory.** Captures, scaffolds and a memory protocol layered a second structure over folders and wikilinks. The store is simpler when the note is the unit and the folder is the scope.
- **An editor is not a product.** The block editor pulled a tree of rendering dependencies into the renderer. A `<textarea>` and a preview keep the markdown file as the source of truth with a fraction of the surface.

## Consequences

- Every later `cut` ticket deletes one slice of the retired list; this record is the contract those diffs answer to.
- A home migrated past this cut keeps only the seven tables. The migration that drops the retired tables and settings rows is a later ticket's, and the reader-first rule applies until it lands.
- The prompts and skills remain readable in `examples/` for the repository's own workflow, but the packaging stage reads them from there only until CUT-3 removes bundling.
- The CLI and MCP surfaces shrink: a script or agent that called a retired verb reads an unknown verb, and the release notes name the removals.
