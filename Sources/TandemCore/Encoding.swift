import Foundation

/// Result encoding for the agent-facing tool contract (spec: "Result encoding"). Every successful
/// result is JSON matching the store schema verbatim, plus one extension: a top-level
/// `note` string, present only when the call triggered corrupt-store recovery.
///
/// This lives in the core because it is the schema itself; the CLI just writes the
/// returned text to stdout.
public enum StoreEncoding {

    /// The `list` result: `{"tickets": [...], "todos": [...]}`, with a top-level
    /// `note` when non-nil. The two keys carry **the same array** for one
    /// release: `ticket` is the new word and `todos` is the one agents parse
    /// today (D1). The release after next drops the old key (C8).
    public static func listResult(_ tickets: [Ticket], note: String?) throws -> String {
        let array = try ticketArray(tickets)
        var object: [String: Any] = ["tickets": array, "todos": array]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `add` / `complete` result: the ticket object, with a top-level `note` when non-nil.
    public static func ticketResult(_ ticket: Ticket, note: String?) throws -> String {
        var object = try ticketObject(ticket)
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `lane add` / `lane rename` result: `{"lanes": [...]}` — the lane list after
    /// the write, in render order. `delete` adds the count it moved, see below.
    public static func lanesResult(_ lanes: [String], note: String?) throws -> String {
        var object: [String: Any] = ["lanes": lanes]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `lane delete` result: `{"lanes": [...], "moved": <n>}` — the lanes left, and
    /// how many tickets moved to the default area (never deleted).
    public static func laneDeletionResult(_ deletion: LaneDeletion, note: String?) throws -> String {
        var object: [String: Any] = ["lanes": deletion.lanes, "moved": deletion.moved]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `ticket comment add` result: the comment object, with a top-level `note`
    /// when non-nil. Encoded through `Comment`'s custom `Codable`, so the `at`
    /// date is ISO-8601 UTC exactly as in the sidecar line.
    public static func commentResult(_ comment: Comment, note: String?) throws -> String {
        var object = try commentObject(comment)
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `ticket comment list` result: `{"comments": [...]}` in append order,
    /// with a top-level `note` when non-nil. An unknown ticket is a successful
    /// empty list, not an error — the log outlives the store's entries.
    public static func commentsResult(_ comments: [Comment], note: String?) throws -> String {
        var object: [String: Any] = ["comments": try comments.map { try commentObject($0) }]
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `session start` / `end` / `link` / `show` result: the session record,
    /// with a top-level `note` when non-nil — a second `start` or `end` for a
    /// record that already exists carries one.
    public static func sessionResult(_ session: Session, note: String?) throws -> String {
        var object = try sessionObject(session)
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `session list` result: `{"sessions": [...]}` — the records newest
    /// `startedAt` first — with a top-level `note` when non-nil. An empty
    /// result is a successful empty list, never an error.
    public static func sessionsResult(_ sessions: [Session], note: String?) throws -> String {
        var object: [String: Any] = ["sessions": try sessions.map { try sessionObject($0) }]
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
    /// caller recovers in one turn (spec: tool contract, `complete`). It carries the
    /// same two keys as `listResult`: `tickets` and `todos`, the same array.
    public static func openList(_ tickets: [Ticket]) throws -> String {
        let array = try ticketArray(tickets)
        return try serialize(["tickets": array, "todos": array])
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

    /// The `note links` result: the bikeshed's link-graph health as one JSON
    /// object — `{"broken": […], "orphans": […], "counts": {…}}`, keys sorted
    /// like every other result. `broken` rows carry the holding note, the
    /// target as written, and the line number; `orphans` is a list of note
    /// paths. The shape is a contract — the tandem skill documents it — so it
    /// is pinned here beside the others.
    public static func bikeshedHealthResult(_ health: BikeshedHealth) throws -> String {
        let broken: [Any] = health.broken.map { link in
            let row: [String: Any] = [
                "note": link.note,
                "link": link.link,
                "line": link.line,
            ]
            return row
        }
        return try serialize([
            "broken": broken,
            "orphans": health.orphans,
            "counts": [
                "notes": health.counts.notes,
                "links": health.counts.links,
                "broken": health.counts.broken,
                "orphans": health.counts.orphans,
            ],
        ])
    }

    // MARK: - Internals

    private static func ticketArray(_ tickets: [Ticket]) throws -> [Any] {
        try tickets.map { try ticketObject($0) }
    }

    /// A single ticket as a JSON-compatible object. Encodes through `Ticket`'s custom
    /// `Codable` so dates are ISO-8601 UTC and the id is a string, exactly as on disk.
    /// The ticket's lane rides beside the v1 wire key `section` — same value, both
    /// spellings — for one release (D1); C8 drops the old one.
    private static func ticketObject(_ ticket: Ticket) throws -> [String: Any] {
        let data = try JSONEncoder().encode(ticket)
        guard var object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode ticket for result")
        }
        object["lane"] = object["section"] ?? NSNull()
        return object
    }

    /// A single comment as a JSON-compatible object (see `ticketObject`).
    private static func commentObject(_ comment: Comment) throws -> [String: Any] {
        let data = try JSONEncoder().encode(comment)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode comment for result")
        }
        return object
    }

    /// A single session record as a JSON-compatible object. Encodes through
    /// `Session`'s custom `Codable`, so the dates are ISO-8601 UTC and every
    /// key is present — `null` included — exactly as in `sessions.json`.
    private static func sessionObject(_ session: Session) throws -> [String: Any] {
        let data = try JSONEncoder().encode(session)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode session for result")
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
