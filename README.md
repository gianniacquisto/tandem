# Tandem

Tandem is a native macOS todo app for you and your AI coding agents.

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

## Install

```sh
brew install --cask gianniacquisto/tap/tandem
```

That puts `Tandem.app` in `/Applications` and the `tandem` and `tandem-mcp` binaries on your `PATH`. Homebrew will ask you to trust the tap the first time — that's normal.

## How it works

All your todos live in `~/.tandem/todos.json`, a plain JSON file you can edit by hand.

The app has a main window and a menu bar item. It watches the file, so the list updates live whenever anything changes it.

Agents read and write the same file through Tandem's MCP server (see below). When an agent adds a todo, it shows up in your app; when you check one off, the agent sees it on its next read.

Everything runs locally on your Mac. No accounts, no network.

## Using Tandem from an agent

The install includes `tandem-mcp`, an MCP server with three tools: `list`, `add`, and `complete`.

Claude Code:

```sh
claude mcp add tandem -- /opt/homebrew/bin/tandem-mcp
```

OpenCode — add this to `opencode.json`:

```json
{
  "mcp": {
    "tandem": {
      "type": "local",
      "command": ["/opt/homebrew/bin/tandem-mcp"],
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

## Building from source

macOS 26+, Swift 6.0+ — the Xcode Command Line Tools are enough.

```sh
swift build            # debug
swift build -c release # release
swift run Tandem
swift test
```

## Releasing

See [docs/releasing.md](docs/releasing.md).
