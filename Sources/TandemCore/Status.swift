import Foundation

/// Where a ticket is in the workflow (GLOSSARY: Status; ADR 0017).
///
/// Six stages in workflow order. The transitions between them are free-form: the value is
/// validated, never the move, so a ticket can go backwards as easily as forwards: the
/// board reflects what is true, not a process to argue with.
///
/// `on-hold` is the parked slot (ADR 0017's 2026-10-29 amendment): a blocker, a human
/// decision pending, a "not now". It is not a stage of refinement: a ticket with a clean
/// brief goes straight to `ready`, and a ticket still missing one stays `backlog`.
///
/// The wire values are the lowercase names, `in-progress` hyphenated. On disk the field
/// is a plain `String`, so a v1-era reader (which does not know the key) ignores it and a
/// hand editor reads and writes a word rather than a number.
public enum Status: String, Sendable, CaseIterable, Hashable {
    /// Nobody has looked at it yet — the default for a new ticket.
    case backlog
    /// Parked on purpose — a human decision waiting, a blocker, a "not now".
    case onHold = "on-hold"
    /// A brief with no question open: the next pass can pick it up.
    case ready
    case inProgress = "in-progress"
    /// The work is done and its PR is waiting for a reader (`In Review` on the board).
    case inReview = "in-review"
    case done

    /// Workflow order, `backlog` first: a board's column order, and the order an error
    /// names the values in.
    public static var workflow: [Status] { allCases }

    /// The word every face draws: the board's column header, the sheet's picker, the
    /// list's row. The wire value is lowercase (`in-progress`); this is the phrase.
    public var displayName: String {
        switch self {
        case .backlog: return "Backlog"
        case .onHold: return "On hold"
        case .ready: return "Ready"
        case .inProgress: return "In progress"
        case .inReview: return "In Review"
        case .done: return "Done"
        }
    }

    /// Every wire value the domain accepts, in `allCases` order.
    public static var wireValues: [String] { allCases.map(\.rawValue) }

    /// The domain boundary: the value, or the error that names what was given and lists
    /// the values that would have worked. A *file* or a *row* is a different edge; it
    /// reads through `stored(raw:)` instead, because nobody can retry a stored word.
    public static func parse(raw: String) throws -> Status {
        guard let status = Status(rawValue: raw) else {
            throw DomainError.invalidStatus(raw)
        }
        return status
    }

    /// The spelling a store may still hold for `on-hold`. It is history, not a stage:
    /// no face offers it and no write emits it. Schema v4 replaces the rows that carry it,
    /// and this alias is the read seam that keeps a row the migration has not reached yet
    /// in On hold; never in `backlog`, which is the data loss the gate exists to prevent.
    static let retiredOnHold = "refined"

    /// The lenient edge: the value, or `on-hold` for the retired spelling, or `backlog`
    /// for a word a later build invented; the same rule the file's decoder always kept,
    /// because nobody can retry a stored word.
    static func stored(raw: String) -> Status {
        if let status = Status(rawValue: raw) { return status }
        return raw == retiredOnHold ? .onHold : .backlog
    }
}
