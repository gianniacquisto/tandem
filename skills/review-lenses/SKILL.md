---
name: review-lenses
description: "The review lenses, the checklists the reviewer applies to a diff: code, security, ui-design, ux, architecture. Reach for it when reviewing a PR, when writing a brief's `## Lenses` section, or when adding a lens to the set."
---

# Review lenses

A lens is a checklist the existing `reviewer` applies to the diff. It is not an agent and not a new pass: one merge authority, one verdict line, one door class, as `agents/reviewer.md` states them.

The refiner picks the lenses for a change and writes a `## Lenses` section in the brief, after `## What`: the lens names from the table below, one clause each on why. The choice comes from the change's surface, not the ticket's title. A brief with no `## Lenses` is reviewed with `code` alone.

## The lenses

| lens | file | what it reads |
| --- | --- | --- |
| `code` | `agents/reviewer.md` | the gate's own reading: correctness, edges, breakage |
| `security` | `skills/review-lenses/security.md` | secrets and tokens, shell and SQL interpolation, input that reaches a path, writes that lose or expose store data, `--force` and permission changes |
| `ui-design` | `skills/review-lenses/ui-design.md` | the app's visual language: `Sources/Tandem/LigneTheme.swift` tokens instead of literals, spacing, type, colour, dark mode, popover and sheet sizing |
| `ux` | `skills/review-lenses/ux.md` | interaction and recovery: keyboard paths, destructive acts without undo or confirmation, empty and error states, the human's round-trips |
| `architecture` | `skills/review-lenses/architecture.md` | layering, one truth per fact, schema and migration discipline, the ADRs the change touches, the reader-first order |

This table is the single source of truth: a lens is a row plus a file, and the two cannot drift. `code` has no file of its own because `agents/reviewer.md` already carries that reading, and a copy would be a second contract to keep in step.

## What a lens may do

Every lens file has the same three headings:

- `## Read`: what in the diff the lens looks at.
- `## Blocks on`: the findings that must change first, a `REQUEST_CHANGES` review, per `agents/reviewer.md` Commenting.
- `## Never blocks on`: the taste calls, a body comment and merge, per "Nits do not veto".

Every `## Blocks on` entry cites a repo path or an ADR, never generic advice, and cites `file:line` in the finding itself. A lens with no basis for a judgement says nothing rather than guessing: `ui-design` and `ux` read the SwiftUI source and any screenshot the PR attaches, and judge no screen they cannot see.

A lens whose file cannot be read is named in the review body, and the gate falls back to `code`: a missing lens is never a silent pass.

## Adding a lens

One file and one row, in the same PR, under the three headings above. A file with no row and a row with no file are both drift, and the pin in `Tests/TandemCoreTests/ScaffoldConventionTests.swift` fails on either.
