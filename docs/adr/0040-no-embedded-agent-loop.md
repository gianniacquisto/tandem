# No embedded agent loop; the app is a board and notes surface

The app shipped a Workflow face plus an embedded opencode harness: a pinned binary, a `~/.tandem/harness` home, a Settings sign-in, and an in-app agent loop. SHE-1 asked whether the app should keep that runtime or become a control surface; SHE-3 builds the cut.

Decided 2026-11-05 (SHE-1 decided, SHE-3 built).

## Decision

- **No Workflow face and no embedded harness in the app.** The renderer ships four tabs: Tickets, Bikeshed, Usage, Settings. A stale `#/workflow` link lands on Tickets.
- **No bundled binary, no harness home creation, no sign-in, no spawn.** The main process spawns the core (`tandem rpc`) only. Credential handling leaves the tree with the spawn code.
- **Orchestration stays outside the app.** `tandem-cli` verbs and the MCP server are the agent face, unchanged. `session.transcript` and `usage.report` are served over `rpc` from the external harness, unchanged.
- **The on-disk harness home stays on disk.** An orphaned `~/.tandem/harness` directory is left in place; the release notes name its by-hand removal. No code deletes it and no migration moves it.
- **The provenance type stays.** `HarnessTransport` (`event-stream` versus `database`) is unchanged, so old records keep valid provenance and the core still reads external harnesses. No enum change, no migration.
- **The doctor row stays, its copy fixed.** `harness-auth` remains informational over the on-disk home, but it no longer points at Settings sign-in.

## Why

The embedded path cost a private-home seam, pinned-version upkeep and a second opencode beside the human's own, for an app that needs to stabilize as a board and notes surface first. Cutting it shrinks the install, removes the credential surface from the window, and keeps re-embedding possible later.

## Consequences

- [ADR 0038](0038-the-harness-home.md) is superseded by this record.
- `Scripts/check-harness-home.py` and `app/scripts/prepare-harness.mjs` are retired with the code they proved and staged.
- Packaging carries the core and the agent face only; the `harness` CI job and the release harness assertions go with them.
