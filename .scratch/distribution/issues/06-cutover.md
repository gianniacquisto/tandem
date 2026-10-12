# 06: Public cutover to v0.5.0 — supervised checklist (U6)

**What to build:** The public cutover, walked deliberately with the human: tag v0.5.0 on the merged cutover work → the release workflow publishes the artifact → 04 and 05 are merged → the live machine migrates (uninstall the old brew records, then `brew install --cask gianniacquisto/tap/tandem`) → the install is verified. Each public step (tag push, tap merge, live migration) happens on the human's explicit go-ahead. Not a subagent unit.

**Blocked by:** 04, 05. Note: the first two steps (tag push, release publish) are the supervised gate that 04 and 05 also wait on; they run as soon as 03 is merged, on the human's explicit go-ahead.

**Status:** ready-for-human

Context: [spec](../spec.md) — "Migration and version", "Testing Decisions" (the cutover verification), and the U6 unit brief under "Execution model".

- [ ] Tag v0.5.0 pushed (human go-ahead); the release publishes with `Tandem-0.5.0.zip` + SHA256 checksum
- [ ] 04 and 05 merged (the tap merge on human go-ahead)
- [ ] Live migration run: old record(s) uninstalled, the cask installed (on this Mac the live state is the 0.4.0 formula; the old cask is not installed)
- [ ] Install verification: `/Applications/Tandem.app` is a real bundle (not a symlink), launches, appears in Launchpad, `open -a Tandem` works, the in-bundle MCP binary passes the stdin-EOF smoke, and `brew list` shows a single current cask record with no formula record
- [ ] The pristine-machine install gap is noted (no second Mac or VM available on this machine)
