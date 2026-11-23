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

The following are public surface or code names and are decided by TAN-862, not here: the home folder `bikeshed/` and `bikeshed.lock`, the `/export` archive entry, the doctor check named `bikeshed`, the `note stats` output field and the Rust module, type and variant names. Until that ticket lands, the docs say "the notes, in `~/.tandem/bikeshed/`".
