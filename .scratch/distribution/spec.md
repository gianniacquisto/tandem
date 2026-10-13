# Spec: Distribution cutover — prebuilt cask, in-bundle MCP server, macOS 27 floor

Type: spec
Status: ready-for-agent
Source: grilling session on ADR 0002 (Oct 2026) → [ADR 0003](../../docs/adr/0003-distribution.md)

## Problem Statement

Tandem is distributed as a Homebrew formula *and* a cask under the same name. The formula compiles the app from source on every user's machine (Xcode Command Line Tools required, multi-minute build) and installs the app bundle into the Cellar; the cask only symlinks it into `/Applications`. The result is two brew records for one product: after every release `brew upgrade` moves the formula forward and the cask's recorded version lags — brew then flags Tandem as outdated although the running app is current — and the cask is an artifact that mirrors the formula's url/sha256 without ever downloading from it. For a public, user-friendliness-first app this is the wrong shape: the install story is a source build, the upgrade story is two identities, and the release story is two files to keep in sync.

## Solution

One brew record: a cask that downloads a **prebuilt** `Tandem.app` (a zip) from a GitHub release built by CI on each tag. `brew install --cask gianniacquisto/tap/tandem` is the entire install — seconds, no compiler, Launchpad/Spotlight presence via `/Applications`. The MCP server ships **inside the app bundle**; an agent's client config points at that in-bundle path (one copied line, stable across upgrades). The formula is removed. Distribution is brew-only. The floor is macOS 27 (Apple silicon); the first release under this model is 0.5.0. The app itself — Main window, Menu bar item, Store, MCP surface — is unchanged.

## User Stories

1. As a Human installing Tandem for the first time, I want one brew command that finishes in seconds without a compiler, so that installing costs no setup knowledge.
2. As a Human, I want Tandem in Launchpad and Spotlight after `brew install --cask`, so that I launch it like any other app — no terminal.
3. As a Human, I want `brew upgrade --cask tandem` to be the only upgrade action, so that "is my Tandem current?" has one answer from one brew record.
4. As a Human, I want `brew uninstall --cask tandem` to remove the app cleanly, so that leaving is as easy as arriving.
5. As a Human on macOS 26 or an Intel Mac, I want brew to refuse the install with a clear "requires macOS 27" message, so that I get an explanation instead of a broken app.
6. As a Human upgrading from ≤ 0.4.x, I want a short documented migration (uninstall the old cask, uninstall the formula, install the cask), so that the cutover is a one-time, known sequence.
7. As a Human, I want my todos in the Store file to survive the cutover untouched, so that a distribution change touches nothing of mine.
8. As an Agent, I want the MCP server reachable at a stable, documented path inside the app bundle, so that my client config survives every upgrade without re-pointing.
9. As an Agent, I want the `list` / `add` / `complete` tools and their contracts unchanged, so that existing integrations keep working after the cutover.
10. As an Agent, I want the README's setup line to be copy-pasteable for both Claude Code and OpenCode, so that integration is one line.
11. As a Human, I want the in-bundle MCP server to be the same tested binary the app ships, so that "the app works" implies "my agent's server works".
12. As a Maintainer, I want a tag to produce a release artifact automatically, so that cutting a release is one command.
13. As a Maintainer, I want the release artifact verified (test suite passes, signature verifies, MCP binary smoke-tests, bundle intact) before it is published, so that a bad build never reaches users.
14. As a Maintainer, I want the release reproducible from the tag, so that a botched artifact can be re-cut by re-running the same pipeline.
15. As a Maintainer, I want the tap's autobump to open a PR bumping the single cask file with the artifact's real sha256, so that merging that PR is the whole release.
16. As a Maintainer, I want the cask's sha256 pinned by brew, so that a swapped or re-hosted artifact fails install rather than silently shipping.
17. As a Maintainer, I want the old formula, its `brew test`, and the tap's bottle workflow retired in the same change, so that no dead distribution path lingers.
18. As a Maintainer, I want the release workflow to run the package test suite, so that the repo's first CI gate protects the core before any artifact ships.
19. As a Human, I want the README to state the macOS 27 / Apple silicon floor, so that the unsupported corner hears it before installing.
20. As a Human, I want the app to behave exactly as before — Main window, Menu bar item, live list, Store file — so that a distribution change changes nothing I use.
21. As a Maintainer, I want each piece of this cutover to be a small, reviewable PR with evidence, so that the cutover can be walked back piece by piece.
22. As a Maintainer, I want the public cutover itself (tag, tap merge, migration on the live machine) to be a supervised checklist, so that the public act happens deliberately.

