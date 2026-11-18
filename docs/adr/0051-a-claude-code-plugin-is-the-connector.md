# A Claude Code plugin is the connector

> **Status:** In force. See [the index](README.md).

Connecting Claude Code to Tandem meant pasting a JSON block into `~/.claude.json` and copying skills by hand. Claude Code installs plugins from a marketplace, which is a git repository with a manifest, so this repository can be its own marketplace and the connection becomes two commands.

Decided 2026-11-18 (MCP-9).

## Decision

- **The repository is the marketplace.** `.claude-plugin/marketplace.json` at the root lists one plugin, `tandem`. Install is `/plugin marketplace add gianniacquisto/tandem` then `/plugin install tandem@tandem`.
- **`examples/` is the plugin's root.** `examples/.claude-plugin/plugin.json` makes the folder a plugin, so `examples/skills/` and `examples/agents/` load as they are, every one of them, by the human's choice. The plugin's own files sit in `examples/claude-plugin/`: `mcp.json` registers the streamable-HTTP endpoint the README documents, with `TANDEM_URL` and `TANDEM_AGENT` as overrides and `claude` as the default actor, and `hooks.json` runs a `SessionStart` check.
- **The plugin follows releases.** The marketplace entry's source is `git-subdir` of this repository at path `examples`, with `ref` naming the latest release tag. Claude Code reads the marketplace from `main` and fetches the plugin from that tag, so a user's skills and agents are the ones that shipped with the binary. At a release, after the tag is pushed, `Scripts/pin-plugin.sh X.Y.Z` moves the `ref` and the change lands on `main` (`docs/releasing.md`). Tag first, pin second: a `ref` naming an unpushed tag breaks every install. Until the first release that contains the plugin, the `ref` is `main`.
- **No version field.** Claude Code versions a `git-subdir` plugin by the commit its `ref` resolves to, so a moved pin is an update and a release needs no second number.
- **The startup check reports, and never acts.** It says when no server answers, when the running server's `data-version` differs from `tandem --version`, and when GitHub's latest release is newer than the installed binary (asked at most once a day, cached under `~/.cache/tandem/`, off with `TANDEM_NO_UPDATE_CHECK=1`). It is silent otherwise and always exits 0.

## Why

- **ADR 0045 retired `setup install`, not installing.** That cut removed an installer Tandem owned. Here the agent host owns the install, the update and the removal, and Tandem ships files only.
- **ADR 0047 holds.** The plugin starts no process and upgrades no binary: `tandem serve` is started by hand, and the check only reports.
- **Releases, not `main`.** A skill on `main` may name a tool the released binary does not serve yet; pinning to the tag keeps the prompts and the server in step.
- **The root, not links.** A `git-subdir` fetch takes one folder; links from a plugin folder to `examples/skills` would point outside it and be dropped. Making `examples/` the root needs no links, which also spares Windows clones that check links out as text.

## Consequences

- Claude Code leaves auto-update off for third-party marketplaces, and the publisher cannot turn it on. Each user enables it once in `/plugin`, or runs `/plugin marketplace update tandem`.
- A release has one more step, and forgetting it leaves the plugin one release behind, not broken.
- A token-guarded or remote serve needs the hand-written registration: the plugin sends no `Authorization` header.
- A source build of `tandem serve` reports the workspace version, not a release stamp, so the version check flags it as differing from an installed release.
- Claude.ai's web connectors directory is out of scope: it needs a public HTTPS server with OAuth, which [ADR 0048](0048-hosted-tandem-is-one-serve-per-workspace.md)'s hosted Tandem may later provide.

## Amendment, 2026-11-18: a refresh does not update the plugin

At the v2.5.0 pin, `/plugin marketplace update tandem` moved the marketplace clone to the new `ref` but left the installed plugin on its old commit; `/plugin uninstall tandem@tandem` then `/plugin install tandem@tandem` fetched the tag (DOC-6). The consequence above that a user "runs `/plugin marketplace update tandem`" now reads: refreshes the marketplace and reinstalls the plugin. Whether auto-update moves an installed plugin to a new pin is not yet observed. The plugin README and the startup check's upgrade line say the same.
