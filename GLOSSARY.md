# Tandem

A native macOS todo app shared by humans and agents on the same machine.

## Language

### The app

**Tandem**:
The macOS todo app, usable by both humans and agents on the same machine.
_Avoid_: "the app", "the todo app" (when ambiguous)

**Main window**:
Tandem's desktop face: the full todo list for browsing, adding, and editing.
_Avoid_: panel, dashboard, view

**Menu bar item**:
Tandem's presence in the macOS menu bar (top bar): quick glance and quick action on todos.
_Avoid_: top bar element, status bar, tray, badge

### The domain

**Todo**:
A single item with a title and a done state. The only entity in the v1 domain.
_Avoid_: task, item, to-do (hyphenated)

**Open**:
A todo that has not yet been completed.
_Avoid_: pending, active

**Done**:
A todo that has been completed.
_Avoid_: closed, finished, complete (as a noun)

**Filter**:
Which todos are in view: `open` (the default), `done`, or `all`. One term for two surfaces: the main window's segmented control and the MCP `list` tool's argument.
_Avoid_: view, tab, scope, state

**Store**:
The single source of truth for todos, shared by the app and agents. All reads and writes go through the store; no other copy is authoritative.
_Avoid_: database, backend, sync, cache

### The users

**Human**:
A person using Tandem through the main window or menu bar item.
_Avoid_: user (ambiguous with the OS account), client, owner

**Agent**:
A local AI coding agent (e.g. OpenCode, Claude Code) running as the same OS user on the same Mac, interacting with Tandem through the MCP server.
_Avoid_: bot, client, automation, user

**MCP server**:
Tandem's agent-facing interface: a Model Context Protocol server exposing todo operations as tools.
_Avoid_: API (too generic), backend, endpoint
