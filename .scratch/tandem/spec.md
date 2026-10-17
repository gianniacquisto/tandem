# Spec: Tandem — a todo app for humans and agents

Type: spec
Status: ready-for-agent
Source: `.scratch/tandem/map.md` + tickets 01–04, ADR 0001, branch `research/swift-mcp-sdk` (SDK findings), branch `prototype/tandem-ui` (UI prototype; winner: variant A)

## Problem Statement

A human and several local AI agents share one Mac, but they have no shared todo list. The human's todos live in apps the agents can't reach, and the agents have nowhere to record a follow-up or pick up what the human queued. The result is two private lists where there should be one workbench: the human can't see what the agents promised, and the agents can't see what the human is tracking.

## Solution

Tandem is a native macOS todo app — a regular app (Dock icon, standard window) with a main window and a menu bar item, built in Swift/SwiftUI for macOS 26+. All todos live in one shared local Store at `~/.tandem/todos.json`. Local agents (same OS user, same Mac, no auth, no network) read and write the same todos through Tandem's MCP server: a Model Context Protocol server over stdio, built on the official Swift SDK, exposing exactly three tools — `list`, `add`, `complete`. The app watches the store file and refreshes live, so agent activity appears in the UI without refreshing; agents re-read through the store on every call, so their view is always current. One list, two kinds of actor, no divergence.

## User Stories

1. As a Human, I want to see all my todos in the Main window, so that I have one complete picture of what is outstanding and what is done.
2. As a Human, I want to add a todo from a quick-add field in the Main window (Return commits), so that capturing an idea costs one keystroke-heavy action, not a form.
3. As a Human, I want to tap a row to toggle a todo's done state, so that completing or undoing a todo is a one-action gesture.
4. As a Human, I want a Filter control (Open / Done / All) with live counts in the Main window, so that I can focus on what is open or review what is done without scrolling past the rest.
5. As a Human, I want done todos to render dimmed with strikethrough, so that state is legible at a glance.
6. As a Human, I want todos displayed in creation order, so that the list matches the order I captured things — and the order agents see them.
7. As a Human, I want a "Clear done" action in the Main window (disabled when nothing is done), so that completed todos can be removed in one bulk write.
8. As a Human, I want to delete a single todo from the Main window, so that a todo that no longer matters can go away even if it was never completed (this is GUI-only in v1; agents have no delete).
9. As a Human, I want the Main window list to update live when an agent adds or completes a todo, so that I can watch work happen without refreshing.
10. As a Human, I want each Main window row to show how long ago the todo was created, so that I know how stale an item is.
11. As a Human, I want a notification naming the backup path when a corrupt store file is recovered, so that I understand why my list appeared empty and where the old data went.
12. As a Human, I want a menu bar item as Tandem's quick-glance presence, so that I can check todos without opening the app.
13. As a Human, I want the menu bar dropdown to show up to 5 open todos in creation order, so that the quick glance surfaces the most relevant open work.
14. As a Human, I want to tap an open todo in the dropdown to mark it done, so that a quick win closes in two clicks.
15. As a Human, I want a "… n more — open the app" row in the dropdown when there are more than 5 open todos, so that I know the glance is truncated.
16. As a Human, I want a quick-add field at the top of the dropdown (Return commits), so that I can capture a todo without opening the app.
17. As a Human, I want an "Open Tandem" action in the dropdown, so that I can jump from the menu bar to the Main window.
18. As a Human, I want the menu bar icon to be a plain checklist glyph with no count badge, so that the menu bar stays quiet (badge explicitly rejected in the prototype).
19. As a Human, I want the menu bar dropdown to reflect store changes made elsewhere (e.g. an agent completing an item), so that the glance is always current.
20. As a Human, I want Tandem to behave like a regular macOS app (Dock icon, standard window, normal window management), so that it needs no mental model beyond the apps I already use.
21. As an Agent, I want a `list` tool, so that I can see the shared todo list before I act on it.
22. As an Agent, I want `list` to take an optional Filter argument (`open` default / `done` / `all`), so that I can scope the list to what I care about.
23. As an Agent, I want `list` to return todos in creation order with a stable schema (`id`, `title`, `done`, ISO-8601 UTC `createdAt`/`completedAt`), so that I can parse results identically across calls and clients.
24. As an Agent, I want an empty list to be a successful result (`{"todos": []}`), so that "nothing there" never looks like a failure.
25. As an Agent, I want an `add` tool that takes just a title, so that recording a follow-up is a one-call action.
26. As an Agent, I want `add` to return the created todo including its id, so that I can complete it later with `complete`.
27. As an Agent, I want duplicate titles to be allowed on `add`, so that I am never blocked by an accidental duplicate.
28. As an Agent, I want an empty or whitespace-only title to be rejected with a clear, actionable error, so that I know exactly what to fix and retry.
29. As an Agent, I want a `complete` tool that takes an id, so that I can mark a todo done the moment the work is done.
30. As an Agent, I want `complete` to be idempotent on an already-done id (returned unchanged, no error), so that retries and double-completion are harmless.
31. As an Agent, I want an unknown-id error from `complete` to include the current full open list in the same shape as `list`, so that a stale or hallucinated id recovers in one turn without a second round-trip.
32. As an Agent, I want a top-level `note` in the result whenever a call triggered corrupt-store recovery, so that I know my prior model of the list is void and why.
33. As an Agent, I want the tool descriptions to explain the shared-workbench framing (humans and agents share one list) and name the store path, so that I understand what the list is and who else sees it — the descriptions are my only documentation.
34. As an Agent, I want the server reachable at a stable `command` path (`~/bin/tandem-mcp`) in my client config, so that the integration survives app rebuilds.
35. As an Agent, I want domain failures to come back as short, actionable plain-text error results (not protocol errors), so that I can act on them without parsing.
36. As a Human, I want my todos and the agents' todos to be the same todos in the same file, so that there is one source of truth and no sync to reason about.
37. As a Human, I want concurrent access — the app plus several agent-spawned server processes at once — to never corrupt the file, so that the list is trustworthy no matter who writes.
38. As a Human, I want the app to tolerate a hand-edited store file as long as required fields are intact (unknown extra fields ignored), so that the file remains human-legible and human-editable.

