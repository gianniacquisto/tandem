# Tandem

Tandem is a native macOS todo app for you and your AI coding agents.

![The tandem bike — the app's brand mark and icon](Resources/TandemBike.jpg)

## Install

macOS 27 or later, on an Apple silicon Mac.

```sh
brew install --cask gianniacquisto/tap/tandem
```

That puts `Tandem.app` in `/Applications`. Homebrew will ask you to trust the tap the first time — that's normal.

### First launch (once per install or upgrade)

Tandem is signed but not notarized by Apple, so macOS shows a Gatekeeper warning the first time you launch it after a download, and the app quits by itself a few seconds later. Clear the download flag once, then relaunch:

```sh
xattr -dr com.apple.quarantine /Applications/Tandem.app
```

Tandem then opens normally until the next install or upgrade.

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

### The `tandem-memory` skill

`skills/tandem-memory/` is an agent skill that makes the bikeshed (`~/.tandem/bikeshed/`) the agent's memory. A session orients from the vault's `Index.md` and its project notes as it starts, then records decisions, gotchas, and open threads as it works, so the next session picks up where the last one left off. The notes double as a wiki of the work, navigable by wikilinks and backlinks in the app.

This skill is a folder, not a single file, so it installs differently from the one-file skills. `SKILL.md` is the entry point — the only file the harness loads when the skill fires — and it holds the two things the agent does: **orient**, then **record**. The other files are companions that `SKILL.md` points to and the agent reads only when it needs them:

| file | what the agent reaches it for |
| --- | --- |
| `CONVENTIONS.md` | where notes go and how they are named |
| `WRITE-PROTOCOL.md` | when and what to record |
| `READ-PROTOCOL.md` | how to orient and find related notes |
| `CONCURRENCY.md` | two sessions writing the same vault |

Install it one of two ways.

Point your harness at this repo's `skills/` directory, so there is one copy and no re-install when the repo changes. For OpenCode, add that directory to the `skills` array in `opencode.json` (beside any entries already there):

```json
{
  "skills": ["/path/to/tandem/skills"]
}
```

Or copy the whole `tandem-memory/` folder into your harness's skills directory the way you would a single-file skill, keeping the files together — `SKILL.md` reaches the companions by their relative paths, so they only work as a set. Don't install the four companions as skills of their own.

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
