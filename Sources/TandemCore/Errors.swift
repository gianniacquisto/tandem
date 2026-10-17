import Foundation

/// A Tandem error that carries a short, actionable plain-text `message` suitable for
/// surfacing verbatim (MCP `isError` results, GUI alerts). Domain and I/O failures
/// both conform; protocol-level failures (malformed request, unknown method) are
/// separate and remain thrown MCP errors handled by the SDK.
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
    /// A section name was empty or whitespace-only.
    case emptySectionName
    /// A section name was already in use.
    case duplicateSection(String)
    /// A section name was referenced but does not exist.
    case unknownSection(String)
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
        case .emptySectionName:
            return "Section name is empty. Provide a non-empty section name and try again."
        case .duplicateSection(let name):
            return "A section named \"\(name)\" already exists."
        case .unknownSection(let name):
            return "Unknown section \"\(name)\". Create it first."
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
