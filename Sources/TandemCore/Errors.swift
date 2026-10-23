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
    /// A session id was empty or whitespace-only. A record with no handle is a
    /// record nothing can find, and the core is the one place a caller cannot
    /// slip past — a guard in the CLI is a courtesy on top of this one.
    case emptySessionID
    /// `end`/`link`/`show` was called with a session id the sessions file does
    /// not carry. `known` is the ids it does carry, so the caller recovers in one
    /// turn.
    case unknownSession(id: String, known: [String])
    /// The sessions file (`sessions.json`) is corrupt and could not be moved
    /// aside, so it stays where it is and the session list cannot be read
    /// until it is gone. **The store is untouched**: `todos.json` is fine, and
    /// this failure is about the sessions file, not about the store — so it
    /// never wears the store's error vocabulary.
    case corruptSessionsFile(path: String, detail: String)
    /// A POSIX call on the sessions file's own storage failed: the lockfile
    /// could not be opened, or the file could not be written. `op` names the
    /// operation (`open lockfile`, `fsync`, `rename`, …) and `detail` says what
    /// the OS said. **The store is untouched**: `todos.json` is fine, and this
    /// failure is about the sessions file, not about the store — so it never
    /// wears the store's error vocabulary.
    case sessionsFileIO(path: String, op: String, detail: String)
    /// The sessions file's lock (`sessions.lock`) is held by another process,
    /// so the read-modify-write could not start. The next step is to retry.
    case sessionsFileLocked(path: String)
    /// The sprints file (`sprints.json`) is corrupt and could not be moved
    /// aside, so it stays where it is and the sprint list cannot be read
    /// until it is gone. **The store and the sessions file are untouched**:
    /// `todos.json` and `sessions.json` are fine, and this failure is about
    /// the sprints file, not about them — so it never wears their error
    /// vocabulary.
    case corruptSprintsFile(path: String, detail: String)
    /// A POSIX call on the sprints file's own storage failed: the lockfile
    /// could not be opened, or the file could not be written. `op` names the
    /// operation (`open lockfile`, `fsync`, `rename`, …) and `detail` says what
    /// the OS said. **The store and the sessions file are untouched**:
    /// `todos.json` and `sessions.json` are fine, and this failure is about
    /// the sprints file, not about them — so it never wears their error
    /// vocabulary.
    case sprintsFileIO(path: String, op: String, detail: String)
    /// The sprints file's lock (`sprints.lock`) is held by another process,
    /// so the read-modify-write could not start. The next step is to retry.
    case sprintsFileLocked(path: String)
    /// `run`/`close` was called with an empty or whitespace-only note body —
    /// an empty pipe, not a landing without words. (`close` that names *no*
    /// note at all is spelled `nil`, and closes with the stamp alone.)
    case emptySprintNote
    /// `ticket` was called with a ticket id the store does not carry — the id
    /// is not on the board. The batch takes only ids the board holds; a ticket
    /// deleted *after* the write is not this failure, it reads "not on the
    /// board".
    case unknownTicketID(String)
    /// `show`/`ticket`/`run`/`close` was called with a (lane, number) the
    /// sprints file does not carry. `known` is the numbers the lane *does*
    /// hold, so the caller recovers in one turn. The pair is the identity —
    /// there is no UUID to misspell.
    case unknownSprint(lane: String, number: Int, known: [Int])
    /// A sprint mode that is not `serial` or `parallel`. The board's `Status`
    /// is a different vocabulary, and so is its own failure.
    case invalidSprintMode(String)
    /// A session status that is not `running` or `ended`. The board's `Status`
    /// is a different vocabulary, and so is its own failure.
    case invalidSessionStatus(String)
    /// A session outcome that is not `succeeded`, `failed`, `interrupted` or
    /// `unknown`.
    case invalidSessionOutcome(String)
    /// No session id was given and the environment names none: the record
    /// would have no handle, and a record nothing can find is not a record.
    case sessionIDUnset
    /// A `--summary -` body was empty or whitespace-only: an empty pipe, not
    /// a session that ended without saying anything.
    case emptySessionSummary
    /// A usage flag (`--tokens-input`, `--cost`, …) was given a value that is
    /// not the kind of number it asks for, or a negative one — a count and a
    /// cost are never negative (spec `F68948C0`).
    case invalidUsageFlag(flag: String, value: String)
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
        case .emptySessionID:
            return "Session id is empty. Provide a non-empty session id — the handle a session is known by — and try again."
        case .unknownSession(let id, let known):
            guard !known.isEmpty else {
                return "Unknown session id \"\(id)\". No session is recorded yet."
            }
            return "Unknown session id \"\(id)\". Here are the recorded sessions to pick the right one from: \(known.joined(separator: ", "))."
        case .corruptSessionsFile(let path, let detail):
            return "The sessions file at \(path) is corrupt and could not be moved aside (\(detail)). Move it aside yourself and try again — the session list then starts empty."
        case .sessionsFileIO(let path, let op, let detail):
            return "The sessions file at \(path) failed during \(op) (\(detail)). Check that the home directory is writable and has free space, then try again."
        case .sessionsFileLocked(let path):
            return "The sessions file at \(path) is locked by another process — please retry."
        case .corruptSprintsFile(let path, let detail):
            return "The sprints file at \(path) is corrupt and could not be moved aside (\(detail)). Move it aside yourself and try again — the sprint list then starts empty."
        case .sprintsFileIO(let path, let op, let detail):
            return "The sprints file at \(path) failed during \(op) (\(detail)). Check that the home directory is writable and has free space, then try again."
        case .sprintsFileLocked(let path):
            return "The sprints file at \(path) is locked by another process — please retry."
        case .emptySprintNote:
            return "Sprint note is empty. Provide a non-empty note and try again."
        case .unknownTicketID(let id):
            return "Unknown ticket id \"\(id)\" — it is not on the board. Add the ticket to the board first, or check the id, and try again."
        case .unknownSprint(let lane, let number, let known):
            guard !known.isEmpty else {
                return "Unknown sprint \"Sprint \(number) — \(lane)\". No sprint is recorded in lane \"\(lane)\" yet."
            }
            return "Unknown sprint \"Sprint \(number) — \(lane)\". Here are the sprints lane \"\(lane)\" holds: \(known.map(String.init).joined(separator: ", "))."
        case .invalidSprintMode(let bad):
            return "mode must be one of \(SprintMode.wireValues.joined(separator: ", ")) (got \"\(bad)\")."
        case .invalidSessionStatus(let bad):
            return "session status must be one of \(SessionStatus.wireValues.joined(separator: ", ")) (got \"\(bad)\")."
        case .invalidSessionOutcome(let bad):
            return "outcome must be one of \(SessionOutcome.wireValues.joined(separator: ", ")) (got \"\(bad)\")."
        case .sessionIDUnset:
            return "Cannot resolve the session id: set OPENCODE_SESSION_ID to this session's id, or pass --session <id>."
        case .emptySessionSummary:
            return "Session summary is empty. Provide a non-empty summary on stdin — or omit --summary to end the session without one."
        case .invalidUsageFlag(let flag, let value):
            return "--\(flag) must be a non-negative number (got \"\(value)\")."
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