## Implementation Decisions

### Package and modules

One local SwiftPM package with three targets:

1. **Core module** (shared library) — the only place with real logic. Contains: the domain model, the Store, and the todo operations with their contract semantics. Both executables link it; nothing is shared between the executables except this module.
2. **GUI app** (executable, SwiftUI, macOS 26+) — a thin adapter: renders Main window + menu bar item, calls core operations, watches the store file.
3. **MCP server** (executable) — a thin adapter: decodes MCP requests, calls core operations, encodes results per the tool contract.

### Domain model (core)

- **Todo**: `id` (UUIDv4, string in the file), `title` (trimmed, no max length, duplicates allowed), `done` (boolean), `createdAt` / `completedAt` (ISO-8601 UTC; `completedAt` is null while open). Nothing else in v1.
- Collection order is **creation order, always**. Display sorting is a UI concern, never the Store's.

> **Superseded (2026-10-17, [ADR 0006](../../docs/adr/0006-stored-order-and-sections.md)):** order is now **stored** — a todo's position in the array is its position in the list, `add` lands on top, and the store gained an ordered `sections` list with an optional `section` on each todo. Those fields are *additive*, so the envelope **version stays 1** (see ADR 0006 for why that matters).

### Store (core) — per ADR 0001

- Single file at `~/.tandem/todos.json`; the directory is created on first write. Schema v1, versioned envelope, todos in creation order:

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

