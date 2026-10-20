import Foundation

/// Result encoding for the agent-facing tool contract (spec: "Result encoding"). Every successful
/// result is JSON matching the store schema verbatim, plus one extension: a top-level
/// `note` string, present only when the call triggered corrupt-store recovery.
///
/// This lives in the core because it is the schema itself; the CLI just writes the
/// returned text to stdout.
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

    /// The `archive` result: `{"archived": <n>}`, with a top-level `note` when non-nil.
    /// A bulk operation reports how much it did, not which tickets it touched — the
    /// caller's next `list` has the tickets, and the count keeps the reply one line.
    public static func archivedResult(_ count: Int, note: String?) throws -> String {
        var object: [String: Any] = ["archived": count]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The open list in the `list` shape — used to render the unknown-id error so the
    /// caller recovers in one turn (spec: tool contract, `complete`).
    public static func openList(_ todos: [Todo]) throws -> String {
        try serialize(["todos": try todoArray(todos)])
    }

    /// The `doctor` result: the setup report as JSON (`{"ok": …, "checks": […]}`),
    /// keys sorted like every other result. The shape is a contract — the tandem
    /// skill documents it — so it is pinned here beside the other result shapes.
    public static func doctorResult(_ report: SetupDiagnostics.Report) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(report)
        guard let text = String(data: data, encoding: .utf8) else {
            throw StoreIOError.io("could not serialize doctor report to UTF-8")
        }
        return text
    }

    /// The `note stats` result: the memory-health readout as one JSON object —
    /// `{"bikeshed": …, "projects": […], "totals": {…}}`, keys sorted like every
    /// other result. Both queue-age timestamps are present as `null` when a
    /// project has nothing pending or no curated notes, so the shape is fixed
    /// and a reader never branches on a missing key. The shape is a contract —
    /// the tandem skill documents it — so it is pinned here beside the others.
    public static func bikeshedStatsResult(_ stats: BikeshedStats) throws -> String {
        let projects: [Any] = stats.projects.map { project in
            let row: [String: Any] = [
                "project": project.project,
                "curatedNotes": project.curatedNotes,
                "rawPending": project.rawPending,
                "rawArchived": project.rawArchived,
                "oldestPending": project.oldestPending as Any? ?? NSNull(),
                "lastConsolidated": project.lastConsolidated as Any? ?? NSNull(),
            ]
            return row
        }
        return try serialize([
            "bikeshed": stats.bikeshed,
            "projects": projects,
            "totals": [
                "curatedNotes": stats.totals.curatedNotes,
                "rawPending": stats.totals.rawPending,
                "rawArchived": stats.totals.rawArchived,
            ],
        ])
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

/// Renders any Tandem failure to the short, actionable plain text the CLI returns
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
