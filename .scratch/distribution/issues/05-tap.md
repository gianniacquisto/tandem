# 05: Tap cutover — cask rewrite, formula removal, workflow trims (U5)

**What to build:** The tap (`gianniacquisto/homebrew-tap`; brew names the tap `gianniacquisto/tap`) ends with exactly one brew record for Tandem: a cask that downloads the prebuilt `Tandem-<version>.zip` from the project's GitHub release with the artifact's real sha256, `depends_on macos: :golden_gate`, and a plain `app` stanza (no formula dependency, no postflight symlink). The formula, its `brew test` block, and the bottle-publish workflow are gone; the PR-test workflow keeps the syntax check and drops the bottle steps; the daily autobump's `brew bump` now opens the bump PR for the single cask file (its livecheck targets the release-asset URL); the tap README is cask-only.

**Blocked by:** 03, plus the supervised gate: the v0.5.0 release exists (the cask's `url` must point at the live artifact; see 06)

**Status:** resolved

Context: [spec](../spec.md) — "Distribution shape" and the U5 unit brief under "Execution model". Tap repo: https://github.com/gianniacquisto/homebrew-tap — per its own README, develop where brew keeps the clone: `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap` (an ordinary git clone; branch, commit, push from there).

- [ ] `Casks/tandem.rb`: `url` → the release-asset zip for `v#{version}`, the artifact's real sha256, `depends_on macos: :golden_gate`, plain `app` stanza, no `depends_on formula`, no `postflight_steps`
- [ ] `Formula/tandem.rb` deleted; `publish.yml` (bottle workflow) deleted; `tests.yml` keeps the tap-syntax step, drops the formulae/bottle steps; `autobump.yml` drops the formula sync flag
- [ ] Tap README: cask-only install story, no formula section
- [ ] `brew style Casks/tandem.rb` and `brew audit --cask --strict gianniacquisto/tap/tandem` pass (output in the PR body)
- [ ] The tap's CI run on the PR is green

## Resolution

Resolved by tap PR #2 (https://github.com/gianniacquisto/homebrew-tap/pull/2, branch commit e596d33, squashed to tap main 84b2563). Evidence: `brew style Casks/tandem.rb` clean; `brew audit --cask --strict gianniacquisto/tap/tandem` clean (livecheck present, URL live); cask sha256 `00894c7…` matches the v0.5.0 release's `SHA256SUMS` and the downloaded `Tandem-0.5.0.zip`; zip layout verified — `Tandem.app` at the zip root (plain `app` stanza), MCP binary at `Contents/MacOS/tandem-mcp`, Info.plist 0.5.0; tap CI green (run 38452904232, tap-syntax on macos-26 + ubuntu). Merged on the maintainer's explicit go-ahead (one-way door).
