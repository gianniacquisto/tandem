# Map: Tandem — a todo app for humans and agents

Type: wayfinder:map

## Destination

A sharp, implementable spec for Tandem: a native macOS todo app (regular app: main window + menu bar item; Swift/SwiftUI; macOS 26+) whose todos live in a shared local store at `~/.tandem/todos.json` and are reachable by local agents through an MCP stdio server (official Swift SDK) exposing `list` / `add` / `complete` — such that a single implementation session can build app + server from the spec. The spec itself is written at handoff (e.g. `/to-spec`); this map only settles the decisions the spec depends on.

## Notes

- Domain: Swift/SwiftUI macOS app + MCP server (Model Context Protocol, stdio transport) + shared local JSON store.
- Tracker: local markdown — map at `.scratch/tandem/map.md`, tickets at `.scratch/tandem/issues/NN-<slug>.md`.
- Skills to consult: `grilling` + `domain-modeling` for grilling/prototype tickets; `research` for research tickets. `GLOSSARY.md` at repo root is the vocabulary authority.
- Settled during charting (conversationally, not as tickets):
  - Plan, don't do: the map produces decisions, not the app.
  - Agent surface: MCP stdio server built on the official Swift SDK (`modelcontextprotocol/swift-sdk` ≥ 0.12), a separate executable sharing a core module with the app.
  - Agents: local only, same OS user, same Mac. No auth, no network.
  - One app, two faces: main window + menu bar item; regular app (Dock icon, standard window).
  - v1 todo model: title + done + created/completed timestamps. Nothing else.
  - Menu bar item: dropdown of ~5 open todos (tap to toggle done) + quick-add field + "Open app".
  - Store: `~/.tandem/todos.json`; JSON + flock + atomic rename; the GUI watches the file for live updates.
  - MCP tool surface v1: `list` / `add` / `complete`.
  - macOS floor: 26+.
  - No notifications on agent activity; no change-origin tracking in v1.

## Decisions so far

<!-- one line per closed ticket: name (wrapping its link) + one-line gist -->

- [Verify the official Swift MCP SDK for a stdio tools server](issues/01-verify-swift-mcp-sdk.md): Official SDK 0.12.1 confirmed (Swift 6.0+/macOS 13.0 floor, so macOS 26+ is fine); tools-only server = `Server` + `StdioTransport` + `withMethodHandler` for `ListTools`/`CallTool`; package as a SwiftPM executable target sharing the core module, installed as a copy at stable `~/bin/tandem-mcp`.

## Not yet specified

- Menu bar icon: glyph choice, and count badge vs plain icon. The UI prototype should surface it as a decision.
- Main window list treatment: filter (open/done/all) vs sections, sort order, search. The UI prototype should make these concrete.
- Agent-setup docs for the spec (exact `opencode.json` / `.mcp.json` snippets): waits on the executable's final install path from the SDK verification ticket.

## Out of scope

- Due dates, priorities, lists/projects — v1 model is title + done only.
- Multi-machine sync, remote agents, auth — local same-user only.
- Notifications on agent activity (would require change-origin tracking in the store) — deferred by explicit decision.
- Extra MCP tools (update/rename, delete, read-one) and MCP resources — v1 surface is `list` / `add` / `complete`.
- CLI adapter for non-MCP agents or shell scripting — the MCP server is the agent surface.
- Distribution: App Store, notarization, code signing — personal local tool.
