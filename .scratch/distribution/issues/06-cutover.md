# 06: Public cutover to v0.5.0 — supervised checklist (U6)

**What to build:** The public cutover, walked deliberately with the human: tag v0.5.0 on the merged cutover work → the release workflow publishes the artifact → 04 and 05 are merged → the live machine migrates (uninstall the old brew records, then `brew install --cask gianniacquisto/tap/tandem`) → the install is verified. Each public step (tag push, tap merge, live migration) happens on the human's explicit go-ahead. Not a subagent unit.

**Blocked by:** 04, 05. Note: the first two steps (tag push, release publish) are the supervised gate that 04 and 05 also wait on; they run as soon as 03 is merged, on the human's explicit go-ahead.

**Status:** resolved

Context: [spec](../spec.md) — "Migration and version", "Testing Decisions" (the cutover verification), and the U6 unit brief under "Execution model".

- [x] Tag v0.5.0 pushed (human go-ahead); the release publishes with `Tandem-0.5.0.zip` + SHA256 checksum
- [x] 04 and 05 merged (the tap merge on human go-ahead)
- [x] Live migration run: old record(s) uninstalled, the cask installed (on this Mac the live state is the 0.4.0 formula; the old cask is not installed)
- [x] Install verification: `/Applications/Tandem.app` is a real bundle (not a symlink), launches, appears in Launchpad, `open -a Tandem` works, the in-bundle MCP binary passes the stdin-EOF smoke, and `brew list` shows a single current cask record with no formula record
- [x] The pristine-machine install gap is noted (no second Mac or VM available on this machine)

## Resolution (2026-10-13)

Cutover done: `v0.5.0` tagged (`475bef9`) and published; 04/05 merged; live machine migrated to the cask (formula 0.4.0 removed, single cask record 0.5.0, real bundle, launches, in-bundle MCP passes the stdin-EOF smoke).

The cutover surfaced a shipped-cask defect: `app "Tandem"` while the zip contains `Tandem.app` — brew installs the wrong thing. Fixed in the tap by a separate PR (merged 2026-10-13).

Gatekeeper: a fresh install arrives quarantined and the ad-hoc-signed bundle is SIGKILLed ~10s after launch; user approval did not persist, falsifying ADR 0003's one-time-friction assumption. Chosen mitigation (owner): the documented one-line `xattr -dr com.apple.quarantine` strip, once per install/upgrade (README + 0.5.0 release notes). An in-app self-strip was built and tested and rejected. ADR 0003 updated; the notarization revisit trigger fired (owner) and the owner chose deferral.

Known gaps:

- The first-launch flow (warning → kill → strip → clean relaunch) is verified on this machine only; a true pristine install (fresh user, fresh tap) is unexercised.
- The tap `autobump` has never produced a bump PR (0.5.0's cask stanza was written by hand).
- Release re-cut reproducibility depends on the self-hosted runner, which does not survive a Mac reboot.
