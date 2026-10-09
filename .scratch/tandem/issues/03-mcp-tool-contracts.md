# 03 - MCP tool contracts: list / add / complete

Type: grilling
Status: open
Blocked by: 02

## Question

Settle the exact agent-facing contracts for the three v1 tools so the MCP server implementation and the spec's tool table are unambiguous. The store schema comes from "Store protocol: locking, schema v1, retention, editability".

1. **Names**: server name and tool names (server `tandem` with tools `list` / `add` / `complete`? or prefixed `todo_add`?). State how each surfaces in OpenCode and Claude Code.
2. **`list`**: arguments (filter open/done/all? a limit?), result shape (which fields per todo, per schema v1), ordering, empty-list behaviour.
3. **`add`**: arguments (just title?), result (the created todo with its id, or an ack?), validation (empty/whitespace title, max length?), duplicate-title handling.
4. **`complete`**: argument (id), result, behaviour for an unknown id and for an already-done id (error vs idempotent ack), and whether any `reopen` path exists (probably not in v1 — say so explicitly).
5. **Error conventions**: how failures are returned (MCP tool error vs a result carrying an error field) and the message style an agent can act on.
6. **Tool descriptions**: the agent-facing description text for each tool. This is the agent's documentation: it should carry the shared-workbench framing (humans and agents share one list) and point at the store path.
