# 03: Add the release workflow — the repo's first CI (U3)

**What to build:** The release pipeline in the main repo. A tag `v*` push (or manual dispatch) on a macOS 27 runner runs: test-suite gate (`swift test`) → release build of both products → bundle assembly with the in-bundle MCP binary → artifact verification (bundle structure, `codesign --verify`, Info.plist version matches the tag, in-bundle MCP stdin-EOF smoke, zip intact) → publish the GitHub release for the tag with `Tandem-<version>.zip` plus a SHA256 checksum file attached. Pull-request runs execute the same build-and-verify stages and upload the artifact, but never publish.

**Blocked by:** 01 (floor), 02 (in-bundle MCP binary)

**Status:** resolved

Context: [spec](../spec.md) — "Release pipeline" and the U3 unit brief under "Execution model". Prior art for the moved assertions: the retired formula's `brew test` block in the tap repo `gianniacustio/homebrew-tap`, `Formula/tandem.rb` (cloned at `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`). The GitHub-**hosted** runner label for macOS 27 is `xcode-27` (the Xcode 27 image runs on a macOS 27 base OS, public preview). `macos-27` is NOT a hosted label — Homebrew's `:golden_gate` → `macos-27` mapping is Homebrew-internal naming; a run pinned to `macos-27` queues forever (observed 2026-10-13: 17h queued, zero pickup).

- [x] `swift test` runs as the first gate; a failing suite fails the run
- [x] PR runs: green run produces the verified `Tandem-<version>.zip` as an artifact, no release published
- [ ] Tag `v*` runs: the GitHub release for the tag is published with the zip + SHA256 checksum attached (pending: the v0.5.0 tag run)
- [x] Verification fails the run on: build failure, `codesign --verify` failure, Info.plist/tag version mismatch, MCP smoke failure, zip that does not contain an intact app
- [x] Evidence: a green pull-request run on GitHub Actions producing the verified artifact (run link in the PR body)

## Resolution

- **PR:** https://github.com/gianniacquisto/tandem/pull/3 (merged, commit `1b671b0`)
- **Green evidence:** PR run https://github.com/gianniacquisto/tandem/actions/runs/38442063055 — 1m24s on self-hosted `tandem-mac-27`: 32-test gate, release builds of both products, bundle assembly, all five verifications, artifact `Tandem-0.0.0` (zip + SHA256SUMS) uploaded; publish step skipped as designed; `gh release list` unchanged (empty) before and after.
- **Runner label resolution:** `macos-27` is not a GitHub-hosted label (17h queued, zero pickup — run 38321664232); the hosted `xcode-27` preview pool is unstable (3/3 runs stalled the test process 10–17s+, failing the concurrency gate — runs 38437473891, 38438891858). The workflow therefore targets `macos-27` as a repo-scoped label served by the self-hosted runner `tandem-mac-27` (the maintainer's macOS 27.0.1 / Xcode 27 machine — the target platform itself). Runner lives at `~/actions-runner-tandem` (v2.338.0, nohup-started; does NOT survive a reboot — restart: `cd ~/actions-runner-tandem && nohup ./run.sh >> runner.log 2>&1 & disown`).
- **Test change:** the locked-store test's wait windows were widened 5s → 15s (matching its sibling tests) for headroom against runner stalls; no assertion or product change.
- **Commits:** `90f4fc2` (workflow), `443310c` (xcode-27 attempt), `99465e8` (test windows), `71ee5f4` (self-hosted label).
