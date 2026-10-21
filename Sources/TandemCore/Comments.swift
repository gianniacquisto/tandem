import Foundation
import Darwin

/// A comment on a ticket (ADR 0017, amendment 2026-10-20): one line of the
/// ticket's sidecar log, `<home>/comments/<ticket-id>.jsonl`.
///
/// **The log is a sidecar, not a field.** The installed app rewrites
/// `todos.json` whole and drops keys it does not know — the same tax `origin`
/// pays — so an in-ticket `comments` array would be erased by the next human
/// edit until a build carrying the field is installed. A sidecar cannot be
/// erased by an older writer, and append-only suits two agents taking turns on
/// one ticket. The log is **append-only**: there is no edit and no delete, and
/// a deleted ticket's log stays on disk.
///
/// `at` is ISO-8601 UTC on the wire, exactly like the store's timestamps.
public struct Comment: Codable, Sendable, Equatable {
    /// The one who commented — an assignee or an agent's name, trimmed.
    /// `""` when unnamed (the `--author` flag was absent).
    public let author: String
    /// When it was appended, UTC.
    public let at: Date
    /// The body: markdown, trimmed at its edges; never empty.
    public let body: String

    public init(author: String, at: Date = .now, body: String) {
        self.author = author
        self.at = at
        self.body = body
    }

    // MARK: - Custom Codable (ISO-8601 UTC, strict on required fields)

    private enum CodingKeys: String, CodingKey {
        case author, at, body
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.author = try c.decode(String.self, forKey: .author)
        let atRaw = try c.decode(String.self, forKey: .at)
        guard let at = Timestamp.decode(atRaw) else {
            throw StoreIOError.io("a comment carries an unreadable timestamp \"\(atRaw)\"")
        }
        self.at = at
        self.body = try c.decode(String.self, forKey: .body)
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(author, forKey: .author)
        try c.encode(Timestamp.encode(at), forKey: .at)
        try c.encode(body, forKey: .body)
    }
}

extension Store {

    /// The comments directory, `<home>/comments/` — created on the first
    /// append, never by a read. Public so `doctor` can name it.
    public var commentsDirectory: URL {
        directory.appendingPathComponent("comments")
    }

    /// The sidecar file for one ticket: `comments/<ticket-id>.jsonl`, the id in
    /// canonical UUID form so a lowercase and an uppercase spelling reach the
    /// same log.
    func commentFileURL(for id: UUID) -> URL {
        commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    // MARK: - append

    /// `append(ticketID:author:body:)`: append one comment to the ticket's log,
    /// under the store's own flock. The ticket must exist — the check and the
    /// append are one locked act, so a ticket deleted in between cannot get a
    /// comment — and an unknown id throws `DomainError.unknownID` carrying the
    /// open list, like `complete`. The body is trimmed; an empty result is
    /// rejected (`DomainError.emptyCommentBody`) before anything is written.
    /// The author is trimmed; an empty result is the unnamed author `""`. The
    /// comments directory is created on first append.
    public func append(ticketID: String, author: String, body: String) throws -> OperationResult<Comment> {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw DomainError.emptyCommentBody }
        let who = author.trimmingCharacters(in: .whitespacesAndNewlines)
        let (comment, note) = try withLockedRead { contents, note in
            // The store's one rule for "which ticket is this id?" — the same
            // answer `complete` gives, from the same place.
            let id = try Self.requireTicket(ticketID, in: contents.todos).uuid
            // The stored form of the timestamp is whole seconds, so the
            // in-memory value is truncated to the same form: an appended
            // comment equals what `list` reads back.
            let at = Date(timeIntervalSince1970: Date.now.timeIntervalSince1970.rounded())
            let comment = Comment(author: who, at: at, body: text)
            try self.appendCommentLine(comment, for: id)
            return (comment, note)
        }
        return OperationResult(comment, note: note)
    }

    /// `list(ticketID:)`: the ticket's comments in append order, or an empty
    /// list — never an error — for a ticket with no log (including one that is
    /// unknown: the sidecar outlives the store's entries). Runs under the
    /// store's flock because an append is a physical `O_APPEND` write, not an
    /// atomic rename: an unlocked reader could observe a half-written line.
    /// A damaged line is a loud failure — the log is append-only, so a bad
    /// line is repair, not recovery.
    public func list(ticketID: String) throws -> OperationResult<[Comment]> {
        guard let id = UUID(uuidString: ticketID) else { return OperationResult<[Comment]>([]) }
        let url = commentFileURL(for: id)
        return try withLock {
            guard let data = try? Data(contentsOf: url) else {
                return OperationResult<[Comment]>([])
            }
            var comments: [Comment] = []
            for line in data.split(separator: 0x0A) where !line.isEmpty {
                guard let comment = try? JSONDecoder().decode(Comment.self, from: Data(line)) else {
                    throw StoreIOError.io(
                        "The comment log \(url.lastPathComponent) has a damaged line; repair the file and try again.")
                }
                comments.append(comment)
            }
            return OperationResult(comments)
        }
    }

    // MARK: - Sidecar mechanics (private)

    /// Append one encoded comment line to the ticket's log. Call only while
    /// holding the store's lock (the append is not atomic on its own; the
    /// flock is what makes the line land whole).
    private func appendCommentLine(_ comment: Comment, for id: UUID) throws {
        try FileManager.default.createDirectory(
            at: commentsDirectory, withIntermediateDirectories: true)
        let url = commentFileURL(for: id)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        var line = try encoder.encode(comment)
        line.append(0x0A)
        // O_APPEND, written whole and fsync'd: the line lands complete because
        // this runs under the store's flock (no other appender), and the fsync
        // keeps it on disk before the lock is released.
        let fd = open(url.path, O_WRONLY | O_CREAT | O_APPEND, 0o644)
        guard fd >= 0 else { throw StoreIOError.posix(op: "open comment log", code: errno) }
        defer { close(fd) }
        var remaining = line
        while !remaining.isEmpty {
            let n = remaining.withUnsafeBytes { buffer in
                guard let base = buffer.baseAddress else { return 0 }
                return Darwin.write(fd, base, remaining.count)
            }
            if n > 0 {
                remaining.removeFirst(n)
            } else if errno == EINTR {
                continue
            } else {
                throw StoreIOError.posix(op: "write comment", code: errno)
            }
        }
        guard Darwin.fsync(fd) == 0 else {
            throw StoreIOError.posix(op: "fsync comment", code: errno)
        }
    }
}