/// Internal decode-failure signal for the **sessions file** — the sessions file's
/// counterpart to `CorruptStore`: same shape, same rule. Thrown while parsing
/// `sessions.json`, caught by the file's own `peek`, and treated as corrupt → the
/// file is moved aside and the list restarts empty. Never surfaced to a caller
/// directly.
///
/// **It is not the store's type, and it is not the store's file.** The store's
/// `CorruptStore` carries the store's decode failures; this one carries the
/// sessions file's, so a reader of the code can tell which file was refused. The
/// store's type is not widened to cover both files.
enum CorruptSessions: Error, Sendable {
    /// A record's `status` is not a word this build knows. A session has two
    /// states and no safe default between them, so the file is invalid.
    case unknownStatus(String)
    /// A timestamp the file carries cannot be read.
    case invalidTimestamp(field: String, value: String)
    /// The envelope's `version` is not one this build reads.
    case unsupportedVersion(Int)
}

/// Internal decode-failure signal for the **sprints file** — the sprints file's
/// counterpart to `CorruptSessions`: same shape, same rule. Thrown while
/// parsing `sprints.json`, caught by the file's own `peek`, and treated as
/// corrupt → the file is moved aside and the list restarts empty. Never
/// surfaced to a caller directly.
///
/// **It is not the sessions file's type, and it is not the sessions file's
/// file.** The sessions file's `CorruptSessions` carries the sessions file's
/// decode failures; this one carries the sprints file's, so a reader of the
/// code can tell which file was refused. The sessions file's type is not
/// widened to cover both files.
enum CorruptSprints: Error, Sendable {
    /// A record's `status` is not a word this build knows. A sprint has two
    /// states and no safe default between them, so the file is invalid.
    case unknownStatus(String)
    /// A record's `mode` is not a word this build knows. A sprint has two
    /// modes and no safe default between them, so the file is invalid.
    case unknownMode(String)
    /// A timestamp the file carries cannot be read.
    case invalidTimestamp(field: String, value: String)
    /// The envelope's `version` is not one this build reads.
    case unsupportedVersion(Int)
}
