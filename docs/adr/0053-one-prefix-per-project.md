# One prefix per project

> **Status:** In force. Amends [ADR 0039](0039-projects-own-lanes.md) and [ADR 0047, lanes are labels](0047-lanes-are-labels.md). See [the index](README.md).

A ref named the label: its key and a per-label counter. Project Tandem issued `TAN-`, `UI-`, `COR-`, `CLI-`, `MCP-`, `SHE-`, `SKI-`, `DOC-` and `CUT-` refs, so a ref read alone suggested a repo that did not exist, and a move to another label re-issued the ref, so a ref a human had written down answered unknown (TAN-316). A label is a way to organise tickets inside a project. This record makes the project own the one prefix and the one counter, and makes the ref stable for as long as the ticket stays in its project.

Decided 2026-11-20 (TAN-339).

## Decision

- **The project owns the key and the counter.** A project has a `key` (uppercase letters and digits, unique case-insensitively across projects and across every retired prefix an alias holds) and a `last_number` that only rises. The ref is the ticket's own project key and its project-wide number (`TAN-412`), derived on read and never stored. A key is frozen once the project has issued a number.
- **A ticket stores its project.** The project is set at creation, never changes, and is never empty. This replaces ADR 0039's "a ticket's project is derived via its label, never stored".
- **A label is optional, and has one parent.** It keeps its name, its render order and its parent project, and loses its key and counter. A ticket has at most one label, which must belong to the ticket's project. The word stays "label" (no "epic").
- **There is no default area.** Every creation path (`ticket_add`, `tandem-cli ticket add`, the board's New form) names a project, or a label that implies one. With neither, the refusal lists the projects (`Name a project: Tandem, code-rank, ... (or a label).`), and on a home with no project it says `No project yet. Add one with project_add first.` This holds on a home with exactly one project: there is no implicit default. `label_add` requires a project the same way.
- **Moves.** A move or drag to another label of the same project, or to no label, keeps the number and the ref. A move or drag to a label of another project is refused, naming the two projects. `label_rename` with a different project is refused while the label holds any ticket. `label_delete` unlabels its tickets inside their project, so no ref changes. `project_delete` is refused while the project holds labels or tickets.
- **Aliases for the refs the migration retired.** Schema v16 adds `ticket_aliases (reference, ticket_id, author)`. A live ref resolves first, by project key and number; a retired ref (`CUT-34`) then resolves through the alias to its ticket, the write succeeds, and the answer carries the current ref. `project_add` and a key change refuse a key equal to any retired prefix, so a ref never means two tickets. The aliases are written by the migration only.
- **Schema v16.** `projects` gains `key` and `last_number`, `tickets` gains `project`, and `labels` loses `key` and `last_number` so no second copy of the counter survives. The re-key, per project: the key is the key of the label named like the project when it has one, otherwise the key of the label with the highest counter (ties go to label position), and a project with no label gets a generated key; the counter starts at that label's counter and that label's tickets keep their numbers; every other numbered ticket, done and archived included, takes the next number in `created_at` order and then by id, and its old ref becomes an alias. Tickets that never had a ref (the default area) are homed unlabelled in the first project in render order (a home with no project at all gets a created "Inbox"), numbered after the rest, with no alias. The migration runs in the seam's one transaction, loses no row, and a failure leaves the file at v15. A read of a file below v16 runs the same arithmetic in memory, so a read before the first write shows the refs the write will leave.
- **A clean cut for the contract.** `label_add` and `label_rename` drop `abbr` and refuse an unknown argument, so a stale `abbr` fails out loud instead of being ignored. `project_add` and `project_rename` gain an optional `abbr`. `ticket_add` gains `project`, `ticket_list` gains a `project` filter, and `label: ""` now means "no label". A ticket answer carries `project`. `tandem-cli label add` and `label rename` lose `--abbr`; `project add` and `project rename` gain it. The board's label forms lose the key input, and its New form gains a required Project select and an optional Label select.

## Amendments

- **[ADR 0039](0039-projects-own-lanes.md)**: its "refs stay lane-keyed" and "the default area stays project-less capture" no longer hold. Refs are project-keyed, and there is no default area. Its "lane" is today's label.
- **[ADR 0047, lanes are labels](0047-lanes-are-labels.md)**: its "a label has a name, a key and an issued-number counter" and "deleting a label returns its tickets to the default area" no longer hold. The key and counter belong to the project, and a delete unlabels.

## Why

- **One prefix per project.** The project is what a human names a body of work by. A prefix per label made one project speak with nine voices.
- **A ref that does not move.** Numbers belong to the project, so a label change cannot re-issue one. A ticket never changes project, so the ref is stable for the ticket's life.
- **The retired refs still resolve.** 175 commit subjects and 64 repo files cite refs such as `CUT-34`. A grandfathered scheme would keep two prefix schemes forever, and a re-key without aliases would point those citations at nothing.

## Consequences

- History keeps its word: ADRs, commits and code comments that cite a retired ref are not rewritten, because the alias resolves them.
- The Hosting label folds into project Tandem before a build carrying v16 opens the home (TAN-372), because re-parenting a label that holds tickets is refused after.
- The two fixture homes were regenerated at v16, and their seeds name projects.
