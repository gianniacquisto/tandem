import MCP

/// The three v1 tools, with the accepted-verbatim descriptions (spec: the descriptions
/// are the agent's only documentation — they carry the shared-workbench framing and name
/// the store path).
enum TandemTools {
    static let list = Tool(
        name: "list",
        description: "List todos from the shared Tandem list — the same todos a human sees and edits in the Tandem app on this Mac. Store: ~/.tandem/todos.json. Optional argument `filter`: `open` (default), `done`, or `all`. Returns todos in creation order as JSON: `id`, `title`, `done`, `createdAt`, `completedAt` (ISO-8601 UTC; `completedAt` is null while open).",
        inputSchema: .object([
            "type": "object",
            "properties": .object([
                "filter": .object([
                    "type": "string",
                    "enum": .array(["open", "done", "all"]),
                    "description": "Which todos to return: open (default), done, or all."
                ])
            ])
        ])
    )

    static let add = Tool(
        name: "add",
        description: "Add a todo to the shared Tandem list (store: ~/.tandem/todos.json), which a human on this Mac shares with you through the Tandem app. Argument: `title` (required; trimmed, empty is rejected). Duplicate titles are allowed. Returns the created todo as JSON, including the `id` you can pass to `complete`.",
        inputSchema: .object([
            "type": "object",
            "properties": .object([
                "title": .object([
                    "type": "string",
                    "description": "The todo title (required; trimmed, empty is rejected)."
                ])
            ]),
            "required": .array(["title"])
        ])
    )

    static let complete = Tool(
        name: "complete",
        description: "Mark a todo as done on the shared Tandem list (store: ~/.tandem/todos.json). Argument: `id` (from `list` or `add`). Idempotent: completing an already-done todo returns it unchanged. If the id is unknown, the error includes the current open list so you can pick the right one. There is no reopen in v1.",
        inputSchema: .object([
            "type": "object",
            "properties": .object([
                "id": .object([
                    "type": "string",
                    "description": "The todo id (from list or add)."
                ])
            ]),
            "required": .array(["id"])
        ])
    )

    static let all: [Tool] = [list, add, complete]
}
