import Foundation

/// One lane row of the board: a cycle lane, cut into its Status columns.
///
/// A row is drawn even when it is empty, because an empty cell is a drop target — the
/// point of the row is where a ticket *can* go, not only where one is. The cells keep
/// **stored order** inside each column (the board never re-sorts; ADR 0017), so a card
/// sits where the human put it.
public struct BoardLane: Identifiable, Sendable, Equatable {
    /// The lane key: `nil` is the default (ungrouped) area.
    public let key: String?
    /// The tickets in each column. A Status with no tickets is absent, not empty.
    public let cells: [Status: [Ticket]]

    public init(key: String?, cells: [Status: [Ticket]]) {
        self.key = key
        self.cells = cells
    }

    public var id: String { key ?? "" }
    public var isDefault: Bool { key == nil }

    /// The tickets in one column, in stored order.
    public func tickets(_ status: Status) -> [Ticket] { cells[status] ?? [] }

    /// Every ticket in the row, whatever its column.
    public var count: Int { cells.values.reduce(0) { $0 + $1.count } }
}

/// The board's geometry, in the core: which rows to draw and what sits in each cell.
///
/// The rule is the list's rule, because a lane must mean the same thing in both faces:
/// the default area first, then each declared lane, then any lane that appears only on a
/// ticket (a hand edit) — so a row cannot be hidden by deleting a lane name from the lane
/// list. Rows are never dropped for being empty; that is what makes a cell a target.
///
/// Grouping is a pure function of the tickets it is handed. The caller decides what is in
/// view (the GUI passes a `TicketQuery`'s result), so "the board" and "the list" cannot
/// disagree about what a lane holds.
public enum TicketBoard {

    /// The board's rows: the tickets grouped by lane, each row split by Status.
    ///
    /// - Parameters:
    ///   - todos: the tickets in view, in stored order.
    ///   - lanes: the declared lane names, in render order.
    public static func rows(from todos: [Ticket], lanes: [String]) -> [BoardLane] {
        orderedKeys(declared: lanes, todos: todos).map { key in
            BoardLane(key: key, cells: Dictionary(grouping: todos.filter { $0.lane == key },
                                                  by: \.status))
        }
    }

    /// Every lane key in render order: the default area, the declared lanes, then any name
    /// only a ticket carries. Shared with the list so the two faces group identically.
    public static func orderedKeys(declared lanes: [String], todos: [Ticket]) -> [String?] {
        var keys: [String?] = [nil]
        for name in lanes where !keys.contains(name) { keys.append(name) }
        for todo in todos where todo.lane != nil {
            if !keys.contains(todo.lane) { keys.append(todo.lane) }
        }
        return keys
    }
}
