import Foundation

/// Where a ticket is in the workflow (GLOSSARY: Status; ADR 0017).
///
/// Five fixed values in workflow order. The transitions between them are free-form: the
/// value is validated, never the move, so a ticket can go backwards as easily as forwards
/// — the board reflects what is true, not a process to argue with.
///
/// The wire values are the lowercase names, `in-progress` hyphenated. On disk the field
/// is a plain `String`, so a v1-era reader (which does not know the key) ignores it and a
/// hand editor reads and writes a word rather than a number.
public enum Status: String, Sendable, CaseIterable, Hashable {
    /// Nobody has looked at it yet — the default for a new ticket.
    case backlog
    case refined
    /// Refined and ready to be picked up.
    case ready
    case inProgress = "in-progress"
    case done

    /// Workflow order, `backlog` first: a board's column order, and the order an error
    /// names the values in.
    public static var workflow: [Status] { allCases }

    /// Every wire value, in workflow order.
    public static var wireValues: [String] { workflow.map(\.rawValue) }

    /// `nil` for a value that is not one of the five, so the *caller* decides what an
    /// unknown value means: an operation rejects it by name, while reading a file treats
    /// it as `backlog` (see `Todo`'s decoder — the two edges have different horizons).
    public static func parse(raw: String) -> Status? {
        Status(rawValue: raw)
    }
}
