import Foundation

/// Where a ticket is in the workflow (GLOSSARY: Status; ADR 0017).
///
/// Six stages in workflow order, plus one retired word kept readable. The transitions
/// between them are free-form: the value is validated, never the move, so a ticket can
/// go backwards as easily as forwards — the board reflects what is true, not a process to
/// argue with.
///
/// `refined` is the retired name of the on-hold slot (TAN-196): the store still holds
/// rows that spell it, so the domain still parses it and the decoder still reads it, but
/// the board draws no column for it and the stage list does not offer it. The writer that
/// migrates those rows and retires the word is TAN-195; this build writes nothing.
///
/// The wire values are the lowercase names, `in-progress` hyphenated. On disk the field
/// is a plain `String`, so a v1-era reader (which does not know the key) ignores it and a
/// hand editor reads and writes a word rather than a number.
public enum Status: String, Sendable, CaseIterable, Hashable {
    /// Nobody has looked at it yet — the default for a new ticket.
    case backlog
    /// Parked on purpose — a human decision waiting, a blocker, a "not now". The slot
    /// "To refine" used to hold.
    case onHold = "on-hold"
    /// The retired name of the on-hold slot: read and accepted, never drawn or offered.
    /// A ticket stored as `refined` reads back as `refined` in this build.
    case refined
    /// Refined, no question open — a brief the next pass can pick up.
    case ready
    case inProgress = "in-progress"
    /// The work is done and its PR is waiting for a reader (`In Review` on the board).
    case inReview = "in-review"
    case done

    /// Workflow order, `backlog` first: a board's column order, and the order an error
    /// names the values in. The retired `refined` is not a stage.
    public static var workflow: [Status] { allCases.filter { $0 != .refined } }

    /// Every wire value the domain accepts, in `allCases` order — the retired word
    /// included, because a caller may still send it and a store may still hold it.
    public static var wireValues: [String] { allCases.map(\.rawValue) }

    /// The domain boundary: the value, or the error that names what was given and lists
    /// the values that would have worked. A *file* is a different edge — it reads an
    /// unrecognised status as `backlog` instead (see `Ticket`'s decoder), because nobody
    /// can retry a file.
    public static func parse(raw: String) throws -> Status {
        guard let status = Status(rawValue: raw) else {
            throw DomainError.invalidStatus(raw)
        }
        return status
    }
}
