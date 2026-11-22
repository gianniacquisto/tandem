# Architecture lens

The checklist for a diff that adds a fact, moves a boundary, or touches the store's shape.

## Read

- Layering: the domain layer knows nothing of a UI, and each face (the CLI, the served UI) sits over it. A rule that lives in a view and nowhere else, or in the CLI and nobody else, is the shape to look at.
- One truth per fact: whether the diff introduces a second place the same fact is written, cached or defaulted. Where the repo names one writer for the store, it stays the only writer.
- Schema and migration discipline: a schema change lands with its numbered migration step and its own transaction, and a change to the shape that skips one is not a small change.
- The ADRs the change touches, cited by number in the diff's prose where it overrules or amends one.
- The reader-first order: whether the change keeps what a reader needs before what a writer does, as the repo's ADRs and `AGENTS.md` state it.
- A second copy of a contract: a rule restated in two files instead of one pointing at the other.

## Blocks on

- A stored fact with two writers, or a second copy of it in a view, a cache or a default that the store does not own.
- A schema change with no matching migration step, or a migration outside its own transaction.
- A domain rule in a face where the domain layer already holds it: the same action reachable two ways with two behaviours.
- A prose file that restates a contract instead of pointing at it, where the repo keeps one home for that contract.
- A change that overrules an ADR without naming and amending the ADR in the same PR.

## Never blocks on

- A file split the reviewer would have drawn one folder over.
- A private type promoted to internal for a test.
- A new file beside an old one, when the old one is not a second truth.
