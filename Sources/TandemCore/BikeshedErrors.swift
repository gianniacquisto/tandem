import Foundation

/// Contract-level (domain) bikeshed failures — the errors an agent or human
/// can act on. One sentence, present tense, names the bad value in quotes,
/// ends with the next step (the `DomainError` text convention).
public enum BikeshedError: Error, Sendable, Equatable {
    /// The bikeshed lock could not be acquired within the bounded retry window
    /// (~2 s).
    case locked
    /// `read` / `delete` / `rename` was called with a path not present in the
    /// bikeshed.
    case unknownNote(path: String)
    /// `create` (or `rename`) was called with a target that already exists in
    /// the bikeshed — an error, never an overwrite.
    case alreadyExists(path: String)
    /// The name violates the bikeshed naming rules (spec: bikeshed protocol — the
    /// app only creates names free of `#` and `|`; note names end in `.md`).
    case invalidName(name: String)
    /// `capture` was called with a `project` or `session` that is not one
    /// visible folder name (empty, `.`, `..`, a leading dot, or a `/`, `#`, `|`).
    case invalidCaptureName(name: String)
    /// `capture` was called with a `--kind` that is not one of the five kinds.
    case invalidCaptureKind(kind: String)
    /// A folder was moved into itself or into one of its own descendants — the
    /// move would make the tree cyclic.
    case moveCycle(path: String, into: String)

    public var message: String {
        switch self {
        case .locked:
            return "The bikeshed is locked by another process — please retry."
        case .unknownNote(let path):
            return "No note at \"\(path)\" in the bikeshed. List the bikeshed to see what is there."
        case .alreadyExists(let path):
            return "A file already exists at \"\(path)\". Choose a different name."
        case .invalidName(let name):
            return "Name \"\(name)\" is not allowed: note names cannot contain # or |, and must end in .md. Choose a different name."
        case .invalidCaptureName(let name):
            return "Capture name \"\(name)\" is not allowed: project and session must each be one folder name with no /, #, |, or leading dot. Choose a different name."
        case .invalidCaptureKind(let kind):
            let kinds = CaptureKind.allCases.map(\.rawValue).joined(separator: ", ")
            return "Unknown capture kind \"\(kind)\". Use one of: \(kinds)."
        case .moveCycle(let path, let into):
            let destination = into.isEmpty ? "the bikeshed root" : "\"\(into)\""
            return "Cannot move \"\(path)\" into \(destination): a folder cannot be moved into itself. Choose a different folder."
        }
    }
}

extension BikeshedError: TandemError {}

/// Low-level bikeshed I/O failures that propagate to the caller (the
/// `StoreIOError` shape, bikeshed wording).
public enum BikeshedIOError: Error, Sendable, Equatable {
    case posix(op: String, code: Int32)
    case io(String)

    public var message: String {
        switch self {
        case .posix(let op, let code):
            let detail = String(cString: strerror(code))
            return "Bikeshed I/O failed during \(op) (\(code): \(detail)). Try again."
        case .io(let detail):
            return "Bikeshed error: \(detail). Try again."
        }
    }
}

extension BikeshedIOError: TandemError {}
