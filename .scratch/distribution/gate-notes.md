# Supervised gate notes

Record of human go-aheads for the public steps (spec: each public step happens on the human's explicit go-ahead).

## Tag v0.5.0 + first release — PRE-APPROVED (2026-10-12, session pin)

- **Scope:** push tag `v0.5.0` at the tip of `cutover/distribution` (after PR #3 is merged) so the Release workflow publishes the first GitHub release: `Tandem-0.5.0.zip` + `SHA256SUMS`. Then verify the release page shows both assets.
- **Conditions (both must hold — if either fails, stop and re-ask the human):**
  1. PR #3 (release workflow) has a green `pull_request` run, the artifact is uploaded, and no GitHub release exists yet (baseline: `gh release list` empty before and after the PR run).
  2. PR #3 is merged into `cutover/distribution`, and there are no unresolved red runs on the branch.
- **Not covered by this approval — still need a live go-ahead:** the final PR to `main`, the tap PR merge (U5), and the U6 live-machine migration steps (uninstall formula, install cask, verify).
