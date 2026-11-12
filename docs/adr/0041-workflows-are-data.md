# Workflows are data; a sprint is the workflow's current run

The board's columns were a fixed six: `Status::WORKFLOW` in the core and `STATUSES` in the renderer were the only source. Adding a column meant a build of the core, the CLI, MCP, the wire and the board. The human's ask is to add or change a workflow without redeveloping the app, and TAN-327's "Selected for development" column was the first case.

Decided 2026-11-08 (TAN-328 built the foundation).

## Decision

- **The workflow is the entity.** A workflow has a UUID, a name, a scope (`home` today), an author, timestamps and an ordered stage list. A stage has a stable key, an editable label, one of six roles (`intake`, `parked`, `ready`, `active`, `review`, `finished`) and an optional binding to a roster role and one of the three passes (`refine`, `build`, `review`).
- **The stage set is data.** `workflows` and `workflow_stages` are schema v7. The seeded `agile` default carries the six keys the board already drew, in the board's order after UI-15: `backlog`, `on-hold`, `ready`, `in-progress`, `in-review`, `done`. A fresh home and the v6 migration seed it alike, so every existing ticket's status is a default stage and no ticket moves.
- **The active workflow is a settings row.** `active_workflow` holds the workflow's id. A missing or unreadable row reads as the default, so a broken row never hides the board. `projects.workflow` is a nullable column for the later per-project adoption; this record does not activate per-project column sets.
- **The board reads the workflow.** The board's columns, the list's status filter and the sheet's status menu are the active workflow's stages, in stage order. A custom stage created with `workflow set` appears without a code change. `ticket add --status` and `ticket set --status` validate against the ticket's workflow and refuse an unknown key with the valid list.
- **The verbs are the CLI's and MCP's.** `workflow list`, `workflow show`, `workflow set` (the whole document from stdin, atomic), `workflow use` and `workflow delete`; the MCP door mirrors them. `workflow set` validates the document as one write; `workflow use` is refused while a sprint is open and while tickets sit in stages the target lacks, naming the counts. The default, the active workflow and a workflow with runs cannot be deleted.
- **The C model is the target.** One workflow entity holds its current run; past runs are the `workflow_runs` history, and the sprint CLI group becomes `workflow run` verbs or aliases. This record lands the workflow entity, the default, the verbs and the board read. TAN-332 turns `sprints` into `workflow_runs` (schema v8) and adds the run verbs; TAN-324 makes the runner and the sprint skill consume the workflow's stages and bindings.
- **The editor is a Settings section.** It is TAN-333, not a tab, and orchestration stays outside the app.

## Amendments

- **[ADR 0017](0017-the-ticket-model.md): statuses become workflow data.** The six values move from a closed enum to the default workflow's keys. A `Custom` status carries the stored key, so a workflow's stage round-trips instead of reading as `backlog`. Free-form transitions stay: the board validates the value, never the move, and the runner enforces its own sequence.
- **[ADR 0032](0032-sprint-records.md): a sprint is the workflow's current run.** The sprint record stops being a peer of the workflow. TAN-332 migrates the table; the sprint's history becomes the workflow's `workflow_runs` history. Until then the `sprints` table is untouched and `workflow use` refuses while one is open.
- **[ADR 0039](0039-projects-own-lanes.md): `projects.workflow` is the later hook.** Projects still own lanes; a project's workflow is a nullable column this record adds but does not read.
- **[ADR 0040](0040-no-embedded-agent-loop.md): the editor is a Settings section.** No Workflow face returns; configuration is a Settings section, and orchestration remains the CLI's and the MCP server's.
- **TAN-327: the default's seven stages.** The default `agile` workflow gains a second intake stage, `selected` (`Selected for development`), immediately before `ready`; its final order is `backlog`, `on-hold`, `selected`, `ready`, `in-progress`, `in-review`, `done`. The v7 seed stays the six; a new schema migration (the next free version after the foundation) inserts the stage only into an untouched six-key default, leaves a customized workflow untouched, and is idempotent when `selected` already exists. `workflow set` accepts **at least one intake** and refuses zero. The **first intake** in stage order is the new-ticket landing stage, so a fresh home still lands `backlog`; the intake stage **nearest Ready** is the refine queue, and earlier intake stages stay the pool. Promotion into the queue is human-only: the board or the CLI moves a ticket in, refine drains it, and sprint triage reports it as one count without drawing it.

## Consequences

- A custom stage is a key an older build does not know, and that build maps it to `backlog`. The reader-first rule applies: install the build carrying this change before writing a custom stage.
- `workflow set` refuses a document that deletes a stage holding tickets, names the stage and the count, and writes nothing on any refusal. A key change is a new stage and a move, never a rename in place.
- A workflow missing the `ready`, `active` or `review` role activates on the board but is not runnable; the runner refuses it when TAN-324 consumes the roles.
- TAN-327 adds the default's second intake stage as an append-only migration; a customized default is never rewritten, and the human adds `selected` through Settings or `workflow set`.
