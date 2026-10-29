# Architecture lens

The checklist for a diff that adds a fact, moves a boundary, or touches the store's shape.

## Read

- Layering: `Sources/TandemCore/` holds the domain and knows nothing of SwiftUI; `Sources/tandem-cli/` is a face over it; `Sources/Tandem/` is the app. A rule that lives in a view and nowhere else, or in the CLI and nobody else, is the shape to look at (ADR 0009).
- One truth per fact: whether the diff introduces a second place the same fact is written, cached or defaulted. `Sources/TandemCore/Store.swift` is the store's one writer.
- Schema and migration discipline: a change to `StoreSchema.swift` lands with its numbered step in `StoreMigration.swift`, in its own transaction (ADR 0031), and a change to the shape that skips one is not a small change.
- The ADRs the change touches: `docs/adr/`, cited by number in the diff's prose where it overrules or amends one.
- The reader-first order: whether the change keeps what a reader needs before what a writer does, as `docs/adr/` and `AGENTS.md` state it.
- A second copy of a contract: a rule restated in two files instead of one pointing at the other.

## Blocks on

- A stored fact with two writers, or a second copy of it in a view, a cache or a default that the store does not own (ADR 0031; `Sources/TandemCore/Store.swift`).
- A change to `Sources/TandemCore/StoreSchema.swift` with no matching step in `Sources/TandemCore/StoreMigration.swift`, or a migration outside its own transaction.
- A domain rule in `Sources/Tandem/` or `Sources/tandem-cli/` where `Sources/TandemCore/` already holds it: the same action reachable two ways with two behaviours (ADR 0009).
- A prose file that restates a contract instead of pointing at it, where the repo keeps one home for that contract (ADR 0026).
- A change that overrules an ADR without naming and amending the ADR in the same PR.

## Never blocks on

- A file split the reviewer would have drawn one folder over.
- A private type promoted to internal for a test.
- A new file beside an old one, when the old one is not a second truth.
