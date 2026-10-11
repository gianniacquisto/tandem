# Tandem

A native macOS todo app shared by **humans** and **agents** on the same Mac. One list, two kinds of actor, no divergence:

- The **human** works through the app — a **main window** (filter tabs, quick-add, one-tap complete) and a **menu bar item** (quick glance, quick add).
- **Agents** (local AI coding agents, same OS user, same Mac — no auth, no network) read and write the same todos through Tandem's **MCP server**, a Model Context Protocol server over stdio exposing three tools: `list`, `add`, `complete`.

All todos live in one **Store** at `~/.tandem/todos.json` — a human-legible, hand-editable JSON file. The app watches it and refreshes live; agents re-read it on every call, so both faces are always current.

## Requirements

- macOS 26+
- Swift 6.0+ (Xcode Command Line Tools are enough)

## Install

```sh
brew install --cask gianniacquisto/tap/tandem
```
Installs the formula — `tandem` and `tandem-mcp` on your `PATH` (`/opt/homebrew/bin` on Apple Silicon) — and places `Tandem.app` in `/Applications`, launchable from **Launchpad** and Spotlight. The cask only links the app bundle the formula builds ([ADR 0002](docs/adr/0002-app-distribution.md)), so `brew upgrade tandem` keeps everything current. Builds from source, so it needs the Xcode Command Line Tools. First-time users are asked to confirm trust in the tap — standard for third-party taps.

The app bundle is assembled by `Scripts/make-app-bundle.sh` from the built binary, `Resources/Tandem-Info.plist` (identity: `com.gianniacquisto.tandem`), and `Resources/AppIcon.icns` (regenerate with `Scripts/make-app-icon.sh`).

## Layout

One SwiftPM package, three product targets plus one test target:

