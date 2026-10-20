# The notes folder is the bikeshed, not a "vault"

Tandem's notes surface carried two names: **Bikeshed** for the surface, **"vault"** for the folder holding the notes ([ADR 0004](0004-vault-protocol.md), "Vault protocol"; GLOSSARY called "vault" the accepted alternate). We decided: **one name.** The folder is the bikeshed too, and "vault" is retired from the app, the CLI, the GLOSSARY, and the live code. The word survives only in this repo's historical records — the dated ADRs and the retired planning notes written when it was the term.

## Why

- **"Vault" is Obsidian's word, not Tandem's.** The folder is a plain directory of markdown notes, and Tandem already had a bike-register name for it. Borrowing the competitor's noun made Tandem read as a clone where its own vocabulary was available.
- **Two names for one thing is a permanent tax.** "Bikeshed" and "vault" named the same folder; every comment, error, and doc had to choose between them, and every reader had to learn the alias. One name removes the choice.
- **The code should speak the app's language.** The core type was already `Bikeshed`; around it the folder was called a "vault", so one concept wore three vocabularies — `Bikeshed`, the `Vault*` types, and the prose "vault". Names now come from app terms (`Bikeshed`, `Note`) or are generic (`directory`, `fileURL`).

## What changed

- **User-facing text**: "the vault" → "the bikeshed" in the app's empty state, the rename sheet, the search empty state, and the core's `unknownNote` message; the README and the `tandem-memory` skill follow.
- **Code identifiers** — app terms: `VaultPath` → `BikeshedPath`, `VaultNode` → `BikeshedNode`, `VaultWatcher` → `BikeshedWatcher`, `BikeshedModel.vault` → `.bikeshed`. Generic where a generic word was truer: `vaultURL(for:)` → `fileURL(for:)`, `existingVaultFile` → `existingFile`. The editor bridge's page global `setVaultIndex` → `setNoteIndex` (its effect/facet/state → `setNoteIndex*` / `noteIndex*`, its payload parameter → `notes`), and the committed `editor.js` bundle was regenerated. Landed *after* the backlog batch, so the same sweep covers the names that batch introduced (see Consequences): the settings key `vaultPath` → `bikeshedPath`, `TandemSettings.chosenVault()`/`setChosenVault(_:)` → `chosenBikeshed()`/`setChosenBikeshed(_:)`, `BikeshedModel.switchVault(to:)` → `switchBikeshed(to:)`, and the folder picker's `chooseBikeshedVault()` → `chooseBikeshed()` with its menu item "Choose Bikeshed Vault…" → "Choose Bikeshed…".
- **GLOSSARY**: the Bikeshed entry drops the "alternate name" sentence and lists "vault" under _Avoid_.
- **The live specs** (the bikeshed and memory specs — the documents the code cites by section name) were swept.

## Rejected

- **Keep "vault" as a documented alias.** The alias is the cost; leaving it in writing is what let it spread into identifiers in the first place.
- **Invent a third bike word for the folder (e.g. "shed").** The problem is two words for one folder; a second bike word keeps the same split.
- **Rewrite the dated ADRs and tickets.** They are the record of what was decided, when, in the words used then — ADR 0004 stays "Vault protocol". This ADR is the pointer that the word has since changed, which is what keeps the historical record honest instead of edited.
- **Rename the editor's image discriminator `"vault"` to the generic `"local"`.** Kept `"bikeshed"`: it is an app term, consistent with the rest.

## Consequences

- **A grep for "vault" is now a history filter.** It hits only the dated `docs/adr/` records, the retired planning notes in this repo's git history, and the untracked `architecture-review.html` — never live code or live docs. That "only history matches" property is the acceptance test for the change, the same shape the MCP retirement left behind.
- **Numbered 0015, above 0010–0014**: those numbers are claimed by the backlog batch (#25–#30, merged as #33), and ADR numbers here are assigned on a branch, not globally.
- **Landed second, so it was re-applied on top of the batch.** #33 merged *before* this rename and brought live names in the old word — the ones listed under "What changed". This branch rebases onto that merge and renames them too. `vaultPath` was never in a release (v0.9.0 predates #27), so the settings key renames outright: a dev build's chosen folder falls back to the default once, and no migration is owed. **#32's copy still says "vault"** and adopts the wording when it rebases onto this.

> **Planning notes.** The tracker documents cited above were retired on 2026-10-21 — the bikeshed is canonical for that prose. They remain in this repo's git history; their ground is in the bikeshed's project notes.
