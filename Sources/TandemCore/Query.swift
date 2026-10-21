import Foundation

/// One view over the tickets (GLOSSARY: Filter; ADR 0017).
///
/// The single definition of "which tickets are in view", shared by both faces: the CLI's
/// flags and the app's filter row are two syntaxes over this value, so "the ready
/// tickets" cannot mean one thing to a human and another to an agent. It is a pure
/// predicate — it never reorders (the stored order is the order) and it never writes.
public struct TodoQuery: Sendable, Equatable {
    /// Free text, matched case- and diacritic-insensitively against the **title and the
    /// description**. `nil` or whitespace-only matches everything.
    public var text: String?
    /// The Statuses in view; empty means every Status.
    public var statuses: Set<Status>
    /// Who the ticket is on.
    public var assignee: Assignee
    /// The cycle lane the ticket belongs to.
    public var lane: Lane
    /// Whether archived tickets are in view.
    public var archived: Archived

    public init(
        text: String? = nil,
        statuses: Set<Status> = [],
        assignee: Assignee = .any,
        lane: Lane = .any,
        archived: Archived = .excluded
    ) {
        self.text = text
        self.statuses = statuses
        self.assignee = assignee
        self.lane = lane
        self.archived = archived
    }

    /// The assignee axis: anyone, the unassigned tickets, or one named owner.
    public enum Assignee: Sendable, Equatable, Hashable {
        case any
        case unassigned
        case named(String)
    }

    /// The lane axis: any lane, the unnamed default area, or one named lane.
    public enum Lane: Sendable, Equatable, Hashable {
        case any
        case unnamed
        case named(String)
    }

    /// The archive axis is three-valued because both halves are real asks: "hide finished
    /// work" is the default, and "show me only the archive" is how it is undone.
    public enum Archived: String, Sendable, CaseIterable {
        case excluded
        case included
        case only
    }

    /// The reserved assignee: the person using the app, as opposed to an agent. Stored
    /// lowercase so it is one name (see `Store.normalizeAssignee`).
    public static let human = "human"

    /// The human's inbox: every open, non-archived ticket assigned to the person.
    ///
    /// This is the escalation surface — an agent that needs a decision files a ticket
    /// assigned to `human`, its description carrying the question — so the inbox is a
    /// query, not a second store and not a flag. Defined here, beside the other views, so
    /// "what is waiting for me" means one thing everywhere it is shown.
    public static var inbox: TodoQuery {
        TodoQuery(statuses: Set(Status.workflow.filter { $0 != .done }),
                  assignee: .named(human),
                  archived: .excluded)
    }
}

// MARK: - The predicate

extension TodoQuery {

    /// The tickets in view, in stored order. An empty result is a success, not a failure.
    public func apply(to todos: [Todo]) -> [Todo] {
        todos.filter(matches)
    }

    /// One ticket against every axis: all of them must hold (the axes are ANDed).
    func matches(_ todo: Todo) -> Bool {
        switch archived {
        case .excluded: if todo.archived { return false }
        case .only: if !todo.archived { return false }
        case .included: break
        }
        if !statuses.isEmpty, !statuses.contains(todo.status) { return false }
        switch assignee {
        case .any: break
        case .unassigned: if todo.assignee != nil { return false }
        case .named(let name): if todo.assignee != name { return false }
        }
        switch lane {
        case .any: break
        case .unnamed: if todo.lane != nil { return false }
        case .named(let name): if todo.lane != name { return false }
        }
        if let needle = normalizedText {
            let inTitle = TextMatch.contains(needle, in: todo.title)
            let inDescription = todo.description.map {
                TextMatch.contains(needle, in: $0)
            } ?? false
            if !inTitle && !inDescription { return false }
        }
        return true
    }

    /// The search text with its edges trimmed. An empty filter box means no text filter,
    /// not "match nothing".
    var normalizedText: String? {
        guard let text else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

// MARK: - The boundary (raw values in)

extension TodoQuery {

    /// The CLI boundary: raw flag values in, one query out — or a domain error naming
    /// what was wrong and what would have worked. The CLI parses nothing itself, so both
    /// faces reject the same value with the same words.
    ///
    /// `filter` is the shipped `open|done|all` contract kept as sugar over the axes; an
    /// explicitly named axis overrides the part of the sugar it names.
    ///
    /// **The CLI's filters never carry the archive.** `all` is every ticket *not*
    /// archived — the app's `All` still shows the archive, because `Filter` is one enum
    /// under two faces and the app has an Archived segment of its own. The archive is
    /// reached by asking for it: `archived` puts it in view. Named with a filter, the
    /// archive joins that filter — `all` plus the archive is every ticket. Named alone,
    /// the archive is the view: the archived tickets, statuses ignored, the app's
    /// Archived segment.
    public static func parse(
        filter rawFilter: String? = nil,
        text: String? = nil,
        status rawStatus: String? = nil,
        assignee rawAssignee: String? = nil,
        lane rawLane: String? = nil,
        archived includeArchived: Bool = false
    ) throws -> TodoQuery {
        guard let filter = Filter.parse(raw: rawFilter) else {
            throw DomainError.invalidFilter(rawFilter ?? "")
        }
        var query = filter.query
        // The CLI's filters never carry the archive; `Filter.all` still does, for the app.
        query.archived = .excluded
        if includeArchived {
            if rawFilter == nil {
                // The archive alone — the app's Archived segment: statuses ignored.
                query.statuses = []
                query.archived = .only
            } else {
                // Named with a filter, the archive joins that filter.
                query.archived = .included
            }
        }
        if let text { query.text = text }
        if let rawStatus { query.statuses = try parseStatuses(rawStatus) }
        if let rawAssignee { query.assignee = parseAssignee(rawAssignee) }
        if let rawLane { query.lane = parseLane(rawLane) }
        return query
    }

    /// `ready,in-progress` — comma-separated, because the flag grammar keeps only the last
    /// value of a repeated flag and would silently drop the earlier ones.
    static func parseStatuses(_ raw: String) throws -> Set<Status> {
        let parts = raw
            .split(separator: ",", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !parts.isEmpty else { throw DomainError.invalidStatus(raw) }
        var statuses: Set<Status> = []
        for part in parts {
            statuses.insert(try Status.parse(raw: part))
        }
        return statuses
    }

    /// The empty value is the empty field: `--assignee ""` asks for the unassigned.
    static func parseAssignee(_ raw: String) -> Assignee {
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? .unassigned : .named(name)
    }

    /// The empty value is the unnamed default area.
    static func parseLane(_ raw: String) -> Lane {
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? .unnamed : .named(name)
    }
}