| Target | What it is |
|---|---|
| `TandemCore` (library) | The only module with real logic: the domain model (Todo), the Store (locking, atomic writes, corrupt-file recovery — see [ADR 0001](docs/adr/0001-store-protocol.md)), and the todo operations with their contract semantics. |
| `Tandem` (executable) | The GUI app (SwiftUI, thin adapter over `TandemCore`): main window + menu bar item. |
| `tandem-mcp` (executable) | The MCP stdio server (thin adapter over the official [Swift MCP SDK](https://github.com/modelcontextprotocol/swift-sdk) + `TandemCore`). |
| `TandemCoreTests` | The single test seam — the core module only. |

The GUI and the MCP server share exactly one module: `TandemCore`.

## Build

```sh
swift build            # debug
swift build -c release # release
```

## Run the app

```sh
swift run Tandem
```

(A Homebrew install exposes the same binary as `tandem` on your `PATH`.)

Or click **Tandem** in Launchpad / Spotlight (the Homebrew install puts `Tandem.app` in `/Applications`).

Tandem is a regular macOS app: Dock icon, standard window, plus the menu bar item. It is **single-instance**: launching it while it is already running — from any path, bundle or binary — activates the running instance instead of opening a second one (a pidfile in `~/.tandem` links the two launch identities; a stale file after a crash is reclaimed on the next launch).

- **Main window**: header with **Clear done** (disabled when nothing is done); quick-add field (Return commits); segmented **Filter** — Open · Done · All — with live counts; todos in creation order; tap a row to complete it (strikethrough + dimmed when done); hover a row to delete it (GUI-only — agents have no delete).
- **Menu bar item** (plain `checklist` glyph, no count badge): quick-add field; up to 5 open todos (tap to complete); "… n more — open Tandem" when truncated; "All clear" when nothing is open; **Open Tandem**.
- The list updates live when the store changes elsewhere (e.g. an agent adds or completes a todo).
- If a corrupt store file is recovered, the app shows a banner naming the backup path (the old file is kept at `~/.tandem/todos.json.corrupt-<timestamp>`, never deleted).

## Install the MCP server

Client configs point at the server's path, so it must live somewhere **stable** that survives rebuilds. The Homebrew install gives you that for free:

```sh
brew install gianniacquisto/tap/tandem
# server path: /opt/homebrew/bin/tandem-mcp
```

**From source** instead:

```sh
swift build -c release
mkdir -p ~/bin
cp .build/release/tandem-mcp ~/bin/tandem-mcp
```

The binary is ad-hoc-signed by `swift build`, which is all Apple Silicon requires for the agent clients to `exec` it directly. No code signing or notarization.

### Wire it into an agent client

The server is named `tandem` and exposes the bare tool names `list`, `add`, `complete`; the client prefixes them.

**OpenCode** — `mcp` key in `opencode.json` (`type` local, `command` an array holding the absolute path):

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

Surfaced as `tandem_list`, `tandem_add`, `tandem_complete`.

**Claude Code** — either:

```sh
claude mcp add tandem -- /opt/homebrew/bin/tandem-mcp
```

or `mcpServers` in `.mcp.json`:

```json
{
  "mcpServers": {
    "tandem": {
      "command": "/opt/homebrew/bin/tandem-mcp",
      "args": []
    }
  }
}
```

Surfaced as `mcp__tandem__list`, `mcp__tandem__add`, `mcp__tandem__complete`.

### The tools

| Tool | Arguments | Result |
|---|---|---|
| `list` | optional `filter` (`open` default / `done` / `all`) | `{"todos": [...]}` in creation order; empty is a successful empty list |
| `add` | `title` (required; trimmed, empty rejected; duplicates allowed) | the created todo, including its `id` |
| `complete` | `id` (required) | the completed todo. Idempotent — an already-done todo comes back unchanged. An unknown `id` errors and carries the full current open list so the agent recovers in one turn. No reopen in v1. |

Every successful result is JSON matching the store schema verbatim. Domain failures come back as `isError` tool results with short, actionable plain text (not protocol errors). When a call triggers corrupt-store recovery, the result carries a top-level `note` naming the backup path.

## The store

`~/.tandem/todos.json` — schema v1, a versioned envelope, todos in creation order:

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

- **Hand-editable by design.** Unknown extra fields are ignored (forward-compat); a missing or mistyped required field invalidates the whole file — it is then backed up to `todos.json.corrupt-<timestamp>` and the store restarts empty, with the backup path surfaced to the app and to MCP results.
- **Safe under concurrency.** Writers take an exclusive `flock` on the sidecar lockfile `~/.tandem/tandem.lock` across the whole read → modify → write-temp → `rename` cycle; readers take no lock — the atomic rename guarantees a reader sees a complete old or new file. The app plus several agent-spawned server processes can write at once without corruption.

Protocol details: [ADR 0001](docs/adr/0001-store-protocol.md).

## Test

```sh
swift test
```

The single test seam is `TandemCore` (real files in temp directories, real processes for contention): store round-trip and atomicity, lock contention and bounded retry, orphan temp-file cleanup, corrupt-file recovery (including no double-recovery), lenient read, and the operation contracts (`list` / `add` / `complete` / `clearDone` / `delete`). The GUI and the MCP server are thin adapters and get manual testing only (the MCP smoke test is a stdio handshake plus one call per tool).

## Releasing

Two repos: this one holds the source; [`gianniacquisto/homebrew-tap`](https://github.com/gianniacquisto/homebrew-tap) holds the [Homebrew formula](https://github.com/gianniacquisto/homebrew-tap/blob/main/Formula/tandem.rb) that builds and installs it. brew only reads taps from its fixed tap directory, so on any dev machine the working copy is wherever brew keeps the clone — `brew tap gianniacquisto/tap` creates it at `$(brew --prefix)/Library/Taps/gianniacquisto/homebrew-tap`. That directory is an ordinary git clone of the tap repo: branch, commit, and push from there (`brew update` keeps it current).

To cut a release:

1. Tag this repo: `git tag v0.2.0 && git push origin v0.2.0`.
2. Bump the formula **and the cask** — `version`, the tag in `url`, and `sha256` (`curl -L <url> | shasum -a 256`; the cask mirrors the formula's). The tap's `autobump` workflow watches for new tags and opens the bump PR for you; merging it is the whole release. The workflow needs the tap's `HOMEBREW_GITHUB_API_TOKEN` secret (a token with `repo` scope) — without a working token the run fails after pushing the bump branch, so the PR has to be opened by hand.
3. If bumping by hand, verify before pushing: `brew style Formula/tandem.rb Casks/tandem.rb && brew audit --strict --formula gianniacquisto/tap/tandem && brew audit --cask --strict gianniacquisto/tap/tandem && brew test gianniacquisto/tap/tandem && brew install --build-from-source gianniacquisto/tap/tandem`.
4. Users get it with `brew update && brew upgrade tandem`.
