# 03: Add the release workflow — the repo's first CI (U3)

**What to build:** The release pipeline in the main repo. A tag `v*` push (or manual dispatch) on a macOS 27 runner runs: test-suite gate (`swift test`) → release build of both products → bundle assembly with the in-bundle MCP binary → artifact verification (bundle structure, `codesign --verify`, Info.plist version matches the tag, in-bundle MCP stdin-EOF smoke, zip intact) → publish the GitHub release for the tag with `Tandem-<version>.zip` plus a SHA256 checksum file attached. Pull-request runs execute the same build-and-verify stages and upload the artifact, but never publish.

**Blocked by:** 01 (floor), 02 (in-bundle MCP binary)

**Status:** ready-for-agent

Context: [spec](../spec.md) — "Release pipeline" and the U3 unit brief under "Execution model". Prior art for the moved assertions: the retired formula's `brew test` block in the tap repo `gianniacustio/homebrew-tap`, `Formula/tandem.rb` (cloned at `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`). The GitHub-**hosted** runner label for macOS 27 is `xcode-27` (the Xcode 27 image runs on a macOS 27 base OS, public preview). `macos-27` is NOT a hosted label — Homebrew's `:golden_gate` → `macos-27` mapping is Homebrew-internal naming; a run pinned to `macos-27` queues forever (observed 2026-10-13: 17h queued, zero pickup).

- [ ] `swift test` runs as the first gate; a failing suite fails the run
- [ ] PR runs: green run produces the verified `Tandem-<version>.zip` as an artifact, no release published
- [ ] Tag `v*` runs: the GitHub release for the tag is published with the zip + SHA256 checksum attached
- [ ] Verification fails the run on: build failure, `codesign --verify` failure, Info.plist/tag version mismatch, MCP smoke failure, zip that does not contain an intact app
- [ ] Evidence: a green pull-request run on GitHub Actions producing the verified artifact (run link in the PR body)
