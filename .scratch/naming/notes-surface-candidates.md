# Scope: the notes-surface name — candidates

**Status:** parked. "Bikeshed" stands; there is no decision to move yet. This note
holds the candidate list so the review is not rerun from memory.

## The question

Tandem's bike register names the app (**Tandem**) and, since ADR 0015, the notes
surface (**Bikeshed**). The open todo *"review if bikeshed should be renamed to
another bike term"* asks whether the surface should carry a *different* bike term.

## The constraint

ADR 0015 coupled the surface word to the on-disk folder: `~/.tandem/bikeshed/`.
Any rename is therefore a **data migration** (or re-splitting the word from the
folder, which ADR 0015 exists to prevent) plus the sweep ADR 0015 performed —
code identifiers, `GLOSSARY.md`, the specs, the memory skill.

## What just entered the register

`GLOSSARY.md` adopted **Hub** (the `Index.md` entry point) and **Spokes** (the
wikilinks a note sends out). Those leave a gap exactly where the surface sits: a
wheel *is* hub + spokes + rim.

## Candidates

| name | case | cost |
|---|---|---|
| **Bikeshed** (keep) | The shed that keeps the bike. Zero cost. Liability: "bikeshed**ding**" (Parkinson's law of triviality) reads as ironic for a *decision* record. | none |
| **Wheel** | Completes hub + spokes; the wheel is the linked whole, not a storage box. The one candidate that adds coherence rather than replacing a word. | rename + migration |
| **Workshop** | Where the bike is built and repaired; the surface where understanding is assembled. No pun, but not a *part*. | rename + migration |
| **Toolbox / Toolkit** | The tools the next session reaches for. Adequate, generic. | rename + migration |
| **Chain** | The linked graph — but redundant with Spokes, and better for links than for the container. | rename + migration |
| **Velodrome / Route / Groupset / Bottom bracket** | Grand, obscure, or colliding with ordinary words. Skip. | — |

## Leaning

If the bikeshedding pun is the only thing pushing off "bikeshed", **Wheel** is the
candidate to reach for. Otherwise keep "bikeshed".

## Related

- `GLOSSARY.md` — Bikeshed, Hub, Spokes
- `docs/adr/0015-bikeshed-not-vault.md`
- todo `A85C9AA8` — "review if bikeshed should be renamed to another bike term"
