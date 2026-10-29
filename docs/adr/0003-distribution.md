# Distribution: prebuilt cask, in-bundle MCP binary, no formula

> **Amended 2026-10-29 by [ADR 0034](0034-agent-face-in-the-artifact.md):** the prebuilt artifact now also carries `Contents/Resources/agent/` (the skills, the agent prompts and `AGENTS.md`, plus a `VERSION` marker), so a skill and the CLI it drives ship from one tag; `tandem-cli setup install` writes it into the Tandem home. The one-cask distribution decision is unchanged. The amendment's text is ADR 0034's.

The formula+cask pair (ADR 0002) gave Tandem two brew records under one name: after every release `brew upgrade` moves the formula forward and the cask's recorded version lags — brew flags Tandem as outdated although the running app is current — and the cask was an artifact that mirrored the formula's url/sha256 without ever downloading from it. Tandem is now a public, user-friendliness-first app, so we decided: **one brew record** — a cask that downloads a **prebuilt** `Tandem.app` (a zip) from a GitHub release built by CI on each tag; the MCP server ships **inside the app bundle** and agent configs point at that in-bundle path; the formula is removed; distribution is brew-only; the floor is macOS 27 (arm64-only); signing stays ad-hoc for now.

## Why a prebuilt cask, not the alternatives

- **Keep formula + cask, fix the staleness** — the dual identity is structural: a plain `brew upgrade` only touches formulae, so the cask's recorded version lags every release; no fix keeps two records under one name without a second upgrade action.
- **Formula only, no Launchpad** — rejected: Launchpad/Spotlight presence is a requirement; a todo app is something a human launches, not types.
- **Cask that builds from source** — casks install prebuilt artifacts by design; source-building is the formula's job, and it keeps the CLT requirement and a multi-minute compile on the user.
- **Drop Homebrew (DMG only)** — rejected: for a public app, `brew install --cask` is the friendlier, discoverable, one-upgrade-command channel, and the release artifact serves the cask either way.
- **Formula for the CLI + cask for the app** — two records again, and the agent surface is one documented line either way; an agent-only install (no GUI) is a corner we can serve later with a thin `tandem-cli` formula if it ever matters.

## Why the MCP binary lives in the app bundle

A cask cannot install binaries onto `PATH`; the formula was the only mechanism, and keeping a formula just for that revives the dual identity. The in-bundle path (`/Applications/Tandem.app/Contents/MacOS/tandem-mcp`) is just as stable — upgrades replace the app in place and the path never changes — and agent setup is a single copied line in the README.

## Why ad-hoc signing stays (for now)

Homebrew applies the quarantine attribute to cask downloads, so a fresh install is Gatekeeper-assessed on first launch. Homebrew does not refuse the install itself — its Gatekeeper audit applies to official casks only. **Observed 2026-10-13 (macOS 27):** the original assumption — that the user's approval (right-click → Open) makes the friction one-time — was falsified: the quarantined ad-hoc bundle is SIGKILLed ~10s after launch and the approval did not persist across relaunches. The working unblock is stripping the quarantine flag (`xattr -dr com.apple.quarantine /Applications/Tandem.app`), which fixes both the app and the in-bundle MCP binary. The documented mitigation (README + release notes) is that one-line strip, once per install or upgrade; an in-app self-strip was built and tested and rejected (the app deleting a security flag about itself). A Developer ID + notarization removes the friction entirely; we defer the $99/yr account — **the revisit trigger (first user stuck at first launch) fired for the owner on 2026-10-13, and the owner chose documented deferral**; next revisit: any external report of the friction. The deferral rationale is recorded internally only — user-facing docs state the warning and the fix, not the rationale.

## Consequences

- The cask's `url`/`sha256` are real again: it downloads the release artifact and brew pins its checksum.
- The tap's `autobump` (daily `brew bump`) livechecks the release-asset URL and opens the bump PR for the single cask file; the mirrored-sha dance is gone.
- The tap's bottle-publish workflow is retired — no formula, no bottles.
- The `tandem` and `tandem-mcp` commands leave `PATH`; terminal launch is `open -a Tandem`; the MCP server is reached at its in-bundle path.
- The old formula's `brew test` block is retired; its assertions (MCP binary launches and exits cleanly on stdin EOF; bundle intact; signature verifies) move into the release workflow.
- First release under this model is 0.5.0; existing installs (formula + linking cask) migrate once: uninstall the cask first (its uninstall removes the `/Applications` symlink), uninstall the formula, install the cask.
- macOS 26 / Intel Macs are unsupported: the cask's `depends_on macos: :golden_gate` makes brew refuse cleanly, and the README states the floor.

Supersedes [ADR 0002](0002-app-distribution.md).
