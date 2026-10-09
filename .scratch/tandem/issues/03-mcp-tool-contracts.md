# 03 - MCP tool contracts: list / add / complete

Type: grilling
Status: resolved
Blocked by: 02

## Question

Settle the exact agent-facing contracts for the three v1 tools so the MCP server implementation and the spec's tool table are unambiguous. The store schema comes from "Store protocol: locking, schema v1, retention, editability".

1. **Names**: server name and tool names (server `tandem` with tools `list` / `add` / `complete`? or prefixed `todo_add`?). State how each surfaces in OpenCode and Claude Code.
2. **`list`**: arguments (filter open/done/all? a limit?), result shape (which fields per todo, per schema v1), ordering, empty-list behaviour.
3. **`add`**: arguments (just title?), result (the created todo with its id, or an ack?), validation (empty/whitespace title, max length?), duplicate-title handling.
4. **`complete`**: argument (id), result, behaviour for an unknown id and for an already-done id (error vs idempotent ack), and whether any `reopen` path exists (probably not in v1 — say so explicitly).
5. **Error conventions**: how failures are returned (MCP tool error vs a result carrying an error field) and the message style an agent can act on.
6. **Tool descriptions**: the agent-facing description text for each tool. This is the agent's documentation: it should carry the shared-workbench framing (humans and agents share one list) and point at the store path.

## Answer

Resolved by grilling (Q1–Q9, all accepted). Normative contract for the MCP server implementation and the spec's tool table:

**Names** — Server `tandem`; tools bare `list` / `add` / `complete` (the server prefix already namespaces them). Client surfacing: OpenCode registers them as `tandem_list` / `tandem_add` / `tandem_complete` (configured under the `mcp` key of `opencode.json`: `"type": "local"`, `command` an array holding the absolute path to the installed binary — `~/bin/tandem-mcp` per ticket 01); Claude Code as `mcp__tandem__list` / `mcp__tandem__add` / `mcp__tandem__complete` (`claude mcp add tandem -- <path>` or `mcpServers` in `.mcp.json`).

**Result encoding** — Every successful result is JSON matching store schema v1 verbatim: `list` returns `{"todos": [...]}`; `add` / `complete` return the todo object (`id`, `title`, `done`, `createdAt`, `completedAt`). One extension: a top-level `note` string, present only when the call triggered corrupt-file recovery (below).

**`list`** — One optional argument `filter`: `open` (default) | `done` | `all`; any other value is an error ("filter must be one of open, done, all"). Ordering: store order (creation order) — display sorting stays a UI concern. No `limit` in v1. Empty is a successful result: `{"todos": []}`.

**`add`** — One argument: `title` (required). Trimmed; empty/whitespace rejected with a clear error. No max length in v1. Duplicate titles allowed — the store defines no uniqueness. Result: the created todo object (with its id).

**`complete`** — One argument: `id`. Result: the updated todo object (`done: true`, `completedAt` set to the current UTC time). Already-done id: idempotent — return it unchanged, no error (completing is a state assertion; retries must be harmless). Unknown id: error that names the bad id and includes the full current open list in the same JSON shape as `list`, so a stale or hallucinated id recovers in one turn. No `reopen` in v1 — stated in the tool description.

**Error conventions** — Domain-level failures (unknown id, empty title, bad filter) are `isError: true` tool results carrying short, actionable plain text that names the failed value and the next step. Protocol-level failures (malformed request, unknown method) remain thrown MCP errors handled by the SDK.

**Recovery notice** — A call that triggers corrupt-file recovery (per ADR 0001 any call, including `list`, can be the one that does it) carries a top-level `note` in its result, e.g. `"note": "Previous store was corrupt and backed up to ~/.tandem/todos.json.corrupt-<ts>; list restarted empty."` The agent's prior model of the list is void in that case; the note says why.

**Tool descriptions** (accepted verbatim — this text is the agent's only documentation):

- `list`: "List todos from the shared Tandem list — the same todos a human sees and edits in the Tandem app on this Mac. Store: ~/.tandem/todos.json. Optional argument `filter`: `open` (default), `done`, or `all`. Returns todos in creation order as JSON: `id`, `title`, `done`, `createdAt`, `completedAt` (ISO-8601 UTC; `completedAt` is null while open)."
- `add`: "Add a todo to the shared Tandem list (store: ~/.tandem/todos.json), which a human on this Mac shares with you through the Tandem app. Argument: `title` (required; trimmed, empty is rejected). Duplicate titles are allowed. Returns the created todo as JSON, including the `id` you can pass to `complete`."
- `complete`: "Mark a todo as done on the shared Tandem list (store: ~/.tandem/todos.json). Argument: `id` (from `list` or `add`). Idempotent: completing an already-done todo returns it unchanged. If the id is unknown, the error includes the current open list so you can pick the right one. There is no reopen in v1."
