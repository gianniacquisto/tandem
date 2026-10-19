import Foundation

/// A Tandem error that carries a short, actionable plain-text `message` suitable for
/// surfacing verbatim (CLI stderr, GUI alerts). Domain and I/O failures
/// both conform; there is no protocol layer above them to translate them.
public protocol TandemError: Error, Sendable {
    var message: String { get }
}

/// Contract-level (domain) failures. These are the errors an agent or human can act on:
/// name the failed value and the next step (spec: "Error convention").
public enum DomainError: Error, Sendable, Equatable {
    /// The store lock could not be acquired within the bounded retry window (~2 s).
    case locked
    /// `add` was called with an empty or whitespace-only title.
    case emptyTitle
    /// `list` was called with a filter that is not `open`, `done`, or `all`.
    case invalidFilter(String)
    /// A Status value that is not one of the five (ADR 0017).
    case invalidStatus(String)
    /// An archive value that is not `excluded`, `included`, or `only`.
    case invalidArchived(String)
    /// A lane name was empty or whitespace-only.
    case emptyLaneName
    /// A lane name was already in use.
    case duplicateLane(String)
    /// A lane name was referenced but does not exist.
    case unknownLane(String)
    /// `complete`/`delete` was called with an id not present in the store. `open` is
    /// the full current open list (in the `list` shape) so the caller recovers in one turn.
    case unknownID(id: String, open: [Todo])

    public var message: String {
        switch self {
        case .locked:
            return "The Tandem store is locked by another process — please retry."
        case .emptyTitle:
            return "Title is empty. Provide a non-empty todo title and try again."
        case .invalidFilter(let bad):
            return "filter must be one of open, done, all (got \"\(bad)\")."
        case .invalidStatus(let bad):
            return "status must be one of \(Status.wireValues.joined(separator: ", ")) (got \"\(bad)\")."
        case .invalidArchived(let bad):
            return "archived must be one of excluded, included, only (got \"\(bad)\")."
        case .emptyLaneName:
            return "Cycle lane name is empty. Provide a non-empty name and try again."
        case .duplicateLane(let name):
            return "A cycle lane named \"\(name)\" already exists."
        case .unknownLane(let name):
            return "Unknown cycle lane \"\(name)\". Create it first."
        case .unknownID(let id, _):
            return "Unknown todo id \"\(id)\". Here is the current open list to pick the right one from:"
        }
    }
}

extension DomainError: TandemError {}

/// Low-level store I/O failures that propagate to the caller (as opposed to decode
/// failures, which the Store converts into recovery).
public enum StoreIOError: Error, Sendable, Equatable {
    case posix(op: String, code: Int32)
    case io(String)

    public var message: String {
        switch self {
        case .posix(let op, let code):
            let detail = String(cString: strerror(code))
            return "Tandem store I/O failed during \(op) (\(code): \(detail)). Try again."
        case .io(let detail):
            return "Tandem store error: \(detail). Try again."
        }
    }
}

extension StoreIOError: TandemError {}

/// Internal decode-failure signal. Thrown while parsing the store file; the Store
/// catches it (in `peek`) and treats the file as corrupt → backup + restart. These are
/// never surfaced to a caller directly.
enum CorruptStore: Error, Sendable {
    case invalidTimestamp(field: String, value: String)
    case unsupportedVersion(Int)
}
