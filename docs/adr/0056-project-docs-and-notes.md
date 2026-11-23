# Durable knowledge goes in the project's own docs; notes hold what has no other home

> **Status:** In force. Amends [ADR 0033](0033-wiki-is-documentation.md). See [the index](README.md).

Agents write to two places: a project's own documentation and the Tandem notes. [ADR 0033](0033-wiki-is-documentation.md) says what the notes are for, but not where they end and a project's docs begin, so a fact could land in both, or in the wrong one. This is a product rule for any user's project, software or not, and Tandem's own repo is one case of it.

## Decision

- **The test an agent applies before writing durable knowledge:** does this project have its own docs that this belongs in? If yes, the fact goes into those docs through the project's own process (for a repo: a pull request inside a ticket's build). If no, for example non-software work or a project with no docs of its own, it goes in the Tandem notes. If it is already in the code, an ADR, the project's docs or a ticket, it goes nowhere new.
- **What notes always hold:** what has no other home. That means the user's working knowledge, and decisions and gotchas recorded nowhere else. [ADR 0033](0033-wiki-is-documentation.md)'s page roles still describe how notes are shaped. The store is named by its role; its folder is `<home>/bikeshed/` today, and the rename to notes ([ADR 0055](0055-the-notes-store-is-called-notes.md)) will change that path without changing this rule.
- **What a project's docs are:** whatever documentation the project keeps outside Tandem, public or shared. Tandem's own repo is one case: its docs are `README.md`, `docs/` (ADRs included), `GLOSSARY.md`, `DESIGN.md` and `examples/`, because the prompts and skills there ship to users through the plugin ([ADR 0051](0051-a-claude-code-plugin-is-the-connector.md)).
- **How docs are written:** they are proper documentation, detailed and neither hand-wavy nor padded. They explain behaviour, concepts, contracts and where to look (files, modules), in enough detail that a reader who does not read the code understands the system. They do not paste or transcribe code that would drift: signatures, line-by-line logic, field lists copied out of a schema.
- **ADR 0033's retired Consequences:** [ADR 0045](0045-the-board-and-notes-cut.md) retired the parts of ADR 0033's Consequences that name `CONVENTIONS.md`, session records, the consolidation pass and the Swift scaffold test. ADR 0033's body is not rewritten.

## Why

- **One home per fact.** A fact written in two places drifts in one of them. The test names a single home before the write.
- **It fits any project.** Tandem is used for work that is not software. "Does this project have its own docs?" has an answer in every case; "would a stranger installing Tandem need this?" only fits Tandem's own repo.
- **Docs stay readable without the code.** Telling writers never to restate the code leaves readers reading Rust to learn what a feature does. Explaining in detail while not transcribing keeps docs useful and keeps them from drifting.

## Decided against

- **The test "would a stranger installing Tandem need this?"**: it only fits Tandem's own repo.
- **"Never restate the code"**: too strong, because readers should not need to read the code. Replaced by "do not transcribe code; explain it in detail".
- **Notes holding drafts of product docs**: a draft drifts from the real page.
- **A trim or sweep of existing docs, and a cleanup ticket**: detailed docs are valued, and the rule applies to writes from here on.
- **A new `Blocks on` line in the docs lens**: the architecture lens already blocks on prose that restates a contract, so it would be a second copy.
- **Rewriting ADR 0033's body**: ADRs are amended, never rewritten. Only its Status line changes.

## Consequences

- `examples/AGENTS.md` (Record) and `examples/agents/builder.md` (Update the docs) carry the test in one sentence and point here. They do not copy this Decision.
- No code, schema or contract changes, and no existing doc is trimmed.
