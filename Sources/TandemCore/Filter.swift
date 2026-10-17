import Foundation

/// Which todos are in view: `open` (the default), `done`, or `all` (GLOSSARY: Filter).
///
/// One term for two surfaces: the Main window's segmented control and the MCP `list`
/// tool's argument. The raw wire values are lowercase `open` / `done` / `all`.
public enum Filter: String, Sendable, CaseIterable {
    case open
    case done
    case all

    /// `nil` (argument absent) means the default filter, `open`.
    /// Any non-`nil` value that is not one of the three cases is invalid.
    static func parse(raw: String?) -> Filter? {
        guard let raw else { return .open }
        return Filter(rawValue: raw)
    }

    /// Apply this filter to a stored-order list. Ordering is never changed —
    /// the stored order is the order (ADR 0001, superseded 2026-10-17: order is
    /// stored, not derived from `createdAt`).
    func apply(to todos: [Todo]) -> [Todo] {
        switch self {
        case .open: return todos.filter { !$0.done }
        case .done: return todos.filter { $0.done }
        case .all: return todos
        }
    }
}
