import Foundation
import Darwin

/// A comment on a ticket (ADR 0017, amendment 2026-10-20; re-backed in ADR
/// 0031's database): one row of the `comments` table in `<home>/tandem.db`.
///
/// **The log is append-only.** There is no edit and no delete, and a deleted
/// ticket's log stays. The legacy `comments/<ticket-id>.jsonl` logs are read as
/// the fallback until the first comment write imports them once, each log
/// copied aside as `<name>.pre-sqlite-<ts>` and left where it was.
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

    /// `append(ticketID:author:body:)`: append one comment to the ticket's log — a
    /// row in `tandem.db` now (ADR 0031). The ticket must exist, and the existence
    /// check and the insert are **one transaction**: a ticket deleted in between
    /// cannot get a comment, and the P1 seam's check-then-append window is gone
    /// with the flock. An unknown id throws `DomainError.unknownID` carrying the
    /// open list, like `complete`. The body is trimmed; an empty result is
    /// rejected (`DomainError.emptyCommentBody`) before anything is written. The
    /// author is trimmed; an empty result is the unnamed author `""`.
    ///
    /// The first append imports `comments/*.jsonl` once: each log is read whole,
    /// its comments inserted, and the file copied aside as
    /// `<name>.pre-sqlite-<ts>` — the log itself is never deleted. From then on
    /// the database is the log's truth.
    public func append(ticketID: String, author: String, body: String) throws -> OperationResult<Comment> {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw DomainError.emptyCommentBody }
        let who = author.trimmingCharacters(in: .whitespacesAndNewlines)
        let (comment, note) = try Self.translate {
            try StoreMigration.sidecarWrite(
                in: directory, domain: StoreSchema.commentsDomain,
                importDomain: { db in try self.importCommentLogs(into: db) },
                body: { db in
                    // The ticket domain's own import rides this transaction too: a
                    // comment never lands against a board the database has not
                    // imported, and a ticket deleted before the commit is not found.
                    _ = try StoreMigration.importIfNeeded(in: self.directory, into: db)
                    let contents = try StoreSchema.readContents(db)
                    // The store's one rule for "which ticket is this id?" — the
                    // same answer `complete` gives, from the same place.
                    let id = try Self.requireTicket(ticketID, in: contents).uuid
                    // The stored form of the timestamp is whole seconds, so the
                    // in-memory value is truncated to the same form: an appended
                    // comment equals what `list` reads back.
                    let at = Date(timeIntervalSince1970: Date.now.timeIntervalSince1970.rounded())
                    let comment = Comment(author: who, at: at, body: text)
                    return (comment, true, nil, { try Self.insert(comment, for: id, into: db) })
                })
        }
        return OperationResult(comment, note: note)
    }

    /// `list(ticketID:)`: the ticket's comments in append order, or an empty
    /// list for a ticket that has no log. The ticket must exist: the id is
    /// resolved the store's one way (`TAN-42` or a UUID, `Operations.resolveTicket`),
    /// so an id that names nothing is the same domain failure `append` and
    /// `complete` raise, and an empty log is the answer only for a ticket on
    /// the board that carries no comments. The database answers once the
    /// comments domain has been imported; until then, and on a home that has
    /// no database, the JSON log is read whole. A damaged line in the fallback
    /// is a loud failure: the log is append-only, so a bad line is repair, not
    /// recovery.
    public func list(ticketID: String) throws -> OperationResult<[Comment]> {
        let (contents, _) = try readAll()
        let id = try Self.requireTicket(ticketID, in: contents).uuid
        let stored = try Self.translate {
            try StoreMigration.sidecarRead(
                in: directory, domain: StoreSchema.commentsDomain
            ) { db in
                try db.rows(
                    "SELECT author, at, body FROM comments WHERE ticketId = ? ORDER BY rowid",
                    [.text(id.uuidString)]
                ) { row -> Comment in
                    let raw = row.text(1)
                    guard let at = Timestamp.decode(raw) else {
                        throw StoreIOError.io(
                            "A comment row carries an unreadable timestamp \"\(raw)\"")
                    }
                    return Comment(author: row.text(0), at: at, body: row.text(2))
                }
            }
        }
        if let stored { return OperationResult(stored) }
        return OperationResult(try readCommentLog(commentFileURL(for: id)))
    }

    // MARK: - Sidecar mechanics (private)

    /// The JSON log whole, in append order. The fallback and the importer share
    /// this read, so the damaged-line failure reads the same before and during
    /// the import.
    private func readCommentLog(_ url: URL) throws -> [Comment] {
        guard let data = try? Data(contentsOf: url) else { return [] }
        var comments: [Comment] = []
        for line in data.split(separator: 0x0A) where !line.isEmpty {
            guard let comment = try? JSONDecoder().decode(Comment.self, from: Data(line)) else {
                throw StoreIOError.io(
                    "The comment log \(url.lastPathComponent) has a damaged line; repair the file and try again.")
            }
            comments.append(comment)
        }
        return comments
    }

    /// Insert one comment row. The caller holds the write transaction.
    private static func insert(_ comment: Comment, for id: UUID, into db: SQLiteDatabase) throws {
        try db.run(
            "INSERT INTO comments (ticketId, author, at, body) VALUES (?, ?, ?, ?)",
            [.text(id.uuidString), .text(comment.author),
             .text(Timestamp.encode(comment.at)), .text(comment.body)])
    }

    /// The one-time import of `comments/*.jsonl`: every log read whole, inserted in
    /// file order, and copied aside. A log that cannot be decoded fails the write;
    /// the transaction rolls the rows back, and the files stay where they were.
    /// Returns the marker's source name — `comments` when any logs existed.
    private func importCommentLogs(into db: SQLiteDatabase) throws -> (source: String?, note: String?) {
        let fm = FileManager.default
        guard fm.fileExists(atPath: commentsDirectory.path) else { return (nil, nil) }
        let names: [String]
        do {
            names = try fm.contentsOfDirectory(atPath: commentsDirectory.path)
        } catch {
            throw StoreIOError.io(
                "could not read the comments directory at \(commentsDirectory.path): \(error)")
        }
        var imported = false
        for name in names.sorted() {
            guard name.hasSuffix(".jsonl") else { continue }
            let stem = String(name.dropLast(".jsonl".count))
            guard let id = UUID(uuidString: stem) else { continue }
            let url = commentsDirectory.appendingPathComponent(name)
            for comment in try readCommentLog(url) {
                try Self.insert(comment, for: id, into: db)
            }
            try StoreMigration.copyAside(url)
            imported = true
        }
        return (imported ? "comments" : nil, nil)
    }
}
