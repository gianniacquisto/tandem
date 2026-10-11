# Tandem

Tandem is a native macOS todo app shared by humans and AI coding agents.

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

## Install

```sh
brew install --cask gianniacquisto/tap/tandem
```

That puts `tandem` and `tandem-mcp` on your `PATH` and `Tandem.app` in `/Applications`. The first install asks you to trust the third-party tap — standard for Homebrew.

Or [build from source](#build-from-source).

## How it works

One list, two actors, no divergence. You work through the app — a **main window** (filter tabs, quick-add, one-tap complete) and a **menu bar item** (quick glance, quick add). Agents work through Tandem's **MCP server**, a Model Context Protocol server over stdio exposing three tools: `list`, `add`, `complete`.

Both faces read and write the same store at `~/.tandem/todos.json` — a human-legible, hand-editable JSON file. The app watches it and refreshes live; agents re-read it on every call, so the list is always current. Everything runs locally: same Mac, same user, no auth, no network.

## Features

- **Shared list** — humans and agents on one todo list, always in sync
- **Two faces** — main window plus menu bar item, both live-updating
- **MCP server** — `list`, `add`, `complete` for any MCP-speaking agent client
- **Hand-editable store** — plain JSON at `~/.tandem/todos.json`, safe under concurrent writes
- **100% local** — no accounts, no sync, nothing leaves your Mac

## MCP server

The Homebrew install gives you the server at a stable path: `/opt/homebrew/bin/tandem-mcp`. Wire it into an agent client:

**Claude Code**

```sh
claude mcp add tandem -- /opt/homebrew/bin/tandem-mcp
```

**OpenCode** — `mcp` key in `opencode.json`:

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

The server is named `tandem` and exposes the bare tool names `list`, `add`, `complete`; clients prefix them (`mcp__tandem__list`, `tandem_add`, …). `list` takes an optional `filter` (`open` default / `done` / `all`), `add` a required `title`, and `complete` the todo's `id` (idempotent — no reopen in v1).

## The store

`~/.tandem/todos.json` — a versioned envelope around a list of todos, in creation order:

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

Hand-editable by design: unknown fields are ignored, and a corrupt file is backed up to `todos.json.corrupt-<timestamp>` with the store restarting empty. Full protocol: [ADR 0001](docs/adr/0001-store-protocol.md).

## Build from source

macOS 26+, Swift 6.0+ (Xcode Command Line Tools are enough).

```sh
swift build            # debug
swift build -c release # release
swift run Tandem       # run the app
swift test             # run the tests
```

The GUI and the MCP server are thin adapters over the `TandemCore` library; tests target the core. Release binaries are ad-hoc signed by `swift build`, which is all Apple Silicon requires for agent clients to exec them directly.

## Releasing

See [docs/releasing.md](docs/releasing.md).
