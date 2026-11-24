# The core is AGPL

> **Status:** In force. See [the index](README.md).

Under MIT anyone can host a competing Tandem with closed changes. This record moves the core to AGPL-3.0-only from the first release after v2.9.1. Decided 2026-11-23 (TAN-507).

## Decision

- **The core is AGPL-3.0-only.** `LICENSE` is the AGPL-3.0 text verbatim, `license = "AGPL-3.0-only"` in `rust/Cargo.toml` is inherited by both crates, and `rust/deny.toml` names the two workspace crates in `exceptions` so the third-party allow list stays permissive.
- **`examples/` stays MIT**, with its own `examples/LICENSE`, and both plugin manifests stay MIT: people copy those prompts and skills, and the vendored mattpocock skills are MIT.
- **Past releases stay MIT.** `NOTICE.txt` keeps the MIT notice for the code they contain, outside contributions included. The outside author was not asked: MIT lets the project relicense under AGPL while the notice is kept.
- **A CLA** (TAN-859) keeps a later relicense or dual licence possible for AGPL-era contributions.
- **Every release archive carries `LICENSE` and `NOTICE.txt`**, and the workflow fails when its listing lacks either. The board footer links to the source (TAN-860), for AGPL section 13.

## Alternatives rejected

- **Stay MIT:** leaves closed hosted forks open.
- **FSL or BSL:** rejected by the human on HOS-19; not open source.
- **AGPL-3.0-or-later:** hands the choice of a future licence to the FSF; `-only` with the CLA keeps it with the project.
- **A DCO instead of a CLA:** grants nothing beyond AGPL, so a later relicense would need every contributor.
- **A trademark:** dropped on 2026-11-23. The cost: AGPL protects the code, not the name.
- **Everything AGPL, `examples/` included:** discourages the copying the examples exist for.
