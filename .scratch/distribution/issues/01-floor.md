# 01: Raise the package platform floor to macOS 27 (U1)

**What to build:** The package declares the macOS 27 floor and everything builds and passes the full core test suite cleanly on a macOS 27 machine. (macOS 26 / Intel users are refused at the distribution layer by the cask's `depends_on`, ticket 05; this ticket moves only the package floor.)

**Blocked by:** None (can start immediately)

**Status:** resolved

Context: [spec](../spec.md) — "Platform floor", "Code surface", and the U1 unit brief under "Execution model". ADR 0003.

- [ ] `Package.swift` declares `platforms: [.macOS(.v27)]`
- [ ] `swift build -c release` completes clean on this macOS 27 machine
- [ ] `swift test` green (the existing 32-test core suite)
- [ ] No other source changes (the app surface is frozen by the spec)
- [ ] Evidence captured: the before (v26 declaration + build) and after (v27 + clean release build + green test suite) output

## Resolution

Resolved by PR #2 (https://github.com/gianniacquisto/tandem/pull/2, commit 6d2019f). Evidence: `swift build -c release` clean from committed state; `swift test` 32 tests / 3 suites green. Note: `swift-tools-version` moved 6.2 → 6.4 — mechanically forced, `.macOS(.v27)` is unavailable under PackageDescription 6.2 and there is no string-based fallback; the floor already implies SDK 27 / Swift 6.4 for any source build.

