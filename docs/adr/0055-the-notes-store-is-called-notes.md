# The notes store is called notes

> **Status:** In force. Supersedes [ADR 0015](0015-bikeshed-not-vault.md) on the word. See [the index](README.md).

ADR 0015 named the notes store "the bikeshed". The MCP tools were already `note_*`, the shell tab already read "Notes" and `docs/concepts.md` headed its section "Notes", so the product spoke two words for one thing. We decided: **the store is called notes, and one file in it is a note.** "Bikeshed" moves to the GLOSSARY's Avoid list beside "vault" and "notebook".

## Why

- **One word beats two.** The tools, the tab and the docs already said "notes". Only the board page, the errors, the help and the prose said "bikeshed", and each reader had to learn that they were the same thing.
- **The word describes the thing.** "Bikeshed" was a joke name that told a new reader nothing about what the folder holds.

## What stays from ADR 0015

Its rule against a documented alias holds: there is one name, and no doc offers "bikeshed" as a second way to say it. The only place the old word survives in prose is the Avoid entry, the dated records and the literal names below.

## Scope

This record covers the **word** on surfaces with no compatibility promise: the board page and its CSS classes, the error messages, the CLI help, the MCP parameter descriptions, the docs and the prompts.

The public surface and the code names were decided with the on-disk move (TAN-862), below.

## The on-disk move (TAN-862)

The word alone left the old name in the paths, the export and the doctor, so the human chose to finish the rename everywhere, with a migration, and to keep the old names answering where a promise needs it (`docs/versioning.md`).

- **The folder.** `<home>/bikeshed/` is `<home>/notes/`, and `bikeshed.lock` is `notes.lock`. `Home::notes_directory()` is the one place the name is joined. The first door that opens the notes (serve start, an MCP note method, a CLI note verb or `export`) renames an older folder once, under both locks: one `rename(2)`, so it is atomic and a second run finds nothing to do. An empty `notes/` beside `bikeshed/` is removed first. If both hold notes, or `bikeshed/` is a link or a file, the move refuses with a `NotesFolderClash` error naming both paths and changes nothing. `doctor` only reads (ADR 0014) and `import` already refuses a non-empty home, so neither moves anything.
- **The export.** `/export` writes `notes/` entries. Import reads `notes/` or `bikeshed/` entries for good, because an archive must import into a later release (ADR 0050), and lands either under `notes/`; an archive with both is refused.
- **The doctor and `note stats`.** The check is `notes`. For one release the report also carries a `bikeshed` row with the same verdict, and `note stats` carries a `bikeshed` field with the same value as `notes`, each with one stderr warning on the CLI, then both go.
- **The code.** The module is `tandem_core::notes`, `Bikeshed*` types are `Notes*`, and the test suites follow.
- **No downgrade path.** An older binary run after the move finds no notes until the folder is renamed back. Release notes say so; a symlink left behind would be refused by the path guards and needs privileges on Windows.

ADRs 0011, 0013, 0022 and the other dated records keep their words; a pointer line tells the reader that the folder they name is now `notes/`.
