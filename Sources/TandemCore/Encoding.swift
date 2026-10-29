import Foundation

/// Result encoding for the agent-facing tool contract (spec: "Result encoding"). Every successful
/// result is JSON matching the store schema verbatim, plus one extension: a top-level
/// `note` string, present only when the call triggered corrupt-store recovery.
///
/// This lives in the core because it is the schema itself; the CLI just writes the
/// returned text to stdout.
public enum StoreEncoding {

    /// The `list` result: `{"tickets": [...]}`, with a top-level `note` when
    /// non-nil. The old `todos` wire key is gone (C8): one word for one entity.
    ///
    /// The rows are **summaries**, not whole tickets: a board is read many times
    /// more often than one ticket is read in full, and the description is the
    /// payload — a several-hundred-ticket board with descriptions was close to a
    /// megabyte of JSON per read, most of it never looked at. Every other field
    /// stays, and the description becomes `descriptionLength` (`0` when there is
    /// none, so the shape never varies by row); the one ticket's full body is
    /// `ticket show --id`'s answer, and `--text` still matches descriptions
    /// before the projection. `add`/`set`/`complete` results stay whole — the
    /// caller just wrote that ticket, and the schema-verbatim object is its
    /// receipt.
    public static func listResult(_ tickets: [Ticket], note: String?) throws -> String {
        let array = try tickets.map { try ticketSummaryObject($0) }
        var object: [String: Any] = ["tickets": array]
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
    /// with a top-level `note` when non-nil. The ticket is resolved before the
    /// read, so an unknown id is the domain failure and never reaches this
    /// encoding; an empty list is a ticket that exists and carries no comments.
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

    /// The `session show` result: the record, the **notes** it wrote, and the
    /// **tickets** it carried — the last two are the record's own `tickets` and
    /// the note paths derived from the capture files (the umbrella's
    /// "`session show` names the notes and the linked tickets").
    ///
    /// The notes are a derived list, never a stored one: consolidation moves
    /// the captures it consumes, so a stored path would be stale by design
    /// within a day.
    public static func sessionDetailResult(
        _ session: Session,
        notes: [String],
        note: String?
    ) throws -> String {
        var object = try sessionObject(session)
        object["notes"] = notes
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `sprint open` / `ticket` / `run` / `close` / `show` result: the
    /// sprint record, with a top-level `note` when non-nil — a second `open`
    /// or `close` for a record that already stands carries one.
    public static func sprintResult(_ sprint: Sprint, note: String?) throws -> String {
        var object = try sprintObject(sprint)
        if let note { object["note"] = note }
        return try serialize(object)
    }

    /// The `sprint list` result: `{"sprints": [...]}` — the records newest
    /// `openedAt` first — with a top-level `note` when non-nil. An empty
    /// result is a successful empty list, never an error.
    public static func sprintsResult(_ sprints: [Sprint], note: String?) throws -> String {
        var object: [String: Any] = ["sprints": try sprints.map { try sprintObject($0) }]
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
    /// `tickets` key like `listResult` —
    /// **summaries**, like the list: recovery needs the id and the title, and an
    /// error that dumped every description was the same megabyte read the list
    /// itself was.
    public static func openList(_ tickets: [Ticket]) throws -> String {
        let array = try tickets.map { try ticketSummaryObject($0) }
        return try serialize(["tickets": array])
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

    /// A single ticket as a **summary** object: the ticket's wire shape with the
    /// description — the one field that scales with the writing — replaced by
    /// its character count, `0` when there is no description. Everything else
    /// rides through `ticketObject` unchanged, so a summary row and a whole
    /// ticket cannot disagree about the fields they share. The length is derived,
    /// like `done`: never stored, only projected.
    private static func ticketSummaryObject(_ ticket: Ticket) throws -> [String: Any] {
        var object = try ticketObject(ticket)
        object["descriptionLength"] = (object["description"] as? String)?.count ?? 0
        object.removeValue(forKey: "description")
        // The block's reason is body text, like the description, so the summary keeps the
        // edge (`blocked`, `blockedBy`, `blocks`) and drops why (TAN-232). The body stays
        // out of a list; `ticket show` carries it.
        object.removeValue(forKey: "blockedReason")
        return object
    }

    /// A single ticket as a JSON-compatible object. Encodes through `Ticket`'s custom
    /// `Codable` so dates are ISO-8601 UTC and the id is a string, exactly as on disk.
    /// The wire key is `lane` — the v1 `section` spelling is gone from the encoding
    /// (C8); the store's *decoder* still reads both names forever (C4's rule).
    private static func ticketObject(_ ticket: Ticket) throws -> [String: Any] {
        let data = try JSONEncoder().encode(ticket)
        guard var object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode ticket for result")
        }
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
    /// key is present — `null` included — the shape the legacy `sessions.json`
    /// wrote and the CLI still emits.
    private static func sessionObject(_ session: Session) throws -> [String: Any] {
        let data = try JSONEncoder().encode(session)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode session for result")
        }
        return object
    }

    /// A single sprint record as a JSON-compatible object. Encodes through
    /// `Sprint`'s custom `Codable`, so the dates are ISO-8601 UTC and every
    /// key is present — `null` included — the shape the legacy `sprints.json`
    /// wrote and the CLI still emits.
    private static func sprintObject(_ sprint: Sprint) throws -> [String: Any] {
        let data = try JSONEncoder().encode(sprint)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("could not encode sprint for result")
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
