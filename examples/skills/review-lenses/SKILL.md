---
name: review-lenses
description: "The review lenses, the checklists applied to a change: code, security, ui-design, ux, architecture, docs. Reach for it when reviewing a PR, when writing a brief's `## Lenses` section, or when adding a lens to the set."
---

# Review lenses

A lens is a checklist, not a reviewer. The refiner reads the chosen lenses while writing the brief, so their constraints land in `## What` before the build; the reviewer applies the same lenses to the diff. The merge gate stays one: one verdict line and one door class, held by `agents/reviewer.md`.

The refiner picks the lenses from the change's surface and writes them in the brief's `## Lenses` section, after `## What`, one clause each on why. A brief with no `## Lenses` is reviewed with `code` alone.

**The floor**: every brief names `code`, `security` and `architecture`. `ui-design` and `ux` are added when the change's surface earns them. `docs` is added, like them, when the change touches a documented surface: CLI flags or output, an API, config, install paths, the shape of stored data, agent or skill prompts, an ADR; it is not part of the floor. A named lens with no target in the change says so in one clause.

## The lenses

| lens | file | what it reads |
| --- | --- | --- |
| `code` | `agents/reviewer.md` | correctness, edges, breakage |
| `security` | `security.md` | secrets and tokens, shell and SQL interpolation, input that reaches a path, writes that lose or expose store data, `--force` and permission changes |
| `ui-design` | `ui-design.md` | the UI's visual language: design tokens instead of literals, spacing, type, colour, dark mode, sizing |
| `ux` | `ux.md` | interaction and recovery: keyboard paths, destructive acts without undo or confirmation, empty and error states, the human's round-trips |
| `docs` | `docs.md` | docs kept true by the diff: the brief's docs list, the PR body's `## Docs` section |
| `architecture` | `architecture.md` | layering, one truth per fact, schema and migration discipline, the ADRs the change touches |

Every lens file has three headings: `## Read` (what in the change it looks at), `## Blocks on` (findings that must change first, each citing a repo path or an ADR), and `## Never blocks on` (taste calls: a body comment, then merge). A lens with no basis for a judgement says nothing rather than guessing; `ui-design` and `ux` judge no screen they cannot see.

A lens names the kind of file it reads, and the repo's `AGENTS.md` names where each lives: the UI and its design tokens, the layers, the store's writer and schema, the path guards, the docs.

## Adding a lens

One file and one row, in the same PR.
