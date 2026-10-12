# 04: Docs to the final state — README + releasing (U4)

**What to build:** README and `docs/releasing.md` describe the post-cutover world. README: the install section is the single cask command (drop the build-from-source / CLT notes); the agent section gives the in-bundle MCP path verbatim for Claude Code and OpenCode; the macOS 27 / Apple silicon floor is stated; the "upgrading from ≤ 0.4.x" migration note lives here (uninstall the old cask, uninstall the formula, install the cask). releasing.md is rewritten to the new chain: tag → release workflow publishes the artifact → autobump PR (single cask file) → merge; manual verification commands become `brew style` and `brew audit --cask --strict` on the tap.

**Blocked by:** 03, plus the supervised gate: v0.5.0 tagged and the release published (see 06 — the docs reference the live flow)

**Status:** ready-for-agent

Context: [spec](../spec.md) — "Docs", "Migration and version", and the U4 unit brief under "Execution model".

- [ ] README install section: `brew install --cask gianniacquisto/tap/tandem` only; no compiler/CLT/build-from-source notes in the install story
- [ ] README agent section: the in-bundle path `/Applications/Tandem.app/Contents/MacOS/tandem-mcp` verbatim for both Claude Code and OpenCode
- [ ] README states the macOS 27 / Apple silicon floor
- [ ] README carries the ≤ 0.4.x migration note
- [ ] releasing.md: tag → workflow → autobump PR (single cask file) → merge; manual checks are `brew style` + `brew audit --cask --strict`
- [ ] Evidence: the doc diff in the PR body
