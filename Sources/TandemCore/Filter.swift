import Foundation

/// Which todos are in view: `open` (the default), `done`, or `all` (GLOSSARY: Filter).
///
/// One term for two surfaces: the Main window's segmented control and the CLI's
/// `todo list --filter` argument. The raw wire values are lowercase `open` / `done` / `all`.
///
/// This is **sugar over `TodoQuery`** (ADR 0017), not a second filter: the three shipped
/// cases are spelled once, here, in terms of the query's axes, so the segmented control
/// and a query an agent writes cannot drift apart.
///
/// One axis parts between the faces: the archive. `all` here is everything, the archive
/// included — the app's `All`. The CLI's `all` is the same set with the archive out, and
/// its `--archived` puts the archive in view (ADR 0017, 2026-10-21).
public enum Filter: String, Sendable, CaseIterable {
    case open
    case done
    case all

    /// `nil` (the CLI's `--filter` omitted) means the default filter, `open`.
    /// Any non-`nil` value that is not one of the three cases is invalid.
    static func parse(raw: String?) -> Filter? {
        guard let raw else { return .open }
        return Filter(rawValue: raw)
    }

    /// The query this filter stands for: `open` is every Status but done, `done` is done,
    /// and `all` is everything — archived tickets included. That is the app's `All`; the
    /// CLI's `all` is the same set with the archive out, and `--archived` is how an agent
    /// asks for the archive (ADR 0017, 2026-10-21).
    public var query: TodoQuery {
        switch self {
        case .open:
            return TodoQuery(
                statuses: Set(Status.workflow.filter { $0 != .done }),
                archived: .excluded)
        case .done:
            return TodoQuery(statuses: [.done], archived: .excluded)
        case .all:
            return TodoQuery(archived: .included)
        }
    }

    /// Apply this filter to a stored-order list. Ordering is never changed —
    /// the stored order is the order (ADR 0001, superseded 2026-10-17: order is
    /// stored, not derived from `createdAt`).
    public func apply(to todos: [Todo]) -> [Todo] {
        query.apply(to: todos)
    }
}
