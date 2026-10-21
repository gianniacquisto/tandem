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
    /// `addFromNote` was called with an empty or whitespace-only selection —
    /// there is no title to file.
    case emptySelection
    /// `addFromNote` was given a note path that is not bikeshed-root-relative.
    case invalidNotePath(String)
    /// `append(ticketID:author:body:)` was called with an empty or whitespace-only body.
    case emptyCommentBody
    /// `start` was called with an empty or whitespace-only brief — there is no
    /// session to record.
    case emptySessionBrief
    /// `end`/`link`/`show` was called with a session id the sessions file does
    /// not carry. `known` is the ids it does carry, so the caller recovers in one
    /// turn.
    case unknownSession(id: String, known: [String])
    /// A `--description -` body was empty or whitespace-only: an empty pipe, not
    /// a deliberate clear (the store still takes `--description ""`).
    case emptyDescriptionBody
    /// `list` was called with a filter that is not `open`, `done`, or `all`.
    case invalidFilter(String)
    /// A Status value that is not one of the five (ADR 0017).
    case invalidStatus(String)
    /// `move` was given an index that is not a whole number.
    case invalidIndex(String)
    /// `--assignee self` or `--origin self` was used but the environment names no agent.
    case agentNameUnset
    /// A lane name was empty or whitespace-only.
    case emptyLaneName
    /// A lane name was already in use.
    case duplicateLane(String)
    /// A lane name was referenced but does not exist.
    case unknownLane(String)
    /// `complete`/`delete` was called with an id not present in the store. `open` is
    /// the full current open list (in the `list` shape) so the caller recovers in one turn.
    case unknownID(id: String, open: [Ticket])

    public var message: String {
        switch self {
        case .locked:
            return "The Tandem store is locked by another process — please retry."
        case .emptyTitle:
            return "Title is empty. Provide a non-empty ticket title and try again."
        case .emptySelection:
            return "Nothing to send. Select text in the note — or put the caret on a checkbox line — and try again."
        case .invalidNotePath(let bad):
            return "Note path \"\(bad)\" is not a bikeshed-relative path (e.g. projects/App.md)."
        case .emptyCommentBody:
            return "Comment body is empty. Provide a non-empty body and try again."
        case .emptySessionBrief:
            return "Session brief is empty. Provide a non-empty brief — what the session is for and what done looks like — and try again."
        case .unknownSession(let id, let known):
            guard !known.isEmpty else {
                return "Unknown session id \"\(id)\". No session is recorded yet."
            }
            return "Unknown session id \"\(id)\". Here are the recorded sessions to pick the right one from: \(known.joined(separator: ", "))."
        case .emptyDescriptionBody:
            return "Description body is empty. Provide a non-empty body on stdin — or pass --description \"\" to clear the description."
        case .invalidFilter(let bad):
            return "filter must be one of open, done, all (got \"\(bad)\")."
        case .invalidStatus(let bad):
            return "status must be one of \(Status.wireValues.joined(separator: ", ")) (got \"\(bad)\")."
        case .invalidIndex(let bad):
            return "index must be a whole number (got \"\(bad)\")."
        case .agentNameUnset:
            return "Cannot resolve self: set TANDEM_AGENT to this agent's name, or pass a name instead of self."
        case .emptyLaneName:
            return "Cycle lane name is empty. Provide a non-empty name and try again."
        case .duplicateLane(let name):
            return "A cycle lane named \"\(name)\" already exists."
        case .unknownLane(let name):
            return "Unknown cycle lane \"\(name)\". Create it first."
        case .unknownID(let id, _):
            return "Unknown ticket id \"\(id)\". Here is the current open list to pick the right one from:"
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
