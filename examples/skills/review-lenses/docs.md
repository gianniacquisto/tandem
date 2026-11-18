# Docs lens

The checklist for a diff that changes a documented surface: the docs it touches or falsifies must be true again in the same PR. It judges prose against the diff, not the prose's style.

## Read

- The docs the change touches or falsifies. Docs are `README.md`, `docs/*.md`, `GLOSSARY.md`, `DESIGN.md`, the `examples/` prompts and skills, and `docs/adr/` (an ADR is amended, never silently contradicted). Code comments and rustdoc stay with the `code` lens.
- The brief's `## What` docs list, against the diff: each doc it names is touched.
- The PR body's `## Docs` section, against both the brief's list and the diff: the files it names are in the diff, and a `none, because <reason>` holds up against what the diff changed.

## Blocks on

- A doc the diff makes false (a flag, tool, path, status or step the doc still describes the old way), citing the doc `file:line` and the diff line that contradicts it.
- A doc the brief named that the diff does not touch.
- A PR body with no `## Docs` section, on a change whose brief names this lens.

## Never blocks on

- Wording or style.
- Docs the change does not touch.
- A new doc the brief did not ask for (a body comment, then merge).
