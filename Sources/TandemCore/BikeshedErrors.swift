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
