# A ticket's area is a label, not a lane

The board, the CLI and the MCP tools called a ticket's single named area a "lane". It is drawn as a chip on the card and used as a filter ([ADR 0043](0043-the-board-draws-one-axis.md)), never as a swimlane, so the word described a drawing the board no longer makes. This ADR renames the word. The model does not change.

Decided 2026-11-14 (UI-37 built it).

## Decision

- **One word, everywhere live.** "Lane" and "cycle lane" become "label": the domain (`Label`, `Ticket.label`, `add_label`), the store (schema v12), the wire (`label.*` methods, the `label` JSON field), the CLI (`tandem-cli label ...`, `--label`), the MCP tools (`label_list|add|rename|delete|move`, the `label` argument), the served board and sheet (the Labels menu, the chip, the select), the glossary, the docs and the agent prompts.
- **The model is unchanged.** A ticket has at most one label, or sits in the unnamed default area. A label has a name, a key and an issued-number counter, and its key and number mint the ref (`TAN-42`). Every label has exactly one parent project. A key is unique case-insensitively and frozen once a ticket holds a number. Deleting a label returns its tickets to the default area.
- **Schema v12 renames in place.** `lanes` becomes `labels`, `tickets.lane` becomes `tickets.label` and `tickets_lane_position` becomes `tickets_label_position`, by `ALTER ... RENAME` in one migration transaction. Rows, keys, numbers, positions and project parents are untouched. The migrations that shipped keep the word, as shipped migrations are never edited. A read before a file's first write still answers on the v11 names, because a read never migrates ([ADR 0031](0031-one-database-the-store.md)). An older build refuses a v12 file on the version, like any newer seam.
- **A clean cut, no aliases.** `lane` and `--lane`, the `lane_*` tools, the `lane` argument and the `lane` JSON field answer as unknown. The repo is one human and its agent prompts change in the same PR, so an alias would leave two vocabularies.
- **History keeps its word.** The ADRs before this one and `docs/epic-lanes.md` record what was true when they were written and are not rewritten. A repo check (`rust/core/tests/label_vocabulary.rs`) fails on a "lane" in any other live file.

## Amendments

- **[ADR 0039](0039-projects-own-lanes.md)**: its "lane" is today's label. A project holds several labels; the decision stands as written.
- **[ADR 0043](0043-the-board-draws-one-axis.md)**: its "lane" is today's label, the chip on the card and the filter in the header.

## Why

- **The word named a drawing the board no longer makes.** A lane is a row of a swimlane view. The board draws one column per stage and a chip for the area, which is what a label is.
- **Two vocabularies cost more than one migration.** A half-renamed wire, store or CLI neither builds nor passes the goldens, and an alias keeps the retired word alive in every prompt that copies it.

## Consequences

- Every ticket, project and CLI golden moved vocabulary and nothing else; `Fixtures/goldens/lane/` is `Fixtures/goldens/label/`.
- The fixture homes were regenerated at schema v12.
- A prompt or script that still says `lane` fails with the CLI's usage refusal (exit 2) or an unknown-tool error, which names the retired word at the call site.