## Implementation Decisions

### Distribution shape (ADR 0003)

- One brew record: the cask `tandem` in the tap. It downloads a zip containing `Tandem.app` from the project's GitHub release for the tag (asset name `Tandem-<version>.zip`), declares `depends_on macos: :golden_gate` (the brew symbol for macOS 27), and installs the app with a plain `app` stanza — no formula dependency, no postflight symlink (the old `/Applications` symlink mechanism dies with the old cask).
- The formula is removed from the tap. The tap's bottle-publish workflow is retired (no formula, no bottles). The tap's PR-test workflow keeps the syntax check and drops the bottle steps.
- The existing daily autobump stays: its `brew bump` livecheck now targets the release-asset URL and opens the bump PR for the single cask file with the artifact's real sha256. The first release verifies the mechanism; a manual `brew bump` is the fallback.
- Distribution is brew-only: no DMG, no documented direct download from the releases page (the release exists to serve the cask).
- Signing stays ad-hoc: the bundle script's existing whole-bundle `codesign --sign -` now covers the MCP binary too. Developer ID + notarization are deferred — the revisit trigger is recorded in ADR 0003, and user-facing docs do not mention Gatekeeper.

### Release pipeline (the main repo gains its first workflow)

- A release workflow triggers on tag push (`v*`) and manual dispatch, running on a macOS 27 runner. Its build-and-verify stages run on pull requests too (uploading the artifact, never publishing); the publish stage runs only on tag push. Stages:
  1. run the package test suite (`swift test`) as a gate,
  2. build both products in release configuration,
  3. assemble the app bundle with the existing bundle script — which gains the MCP binary as a second input, placed inside the bundle at `Contents/MacOS/tandem-mcp` (the stable path agent configs reference),
  4. verify the artifact: bundle structure, `codesign --verify`, Info.plist version matches the tag, and the in-bundle MCP binary smoke test (stdin EOF → clean exit — the same smoke the old formula's test used),
  5. publish the GitHub release for the tag with the zip (and a SHA256 checksum file) attached.
- The old formula's `brew test` block is retired; its assertions live in the release workflow instead.

### Platform floor

- The Swift package's macOS platform floor moves 26 → 27. arm64-only: no universal binary, no x86_64 build (macOS 27 is Apple-silicon-only). The cask's `depends_on` and the README state the floor.

### Code surface

- No changes to the app: the GUI, the MCP server code, the core module, the Store, and the tool contracts are frozen. The source-tree changes are exactly: the package platform floor, the bundle script (MCP binary in-bundle), the new release workflow, and the docs (README + releasing).

### Migration and version

- First release under this model: **0.5.0**.
- Existing installs (formula + linking cask) migrate once, cask first (its uninstall removes the `/Applications` symlink): uninstall the cask, uninstall the formula, then `brew install --cask gianniacquisto/tap/tandem`. Documented as a short "upgrading from ≤ 0.4.x" note in the README and the release notes.

### Docs

- README: the install section is the single cask command (drop the "builds from source / CLT" notes); the agent section gives the in-bundle path verbatim for Claude Code and OpenCode; the macOS 27 / Apple silicon floor is stated; the migration note lives here.
- releasing.md is rewritten to the new chain: tag → release workflow publishes the artifact → autobump PR (single cask file) → merge. Manual verification commands become `brew style` and `brew audit --cask --strict` on the tap.
- The tap README is updated to match (cask-only, no formula section).

## Testing Decisions

- **Good test = external behaviour only** (v1 convention). This cutover changes no Swift logic, so **no new unit tests** are written; the existing 32-test core suite (3 suites) is the regression gate.
- **The release workflow is the build seam.** It fails the run if: the test suite fails; the build fails; the assembled bundle fails `codesign --verify`; the Info.plist version does not match the tag; the in-bundle MCP binary does not exit cleanly on stdin EOF; or the zip does not contain an intact app. Prior art: the old formula's `brew test` block (retired, assertions moved) and the v1 spec's MCP stdio smoke pattern.
- **The cask is verified at the tap seam**: `brew style` + `brew audit --cask --strict` locally (evidence in the tap PR), plus the tap's CI syntax check.
- **The install is verified at the cutover (manual, one-time)**, on the live machine: run the migration; confirm the app in `/Applications` is a real bundle (not a symlink), launches, appears in Launchpad, `open -a Tandem` works, the in-bundle MCP binary responds to the stdin-EOF smoke, and `brew list` shows a single current cask record with no formula record. A pristine-machine install (no prior brew records) is a known gap on this Mac; if a second Mac or a VM is available it is checked there, otherwise the gap is noted in the cutover record.

## Execution model — subagents implement

- **Subagents implement; the main agent integrates.** Each work unit below is dispatched to a fresh subagent that reads this spec plus the unit brief, works on its own branch, verifies per the Testing Decisions, and opens a PR.
- **One PR per unit. Every PR body is written with the `pr` skill** — Summary / Evidence / Merge Danger, with the Evidence section showing the unit's before/after output. The main agent reviews each PR and merges on approval; a failed review re-dispatches a new subagent with the feedback.
- **Units and order** (U1 ∥ U2 → U3 → tag + release → U4 ∥ U5 → U6):
  - **U1 — floor**: package platform floor 26 → 27. Evidence: a clean release build on macOS 27.
  - **U2 — bundle**: the bundle script takes the MCP binary and ships it inside the bundle at the in-bundle path. Evidence: the resulting bundle tree + `codesign --verify`.
  - **U3 — release workflow**: the release pipeline above (needs U1, U2). Evidence: a green pull-request run producing the verified artifact (no publish on PRs).
  - **U4 — docs**: README + releasing to the final state. Evidence: the doc diff.
  - **U5 — tap**: cask rewrite, formula removal, tap README, workflow trims. Evidence: `brew style` + `brew audit --cask --strict` output. Opens only after the v0.5.0 release exists (its url points at the live artifact).
  - **U6 — cutover (supervised, not a subagent PR)**: tag v0.5.0 → release published → merge U4 + U5 → run the migration on the live machine → the install verification above. Each public step (tag push, tap merge) happens on the human's explicit go-ahead.
- **Rollback**: U1–U4 are two-way doors on main (revert the PR). U5 is effectively a one-way door (users may have installed 0.5.0 before a problem is noticed) — so U5 is reviewed hardest and only after U3 has produced a real, verified artifact.

## Out of Scope

- Developer ID signing and notarization — deferred by decision (ADR 0003, revisit trigger recorded there); user-facing docs stay silent on it.
- DMG or direct-download distribution — brew-only for now.
- A PATH-installed MCP binary or `tandem` CLI command; agent-only installs (a thin `tandem-cli` formula is the future shape if that corner ever matters).
- Universal binaries; Intel Macs; macOS 26 (the floor is 27).
- Any change to the app itself: GUI, Menu bar item, core module, Store schema, MCP tool surface — all frozen.
- Auto-update inside the app — brew is the upgrade path.

## Further Notes

- **Provenance**: the decisions above come from the grilling session on ADR 0002 (Oct 2026) and are recorded in [ADR 0003](../../docs/adr/0003-distribution.md), which supersedes ADR 0002.
- **Why the cask installs cleanly unnotarized**: Homebrew applies the quarantine attribute to cask downloads, but its Gatekeeper audit applies to official casks only — brew does not refuse a personal-tap cask; the first-launch approval is one-time per user and brew preserves it across upgrades. Internal note only — not for user-facing docs (ADR 0003).
- **The release zip is built by CI from the tagged source**; the old tag-tarball download is no longer used by anything — the cask never sees a source tarball.
- **The main repo has no CI today**; the release workflow is its first, which is why the test-suite gate matters: it is the first automated run of the 32 core tests.
- **Environment verified on this machine** (Oct 2026): macOS 27.0.1, Xcode 27.0 / SDK 27, Homebrew 7.0.x (knows the `:golden_gate` symbol; its runner matrix maps it to the macOS 27 GitHub Actions image), `gh` authenticated with repo scope.
