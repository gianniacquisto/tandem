---
name: review-lenses
description: "The review lenses, the checklists the reviewer applies to a diff: code, security, ui-design, ux, architecture. Reach for it when reviewing a PR, when writing a brief's `## Lenses` section, or when adding a lens to the set."
---

# Review lenses

A lens is a checklist one reader applies to a diff. It is not a pass and not a ceremony: the merge gate stays one, with one verdict line and one door class, as `agents/reviewer.md` states them. A lens may have a specialist reader (`architect`, `security`, `ux`); the reader owns the lens, and the reviewer keeps the verdict, the merge and the escalation.

The refiner picks the lenses for a change and writes a `## Lenses` section in the brief, after `## What`: the lens names from the table below, one clause each on why. The choice comes from the change's surface, not the ticket's title. A brief with no `## Lenses` is reviewed with `code` alone.

## The floor

Every brief names the floor: `code`, `security` and `architecture`, one clause each on why. `ui-design` and `ux` are chosen when the change's surface earns them. This is the one home of the floor; `agents/refiner.md` and `agents/reviewer.md` point here and do not restate the list.

A lens is named either way. A named lens with no target in the change says nothing, and the brief records that. A specialist is dispatched only when its lens has a target; when it cannot be dispatched, the brief names the fallback and the pass owner applies the lens. Never a silent skip.

## The lenses

| lens | file | owner | what it reads |
| --- | --- | --- | --- |
| `code` | `agents/reviewer.md` | `reviewer` | the gate's own reading: correctness, edges, breakage |
| `security` | `skills/review-lenses/security.md` | `security` | secrets and tokens, shell and SQL interpolation, input that reaches a path, writes that lose or expose store data, `--force` and permission changes |
| `ui-design` | `skills/review-lenses/ui-design.md` | `reviewer` | the app's visual language: `app/renderer/src/tokens.css` tokens instead of literals, spacing, type, colour, dark mode, popover and sheet sizing |
| `ux` | `skills/review-lenses/ux.md` | `ux` | interaction and recovery: keyboard paths, destructive acts without undo or confirmation, empty and error states, the human's round-trips |
| `architecture` | `skills/review-lenses/architecture.md` | `architect` | layering, one truth per fact, schema and migration discipline, the ADRs the change touches, the reader-first order |

This table is the single source of truth: a lens is a row plus a file and an owner, and the three cannot drift. `code` has no file of its own because `agents/reviewer.md` already carries that reading, and a copy would be a second contract to keep in step.

## What a lens may do

Every lens file has the same three headings:

- `## Read`: what in the diff the lens looks at.
- `## Blocks on`: the findings that must change first, a `REQUEST_CHANGES` review, per `agents/reviewer.md` Commenting.
- `## Never blocks on`: the taste calls, a body comment and merge, per "Nits do not veto".

Every `## Blocks on` entry cites a repo path or an ADR, never generic advice, and cites `file:line` in the finding itself. A lens with no basis for a judgement says nothing rather than guessing: `ui-design` and `ux` read the SwiftUI source and any screenshot the PR attaches, and judge no screen they cannot see.

A lens whose file cannot be read is named in the review body, and the gate falls back to `code`: a missing lens is never a silent pass.

## Adding a lens

One file and one row with its owner, in the same PR, under the three headings above. A file with no row, a row with no file, and an owner with no prompt are all drift, and the pins in `rust/core/src/tests/scaffold_convention.rs` fail on any of them.
