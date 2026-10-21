import Foundation

/// Where a ticket is in the workflow (GLOSSARY: Status; ADR 0017).
///
/// Six fixed values in workflow order. The transitions between them are free-form: the
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
    /// The work is done and its PR is waiting for a reader (`In Review` on the board).
    case inReview = "in-review"
    case done

    /// Workflow order, `backlog` first: a board's column order, and the order an error
    /// names the values in.
    public static var workflow: [Status] { allCases }

    /// Every wire value, in workflow order.
    public static var wireValues: [String] { workflow.map(\.rawValue) }

    /// The domain boundary: the value, or the error that names what was given and lists
    /// the six that would have worked. A *file* is a different edge — it reads an
    /// unrecognised status as `backlog` instead (see `Ticket`'s decoder), because nobody
    /// can retry a file.
    public static func parse(raw: String) throws -> Status {
        guard let status = Status(rawValue: raw) else {
            throw DomainError.invalidStatus(raw)
        }
        return status
    }
}