- **Locking**: writers take an exclusive `flock` on the sidecar lockfile `~/.tandem/tandem.lock` (created if missing, never renamed), held across the entire read → modify → write-temp → `rename` cycle. Readers take no lock — atomic rename guarantees a reader sees a complete old or new file. `flock` releases on process death; no stale-lock recovery exists to build.
- **Contention**: retry with 10–50 ms backoff for up to ~2 s, then fail with a clear "store locked by another process — retry" error.
- **Atomic writes**: temp file in the same directory (required for atomic rename), named `.todos-<random>.tmp`, fsync before `rename`. After acquiring the lock, each writer deletes orphaned `.todos-*.tmp` files older than 60 s (crash leftovers; the age threshold never touches in-flight writes).
- **Lenient read, strict write**: unknown extra fields are ignored (forward-compat); a missing or mistyped required field invalidates the whole file — never silently discarded.
- **Recovery**: on an invalid file, under the lock (so the app and an MCP server can't double-recover the same file), rename it to `todos.json.corrupt-<unix-timestamp>`, start from an empty store, and surface the backup path to the GUI (notification) and to MCP results (the `note` field). Any operation — including reads — can trigger recovery, so core operation results must carry an optional recovery note.

### Operations (core) — contract semantics from the MCP grilling

These semantics are defined in core so the GUI and the MCP server share them; the MCP server adds only encoding.

- **list(filter)**: `filter` is `open` (default), `done`, or `all`; any other value is an error ("filter must be one of open, done, all"). Results in creation order, no limit. Empty is a successful empty list.
- **add(title)**: title is trimmed; empty/whitespace is rejected with an actionable error. Duplicate titles allowed. Returns the created todo (id generated, `completedAt` null).
- **complete(id)**: idempotent — an already-done todo is returned unchanged, no error (completing is a state assertion). On success, `done` is set true and `completedAt` to the current UTC time. An unknown id is an error that names the bad id and carries the full current open list in the `list` shape, so the agent recovers in one turn. No reopen path in v1.
- **clearDone()**: removes all done todos in one write (GUI "Clear done" and the retention rule).
- **delete(id)**: removes a single todo. Exposed to the GUI only — there is no delete tool in the MCP v1 surface.
- **Error convention**: domain-level failures are result-level errors (typed in core; `isError: true` tool results at the MCP boundary) carrying short, actionable plain text that names the failed value and the next step. Protocol-level failures (malformed request, unknown method) remain thrown MCP errors handled by the SDK.

### MCP server executable

> **Superseded (2026-10-17, [ADR 0009](../../docs/adr/0009-cli-agent-face.md)):** the MCP server retires; `tandem-cli` is the agent face, an in-bundle binary carrying the same three operations under `tandem-cli todo list|add|complete`, with the tool descriptions verbatim in `skills/tandem/SKILL.md`. The tools, schemas, verbatim descriptions, and result encoding below describe the retired surface.

- Official Swift SDK (`modelcontextprotocol/swift-sdk` ≥ 0.12): `Server(name: "tandem", version:, capabilities:)` + `StdioTransport()` + `withMethodHandler` for `ListTools` and `CallTool`. `start(transport:)` returns immediately, so the executable must stay alive via `waitUntilCompleted()`.
- **Tools**: bare names `list` / `add` / `complete` (the server prefix already namespaces them). Client surfacing: `tandem_list` / `tandem_add` / `tandem_complete` in OpenCode (configured under the `mcp` key of `opencode.json`: `"type": "local"`, `command` an array holding the absolute path to the installed binary); `mcp__tandem__list` / `mcp__tandem__add` / `mcp__tandem__complete` in Claude Code (`claude mcp add tandem -- <path>` or `mcpServers` in `.mcp.json`).
- **Input schemas**: `list` — optional `filter` (enum: open/done/all). `add` — required `title` (string). `complete` — required `id` (string).
- **Result encoding**: every successful result is JSON matching store schema v1 verbatim — `list` returns `{"todos": [...]}`; `add` / `complete` return the todo object. One extension: a top-level `note` string, present only when the call triggered corrupt-file recovery, e.g. `"note": "Previous store was corrupt and backed up to ~/.tandem/todos.json.corrupt-<ts>; list restarted empty."`
- **Text results**: use the `.text(text:annotations:_meta:)` content case — the README's `.text("...")` factory is deprecated.
- **Packaging**: `swift build -c release`, then install the binary to the stable path `~/bin/tandem-mcp` (absolute path in client configs), so the `command` survives app rebuilds. Document this install step in the repo README.
- Tool descriptions, verbatim (accepted in the contract grilling — this text is the agent's only documentation):
  - `list`: "List todos from the shared Tandem list — the same todos a human sees and edits in the Tandem app on this Mac. Store: ~/.tandem/todos.json. Optional argument `filter`: `open` (default), `done`, or `all`. Returns todos in creation order as JSON: `id`, `title`, `done`, `createdAt`, `completedAt` (ISO-8601 UTC; `completedAt` is null while open)."
  - `add`: "Add a todo to the shared Tandem list (store: ~/.tandem/todos.json), which a human on this Mac shares with you through the Tandem app. Argument: `title` (required; trimmed, empty is rejected). Duplicate titles are allowed. Returns the created todo as JSON, including the `id` you can pass to `complete`."
  - `complete`: "Mark a todo as done on the shared Tandem list (store: ~/.tandem/todos.json). Argument: `id` (from `list` or `add`). Idempotent: completing an already-done todo returns it unchanged. If the id is unknown, the error includes the current open list so you can pick the right one. There is no reopen in v1."

### GUI app executable

- Regular app: Dock icon, standard window, SwiftUI, macOS 26+ floor.
- **Main window** (prototype verdict: variant A "Filter tabs"):
  - Header: app title + "Clear done" (disabled when there are no done todos).
  - Quick-add field (Return commits).
  - Segmented Filter control — Open · Done · All — with live counts per filter.
  - List in creation order. Row: checkbox glyph (open circle / filled checkmark), title (strikethrough + dimmed when done), relative creation time trailing. Tap row to toggle done.
  - Per-todo delete affordance (GUI-only; removes that todo in one write).
  - Live updates: the app watches the store file and refreshes the UI on external changes (its own writes refresh too). The watching mechanism (e.g. FSEvents/DispatchSource) is an implementation detail — the behavior is specified.
  - On corrupt-store recovery: notify the human with the backup path.
- **Menu bar item**:
  - Icon: SF Symbol `checklist`, template rendering, plain — no count badge.
  - Menu, top to bottom: quick-add field (Return commits); up to 5 open todos in creation order, tap a row to toggle done; "… n more — open the app" when there are more than 5 open; "All clear" when there are none open; separator; "Open Tandem" (activates the app and brings the Main window forward).
  - The menu is rebuilt on every open; menu content and icon state refresh when the store changes.
- No notifications on agent activity; no change-origin tracking in v1.

## Testing Decisions

- **One seam: the core module.** It is the only module with real logic; the GUI and the MCP server are thin adapters and get no unit tests. A good test here exercises external behavior only — what operation results and errors come back, and what the store file contains on disk — never internal locking mechanics or parse structure.
- **Store tests** (real files in a temp directory, real processes for contention):
  - Round-trip: write → read back yields schema v1 shape with creation order preserved.
  - Atomicity: an unlocked reader never observes a partial file while another process writes (file is always parseable).
  - Contention: two writers racing both succeed via bounded retry; a lock held beyond ~2 s produces the clear "store locked" error.
  - Orphan cleanup: a pre-seeded stale `.todos-*.tmp` (> 60 s old) is removed after the next write; a fresh temp file is untouched.
  - Recovery: invalid JSON / missing required field / mistyped field → file renamed to `todos.json.corrupt-<ts>`, store restarts empty, backup path surfaced in the operation result; a concurrent second process does not double-recover the same file.
  - Lenient read: a valid file with unknown extra fields reads successfully.
  - Recovery note: an operation that triggers recovery carries the note; one that doesn't doesn't.
- **Operation tests** (against a real temp-dir store):
  - `add`: trims; empty/whitespace rejected; duplicates allowed; returned todo has an id and null `completedAt`.
  - `complete`: sets `done` + `completedAt`; idempotent second call returns unchanged with no error; unknown id errors with the full current open list.
  - `list`: each filter plus the default; invalid filter errors; creation order; empty list succeeds.
  - `clearDone`: removes all done in one write, leaves open untouched. `delete`: removes exactly one todo.
- **Prior art**: none — this is a greenfield repo; the core test target is new.
- **MCP server**: a manual stdio smoke test (spawn the installed binary, `initialize` handshake, one call per tool) — the pattern was validated in ticket 01's research, with the known caveat that hand-rolled `echo |` handshakes can lose the response if stdin closes within ~50 ms of startup (real clients keep stdin open).
- **GUI**: manual. Layout and interaction were already judged in the prototype (branch `prototype/tandem-ui`).

## Out of Scope

- Due dates, priorities, lists/projects — the v1 todo model is title + done + timestamps only.
- Search in the Main window (rejected with the prototype's losing variants).
- Reopen of a done todo (no path in v1; stated in the `complete` description).
- Count badge on the menu bar icon (explicitly rejected in the prototype).
- Multi-machine sync, remote agents, authentication — local same-user only.
- Notifications on agent activity and change-origin tracking — deferred by explicit decision.
- Extra MCP tools (update/rename, delete, read-one) and MCP resources — the v1 surface is `list` / `add` / `complete`. Delete exists in core and the GUI only.
- CLI adapter for non-MCP agents or shell scripting — the MCP server is the agent surface.
- Distribution: App Store, notarization, code signing — personal local tool.

## Further Notes

- **Decision provenance**: every decision above traces to the wayfinder map and its resolved tickets (01 SDK, 02 store protocol, 03 tool contracts, 04 UI), ADR 0001, and the two capture branches: `research/swift-mcp-sdk` (full SDK findings, including the deprecation and process-lifetime gotchas) and `prototype/tandem-ui` (all UI variants; variant A won).
- **Single implementation session** is the target: this spec is written so app + server can be built from it alone.
- The `note` field is the only extension beyond store schema v1 anywhere in the MCP results.
- SDK floor is Swift 6.0 / macOS 13.0, so the macOS 26+ deployment target is a non-issue (verified on this machine: Swift 6.4, macOS 27.0.1).
- Hand-editability of the store is a feature, not an accident: the file is intentionally human-legible JSON, and the lenient-read/strict-write rules are what make that safe.
