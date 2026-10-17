import Foundation
import MCP
import TandemCore

/// The three v1 tools, with the accepted-verbatim descriptions (spec: the descriptions
/// are the agent's only documentation — they carry the shared-workbench framing and name
/// the store path).
///
/// The path is the one *this* build uses, rendered with `~`: the shipped app's text is
/// byte-for-byte what the spec records, while a dev build names `~/.tandem-dev` — an
/// agent talking to a from-source server is told which list it is holding.
enum TandemTools {
    static let list = Tool(
        name: "list",
        description: "List todos from the shared Tandem list — the same todos a human sees and edits in the Tandem app on this Mac. Store: \(storePath). Optional argument `filter`: `open` (default), `done`, or `all`. Returns todos in the list's stored order (the order the human arranged; new todos land on top) as JSON: `id`, `title`, `done`, `section` (null for the default area), `createdAt`, `completedAt` (ISO-8601 UTC; `completedAt` is null while open).",
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
        description: "Add a todo to the shared Tandem list (store: \(storePath)), which a human on this Mac shares with you through the Tandem app. Argument: `title` (required; trimmed, empty is rejected). Duplicate titles are allowed. The new todo lands on top of the list's default area. Returns the created todo as JSON, including the `id` you can pass to `complete`.",
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
        description: "Mark a todo as done on the shared Tandem list (store: \(storePath)). Argument: `id` (from `list` or `add`). Idempotent: completing an already-done todo returns it unchanged. If the id is unknown, the error includes the current open list so you can pick the right one. There is no reopen tool (a human can reopen a done todo in the app).",
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

    /// This build's store path with the home directory shortened to `~`, so the
    /// shipped app's description reads exactly `~/.tandem/todos.json` and a dev
    /// build's reads `~/.tandem-dev/todos.json`.
    private static var storePath: String {
        let dir = TandemHome.directory
        let home = NSHomeDirectory()
        let shown = dir.path.hasPrefix(home + "/") ? "~" + dir.path.dropFirst(home.count) : dir.path
        return "\(shown)/todos.json"
    }
}
