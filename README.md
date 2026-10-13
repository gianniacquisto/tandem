# Tandem

Tandem is a native macOS todo app for you and your AI coding agents.

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

## Install

macOS 27 or later, on an Apple silicon Mac.

```sh
brew install --cask gianniacquisto/tap/tandem
```

That puts `Tandem.app` in `/Applications`. Homebrew will ask you to trust the tap the first time — that's normal.

### Upgrading from ≤ 0.4.x

Before 0.5.0, Tandem installed as a formula plus a linking cask. The migration is one-time, cask first (its uninstall removes the old `/Applications` symlink):

```sh
brew uninstall --cask tandem   # the old cask
brew uninstall tandem          # the formula
brew install --cask gianniacquisto/tap/tandem
```

## How it works

All your todos live in `~/.tandem/todos.json`, a plain JSON file you can edit by hand.

The app has a main window and a menu bar item. It watches the file, so the list updates live whenever anything changes it.

Agents read and write the same file through Tandem's MCP server (see below). When an agent adds a todo, it shows up in your app; when you check one off, the agent sees it on its next read.

Everything runs locally on your Mac. No accounts, no network.

## Using Tandem from an agent

The app bundles `tandem-mcp`, an MCP server with three tools: `list`, `add`, and `complete`. The path below survives upgrades.

Claude Code:

```sh
claude mcp add tandem -- /Applications/Tandem.app/Contents/MacOS/tandem-mcp
```

OpenCode — add this to `opencode.json`:

```json
{
  "mcp": {
    "tandem": {
      "type": "local",
      "command": ["/Applications/Tandem.app/Contents/MacOS/tandem-mcp"],
      "enabled": true
    }
  }
}
```

## The store file

`~/.tandem/todos.json` looks like this:

```json
{
  "version": 1,
  "todos": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "title": "Water the plants",
      "done": false,
      "createdAt": "2026-10-09T14:03:22Z",
      "completedAt": null
    }
  ]
}
```

If the file ever gets corrupted, Tandem moves it to `todos.json.corrupt-<timestamp>` and starts with an empty list. The full file protocol is in [ADR 0001](docs/adr/0001-store-protocol.md).

## Releasing

See [docs/releasing.md](docs/releasing.md).
