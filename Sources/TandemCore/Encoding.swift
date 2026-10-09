import Foundation

/// Result encoding for the MCP tool contract (spec: "Result encoding"). Every successful
/// result is JSON matching store schema v1 verbatim, plus one extension: a top-level
/// `note` string, present only when the call triggered corrupt-store recovery.
///
/// This lives in the core because it is the schema itself; the MCP server just wraps the
/// returned text in a tool content block.
public enum StoreEncoding {

    /// The `list` result: `{"todos": [...]}`, with a top-level `note` when non-nil.
    public static func listResult(_ todos: [Todo], note: String?) throws -> String {
        var object: [String: Any] = ["todos": try todoArray(todos)]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `add` / `complete` result: the todo object, with a top-level `note` when non-nil.
    public static func todoResult(_ todo: Todo, note: String?) throws -> String {
        var object = try todoObject(todo)
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The open list in the `list` shape — used to render the unknown-id error so the
    /// caller recovers in one turn (spec: tool contract, `complete`).
    public static func openList(_ todos: [Todo]) throws -> String {
        try serialize(["todos": try todoArray(todos)])
    }

    // MARK: - Internals

    private static func todoArray(_ todos: [Todo]) throws -> [Any] {
        try todos.map { try todoObject($0) }
    }

    /// A single todo as a JSON-compatible object. Encodes through `Todo`'s custom
    /// `Codable` so dates are ISO-8601 UTC and the id is a string, exactly as on disk.
    private static func todoObject(_ todo: Todo) throws -> [String: Any] {
        let data = try JSONEncoder().encode(todo)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode todo for result")
        }
        return object
    }

    private static func serialize(_ object: Any) throws -> String {
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        guard let text = String(data: data, encoding: .utf8) else {
            throw StoreIOError.io("could not serialize result to UTF-8")
        }
        return text
    }
}

/// Renders any Tandem failure to the short, actionable plain text the MCP surface returns
/// (spec: "Error convention"). For `DomainError.unknownID`, the current open list is
/// appended in the `list` shape. A `RecoveryFailure` additionally appends its note.
public func tandemErrorText(_ error: any Error) -> String {
    if let recovery = error as? RecoveryFailure {
        return baseText(recovery.underlying) + "\n\n" + recovery.note
    }
    return baseText(error as? any TandemError)
}

private func baseText(_ error: any TandemError?) -> String {
    guard let error else {
        return "Unexpected Tandem error. Please try again."
    }
    if case let .unknownID(_, open) = error as? DomainError {
        if let list = try? StoreEncoding.openList(open) {
            return error.message + "\n" + list
        }
    }
    return error.message
}
