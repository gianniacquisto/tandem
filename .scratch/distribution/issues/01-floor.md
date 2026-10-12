# 01: Raise the package platform floor to macOS 27 (U1)

**What to build:** The package declares the macOS 27 floor and everything builds and passes the full core test suite cleanly on a macOS 27 machine. (macOS 26 / Intel users are refused at the distribution layer by the cask's `depends_on`, ticket 05; this ticket moves only the package floor.)

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

Context: [spec](../spec.md) — "Platform floor", "Code surface", and the U1 unit brief under "Execution model". ADR 0003.

- [ ] `Package.swift` declares `platforms: [.macOS(.v27)]`
- [ ] `swift build -c release` completes clean on this macOS 27 machine
- [ ] `swift test` green (the existing 32-test core suite)
- [ ] No other source changes (the app surface is frozen by the spec)
- [ ] Evidence captured: the before (v26 declaration + build) and after (v27 + clean release build + green test suite) output
