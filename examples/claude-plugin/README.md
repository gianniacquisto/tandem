# Tandem plugin for Claude Code

The Tandem connector, packaged as a Claude Code plugin. It gives Claude the Tandem MCP tools and every example skill and agent from `examples/`, and checks at session start that `tandem serve` is up and current.

## Install

1. Install Tandem and start the server, and leave it running:

   ```sh
   curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh
   tandem serve
   ```

2. In Claude Code, add this repository as a marketplace and install the plugin:

   ```
   /plugin marketplace add gianniacquisto/tandem
   /plugin install tandem@tandem
   ```

   In the desktop app, the same two steps are in the plugin browser: add the marketplace `gianniacquisto/tandem`, then install `tandem`.

3. Start a new session and ask: "file a Tandem ticket called hello". It appears on the board.

## Updates

The plugin follows Tandem's releases, not `main`: the marketplace pins it to the latest release tag, so the skills and agents you get are the ones that shipped with the binary you installed.

- **Update by hand after a release.** Refresh the marketplace, then reinstall the plugin:

  ```
  /plugin marketplace update tandem
  /plugin uninstall tandem@tandem
  /plugin install tandem@tandem
  ```

  The refresh alone is not enough: it moves Claude Code's copy of the marketplace to the new pin but leaves the installed plugin where it was (seen at v2.5.0). The reinstall fetches the plugin from the pinned tag. The new version loads in your next session.
- **Or turn on auto-update.** Claude Code leaves it off for marketplaces outside Anthropic's own: run `/plugin`, open **Marketplaces**, select `tandem`, choose **Enable auto-update**. It runs once per session, within about ten minutes of your first message. Whether it moves an installed plugin to a new pin has not been observed yet; if the plugin stays behind, reinstall as above.
- **The binary is yours to upgrade.** The plugin never touches it. At session start the plugin tells Claude when a newer release is out, with the reinstall command, and when the running `tandem serve` is a different version from the installed binary (restart it).
- The release check asks GitHub at most once a day and caches the answer in `~/.cache/tandem/latest-release`. Set `TANDEM_NO_UPDATE_CHECK=1` to turn it off.

## What it contains

The plugin's root is `examples/`, so the examples load as they are, with no copy and no links.

| Part | File | Does |
| --- | --- | --- |
| Manifest | `examples/.claude-plugin/plugin.json` | Names the plugin and points at the two files below. |
| MCP server | `claude-plugin/mcp.json` | Registers `tandem` as a streamable-HTTP server at `http://127.0.0.1:7700/mcp`. |
| Skills | `examples/skills/` | Every skill there: refine, sprint, tidy, interview, retro, review lenses, the team roster and the general skills (`grilling`, `tdd`, `writing-for-agents`). |
| Agents | `examples/agents/` | Every agent there: `scout`, `refiner`, `builder`, `reviewer`. |
| Hook | `claude-plugin/hooks.json`, `claude-plugin/scripts/check-serve.sh` | On session start or resume: is the server up, does it match the binary, is a newer release out. Silent when all three are fine. |

## Settings

Read from the environment Claude Code starts in:

- `TANDEM_URL`: the server's base URL, default `http://127.0.0.1:7700`. Set it when `tandem serve` runs on another port or host.
- `TANDEM_AGENT`: the actor Claude's writes are recorded under (the `X-Tandem-Agent` header), default `claude`. Lowercase letters, digits and hyphens.
- `TANDEM_NO_UPDATE_CHECK=1`: skip the release check.

A token-guarded server (`TANDEM_TOKEN`) is not covered: the plugin sends no `Authorization` header. For that case, register the server by hand as in the top-level README and add the header there. In a team workspace the header carries a personal token, which each member makes on the Personal tokens screen (see `docs/cli.md`).

## Notes

- `examples/AGENTS.md` (tier 1, the memory instructions) is not loaded: a plugin cannot add to Claude's standing instructions. Copy it into `~/.claude/CLAUDE.md` or import it from there.
- The plugin installs nothing into `~/.tandem` and does not start the server: `tandem serve` is started by hand (ADR 0047).
- Plugin skills and agents are namespaced: `/tandem:tandem-sprint`, the `tandem:builder` subagent. If you also copied the examples into `~/.claude/skills` or `~/.claude/agents`, both copies load; remove the hand-copied ones to keep one.
- The skills and agents encode this repository's own workflow (roster, statuses, review gate). `tandem-team` is a roster template: the plugin ships it as is, so edit a copy if your team differs.
- The agent files carry opencode fields (`mode`, `permissions`, hex `color`) that Claude Code ignores.
- Releasing: `docs/releasing.md` has the pin step (`Scripts/pin-plugin.sh`).
