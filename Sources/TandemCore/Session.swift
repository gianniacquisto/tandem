import Foundation

/// A session: a record of one agent session's run, kept as an artifact of its
/// own (spec `FC770B8A`).
///
/// **A session is not a ticket.** The human's answer (`5C98453A` → **B**) makes
/// the session its own record, so it lives in its own file (`sessions.json`)
/// beside `todos.json` — never as a key inside it: the installed app rewrites
/// `todos.json` whole and drops the keys it does not know (ADR 0017's comments
/// amendment), so a `sessions` array there would be erased by the next human
/// edit until a build carrying the field is installed.
///
/// **The vocabulary is the record's, not the board's.** `running` and `ended`
/// are its two states; `succeeded`, `failed`, `interrupted` and `unknown` are
/// its outcomes. The board's `Status` (`backlog`…`done`) says nothing about a
/// session — `in-review` on a record is a category error — so these types do
/// not reference it.
///
/// **A record is a fact, not a log.** It is created once (`start`), stamped
/// once (`end`) and linked to (`link`); there is no edit and no delete.
public struct Session: Identifiable, Codable, Sendable, Equatable {
    /// The host's session id (`ses_…`) — the handle everywhere: the file, the
    /// CLI's `--session`, the app's row.
    public let id: String
    /// The codebase the session works in (`tandem`, `fleet`).
    public let project: String
    /// The agent's name, when the host told the session who it was. `nil` when
    /// nobody is on it.
    public let agent: String?
    /// What the session is for and what done looks like — written once, at
    /// `start`, and never changed.
    public let brief: String
    /// What it did — written once, by `end`. `nil` when the session ended
    /// without saying.
    public var summary: String?
    /// `running` or `ended` — never the board's `Status`.
    public var status: SessionStatus
    /// How the session ended. `nil` while it runs; `unknown` when it ended
    /// without saying how.
    public var outcome: SessionOutcome?
    /// The tickets the session carried, in link order.
    public var tickets: [UUID]
    /// When the session started, UTC.
    public let startedAt: Date
    /// When it ended, UTC. Stamped once, by `end`.
    public var endedAt: Date?

    public init(
        id: String,
        project: String,
        agent: String? = nil,
        brief: String,
        summary: String? = nil,
        status: SessionStatus = .running,
        outcome: SessionOutcome? = nil,
        tickets: [UUID] = [],
        startedAt: Date = .now,
        endedAt: Date? = nil
    ) {
        self.id = id
        self.project = project
        self.agent = agent
        self.brief = brief
        self.summary = summary
        self.status = status
        self.outcome = outcome
        self.tickets = tickets
        self.startedAt = startedAt
        self.endedAt = endedAt
    }

    /// Stamp the record as ended: the outcome, the end time, the summary and
    /// the status change together, **once**. A record that has already ended is
    /// left alone — the first outcome is the fact, and a second `end` cannot
    /// restate it.
    public mutating func end(outcome: SessionOutcome, summary: String?, at now: Date = .now) {
        guard status == .running else { return }
        self.outcome = outcome
        self.endedAt = now
        self.summary = summary
        self.status = .ended
    }

    /// Link one ticket the session carried. A ticket already linked is not
    /// linked twice, and the link order is the order the links were made in.
    public mutating func link(ticket: UUID) {
        guard !tickets.contains(ticket) else { return }
        tickets.append(ticket)
    }

    // MARK: - Custom Codable (ISO-8601 UTC dates, strict on required fields)

    /// The on-disk keys. Written explicitly — every key, `null` included — so
    /// the file shows the whole shape of a record rather than a shape that
    /// varies from one session to the next.
    private enum CodingKeys: String, CodingKey {
        case id, project, agent, brief, summary, status, outcome, tickets, startedAt, endedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // Required fields: a missing value, or one of the wrong type, throws —
        // which the sessions file treats as an invalid file (moved aside, list
        // restarts empty) — never silently discarded. The `status` is required
        // too, but an unrecognised *word* is its own case, and its rule is
        // written below.
        self.id = try c.decode(String.self, forKey: .id)
        self.project = try c.decode(String.self, forKey: .project)
        self.brief = try c.decode(String.self, forKey: .brief)
        self.tickets = try c.decode([UUID].self, forKey: .tickets)

        // **The status is strict, and that is a deliberate divergence from the
        // store's own rule.** `Ticket`'s decoder flattens a status it does not
        // recognise to `backlog` and keeps the file — with six workflow stages,
        // "least progressed" is a safe guess, and flattening one ticket's stage
        // is recoverable where calling the whole file corrupt is not (see
        // `Ticket`'s decoder). A session has two states and **no safe default
        // between them**: `running` would report a finished session as live, and
        // `ended` would report a live one as finished — and the state is what
        // `end` stamps against, so a wrong one is written back. An unrecognised
        // status therefore makes the file invalid: it is moved aside
        // (`sessions.json.corrupt-<ts>`) and the list restarts empty.
        //
        // **The price, on the record:** a later build that adds a `SessionStatus`
        // case makes an older build move the whole file aside. The store's rule
        // buys forward-compatibility that a session's *state* cannot.
        let storedStatus = try c.decode(String.self, forKey: .status)
        guard let status = SessionStatus(rawValue: storedStatus) else {
            throw CorruptSessions.unknownStatus(storedStatus)
        }
        self.status = status

        // Optional fields: absent reads as absent.
        self.agent = try c.decodeIfPresent(String.self, forKey: .agent)
        self.summary = try c.decodeIfPresent(String.self, forKey: .summary)
        if let storedOutcome = try c.decodeIfPresent(String.self, forKey: .outcome) {
            // An outcome this build does not recognise reads as `unknown` — the
            // honest answer for "how did it end" — rather than costing the whole
            // file: flattening one record's outcome is recoverable where calling
            // the file corrupt is not (the store's own treatment of a value it
            // cannot read). **The status above is not treated that way**, and
            // the reason is written there.
            self.outcome = SessionOutcome(rawValue: storedOutcome) ?? .unknown
        } else {
            self.outcome = nil
        }

        let startedRaw = try c.decode(String.self, forKey: .startedAt)
        guard let startedAt = Timestamp.decode(startedRaw) else {
            throw CorruptSessions.invalidTimestamp(field: "startedAt", value: startedRaw)
        }
        self.startedAt = startedAt

        if let endedRaw = try c.decodeIfPresent(String.self, forKey: .endedAt) {
            guard let endedAt = Timestamp.decode(endedRaw) else {
                throw CorruptSessions.invalidTimestamp(field: "endedAt", value: endedRaw)
            }
            self.endedAt = endedAt
        } else {
            self.endedAt = nil
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(project, forKey: .project)
        if let agent {
            try c.encode(agent, forKey: .agent)
        } else {
            try c.encodeNil(forKey: .agent)
        }
        try c.encode(brief, forKey: .brief)
        if let summary {
            try c.encode(summary, forKey: .summary)
        } else {
            try c.encodeNil(forKey: .summary)
        }
        try c.encode(status.rawValue, forKey: .status)
        if let outcome {
            try c.encode(outcome.rawValue, forKey: .outcome)
        } else {
            try c.encodeNil(forKey: .outcome)
        }
        try c.encode(tickets, forKey: .tickets)
        try c.encode(Timestamp.encode(startedAt), forKey: .startedAt)
        if let endedAt {
            try c.encode(Timestamp.encode(endedAt), forKey: .endedAt)
        } else {
            try c.encodeNil(forKey: .endedAt)
        }
    }
}

/// The two states a session record has (spec `FC770B8A`): `running` and
/// `ended`.
///
/// The board's `Status` is **not** reused here: `backlog`…`done` are the
/// board's vocabulary, and a session is not work on a board. `interrupted` is
/// an *outcome*, not a state.
///
/// **There is no third state to fall back on**, and that is why the file's
/// decoder treats an unrecognised status as invalid rather than defaulting it
/// (see `Session`'s decoder for the divergence from the store's own rule, and
/// the price it carries).
public enum SessionStatus: String, Sendable, CaseIterable, Hashable {
    /// The session is still running — it carries no outcome.
    case running
    /// The session ended. It always carries an outcome.
    case ended

    /// Every wire value, in state order.
    public static var wireValues: [String] { allCases.map(\.rawValue) }
}

/// How a session ended (spec `FC770B8A`).
///
/// `unknown` is the honest value: a session that ended without saying how. It
/// is also what a file whose outcome word this build does not know reads as —
/// flattening one record's outcome is recoverable where calling the whole file
/// corrupt is not (the store's own rule for a status it cannot read). The
/// record's **state** is not treated that way: `status` is required, and a
/// missing or unrecognised one invalidates the file.
public enum SessionOutcome: String, Sendable, CaseIterable, Hashable {
    case succeeded
    case failed
    case interrupted
    case unknown

    /// Every wire value.
    public static var wireValues: [String] { allCases.map(\.rawValue) }
}

/// The sessions file: `<home>/sessions.json`, beside `todos.json` (spec
/// `FC770B8A`).
///
/// **Its own file, not a key in `todos.json`.** The installed app rewrites
/// `todos.json` whole and drops the keys it does not know (ADR 0017's comments
/// amendment), so a `sessions` array there is erased by the next human edit
/// until a build carrying the field is installed. A file no older writer
/// touches is the only shape that survives.
///
/// The file mechanics are the store's own (ADR 0001 / ADR 0007):
/// - **Locking** — an exclusive `flock` on `<home>/sessions.lock` (created if
///   missing, never renamed), held across the whole read → modify →
///   write-temp → `rename` cycle. Readers take no lock: the atomic rename
///   guarantees a reader sees a complete old or new file.
/// - **Atomic writes** — same-directory temp `.sessions-<random>.tmp`, fsync,
///   `rename`; orphaned temps older than 60 s are swept under the lock.
/// - **Lenient read, strict write** — unknown keys are ignored; a missing or
///   mistyped required field invalidates the file.
/// - **Recovery** — an invalid file is renamed to `sessions.json.corrupt-<unix-ts>`
///   under the lock and the list restarts empty, with a note naming the file
///   that was kept.
///
/// **Its own errors, too.** The file's decode failures are its own internal
/// signal (`CorruptSessions`, the sessions file's counterpart to the store's
/// `CorruptStore`) — never surfaced, exactly like the store's. A file that
/// cannot be moved aside is a contract failure that names `sessions.json` and
/// what the human has to do about it (`DomainError.corruptSessionsFile`). A
/// lock that cannot be taken, and a write that cannot happen, are the same
/// kind of failure — `DomainError.sessionsFileLocked` and
/// `DomainError.sessionsFileIO`, both naming the file and the operation. The
/// store's `StoreIOError` is the store's: it is user-facing prose about
/// `todos.json`, and the sessions file is not `todos.json`.
///
/// **No rolling backup.** A record is recoverable from the transcript and the
/// saddlebag; `todos.json.bak` exists to protect hand-written work, and this
/// file holds none.
public struct Sessions: Sendable {
    /// The home directory the file lives in (the store's directory, so the app's
    /// existing watcher sees a session write for free). Created on first write.
    public let directory: URL
    /// The sessions file.
    let fileURL: URL
    /// The sidecar lockfile (created if missing, never renamed).
    let lockURL: URL
    /// The shared storage mechanics (ADR 0001): flock + atomic write + orphan
    /// sweep, identical to the store's.
    private let storage: LockedDirectory

    /// The default sessions file: the real `~/.tandem` from the shipped app,
    /// `~/.tandem-dev` from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A sessions file rooted at an explicit directory (used by tests with a
    /// temp dir). The file is `<directory>/sessions.json` and the lockfile its
    /// sibling `<directory>/sessions.lock`.
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("sessions.json")
        self.lockURL = directory.appendingPathComponent("sessions.lock")
        self.storage = LockedDirectory(
            lockURL: self.lockURL,
            tempPrefix: ".sessions-",
            cleanRoot: directory,
            cleanRecursively: false)
    }

    // MARK: - Operations

    /// `start(id:project:agent:brief:)`: record a session, once.
    ///
    /// The id is trimmed, and an empty result is refused
    /// (`DomainError.emptySessionID`) — a record with no handle is a record
    /// nothing can find. The brief is trimmed too, and an empty result is
    /// refused (`DomainError.emptySessionBrief`) before anything is written.
    /// The agent's name is trimmed, and an empty result is no agent (`nil`).
    ///
    /// A record with the same id already in the file is **returned unchanged**,
    /// with a note — whatever its status: a session that has ended is not
    /// restarted, and one that is running is not touched. The first `start` for
    /// an id creates the record and writes the file.
    public func start(
        id: String,
        project: String,
        agent: String? = nil,
        brief: String
    ) throws -> OperationResult<Session> {
        let handle = try Self.handle(id)
        let text = brief.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw DomainError.emptySessionBrief }
        let who = agent?.trimmingCharacters(in: .whitespacesAndNewlines)
        let name = (who?.isEmpty == false) ? who : nil
        let (session, note) = try mutate { sessions in
            if let existing = sessions.first(where: { $0.id == handle }) {
                return (existing, false, "session \(handle) already recorded")
            }
            let session = Session(
                id: handle,
                project: project,
                agent: name,
                brief: text,
                startedAt: Self.now())
            sessions.append(session)
            return (session, true, nil)
        }
        return OperationResult(session, note: note)
    }

    /// `end(id:outcome:summary:)`: stamp a session as ended, **once**.
    ///
    /// The outcome, the end time and the summary are written together; the
    /// summary is trimmed, and an empty result is no summary (`nil`). A record
    /// that has already ended is **returned unchanged** — the first outcome and
    /// the first summary stand, with a note — so a second `end` cannot restate
    /// how a session ended. An empty id is `DomainError.emptySessionID`; an id
    /// the file does not carry is `DomainError.unknownSession`.
    public func end(
        id: String,
        outcome: SessionOutcome,
        summary: String? = nil
    ) throws -> OperationResult<Session> {
        let handle = try Self.handle(id)
        let text = summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        let said = (text?.isEmpty == false) ? text : nil
        let (session, note) = try mutate { sessions in
            guard let index = sessions.firstIndex(where: { $0.id == handle }) else {
                throw DomainError.unknownSession(id: handle, known: sessions.map(\.id))
            }
            guard sessions[index].status == .running else {
                return (sessions[index], false, "session \(handle) already ended")
            }
            var record = sessions[index]
            record.end(outcome: outcome, summary: said, at: Self.now())
            sessions[index] = record
            return (record, true, nil)
        }
        return OperationResult(session, note: note)
    }

    /// `link(id:ticket:)`: record that a session carried a ticket.
    ///
    /// The ticket is appended once — a ticket already linked is not linked
    /// twice — and the link order is the order the links were made in. An empty
    /// id is `DomainError.emptySessionID`; an id the file does not carry is
    /// `DomainError.unknownSession`.
    public func link(id: String, ticket: UUID) throws -> OperationResult<Session> {
        let handle = try Self.handle(id)
        let (session, note) = try mutate { sessions in
            guard let index = sessions.firstIndex(where: { $0.id == handle }) else {
                throw DomainError.unknownSession(id: handle, known: sessions.map(\.id))
            }
            guard !sessions[index].tickets.contains(ticket) else {
                return (sessions[index], false, nil)
            }
            var record = sessions[index]
            record.link(ticket: ticket)
            sessions[index] = record
            return (record, true, nil)
        }
        return OperationResult(session, note: note)
    }

    /// `list(project:status:)`: the recorded sessions, **newest `startedAt`
    /// first**.
    ///
    /// A `nil` filter takes everything. Records that share a timestamp keep the
    /// order they were recorded in, so the list does not reshuffle between two
    /// reads. An empty result is a successful empty list, never an error.
    public func list(
        project: String? = nil,
        status: SessionStatus? = nil
    ) throws -> OperationResult<[Session]> {
        let (sessions, note) = try readAll()
        let filtered = sessions
            .filter { project == nil || $0.project == project }
            .filter { status == nil || $0.status == status }
        return OperationResult(Self.newestFirst(filtered), note: note)
    }

    /// `show(id:)`: one record, or `DomainError.unknownSession` — carrying the
    /// ids the file does hold, so the caller recovers in one turn. An empty id
    /// is `DomainError.emptySessionID`.
    public func show(id: String) throws -> OperationResult<Session> {
        let handle = try Self.handle(id)
        let (sessions, note) = try readAll()
        guard let session = sessions.first(where: { $0.id == handle }) else {
            throw unknownSession(handle, known: sessions.map(\.id), note: note)
        }
        return OperationResult(session, note: note)
    }

    // MARK: - File mechanics (private)

    /// The file's envelope. `version` follows the store's own rule: it moves
    /// only for a change an older reader would misread, never for an additive
    /// field. Version 1 is the only shape written or read.
    private struct FileEnvelope: Codable {
        var version: Int
        var sessions: [Session]
    }

    private enum Peek {
        case absent
        case valid([Session])
        case invalid
    }

    /// Take the sessions lock (translating the primitive's errors into the
    /// sessions file's own vocabulary) and run `body` under it, orphan temps
    /// cleaned first.
    ///
    /// **The store's vocabulary is not the file's.** A lock that cannot be
    /// taken is a failure of *this* file's lock (`sessions.lock`), and the
    /// human is told which file it was — the store (`todos.json`) is fine.
    private func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
        do {
            return try storage.withLock(body)
        } catch LockedDirectoryError.locked {
            throw DomainError.sessionsFileLocked(path: fileURL.path)
        } catch LockedDirectoryError.posix(let op, let code) {
            throw DomainError.sessionsFileIO(
                path: fileURL.path, op: op, detail: Self.posixDetail(code))
        }
    }

    /// A lock-free read of the current file state: "no file yet" (a valid empty
    /// list), a readable file, or a file that cannot be decoded.
    private func peek() -> Peek {
        let fm = FileManager.default
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: fileURL.path, isDirectory: &isDir), !isDir.boolValue else {
            return .absent
        }
        guard let data = try? Data(contentsOf: fileURL) else {
            return .invalid
        }
        do {
            return .valid(try Self.decode(data))
        } catch {
            return .invalid
        }
    }

    /// Read while holding the exclusive lock. Re-checks under the lock so a
    /// process that already recovered is not double-recovered; if the file is
    /// still invalid, it is moved aside and the list restarts empty. Returns
    /// the records and, when *this* call performed the move, the path it was
    /// kept at.
    private func readLocked() throws -> (sessions: [Session], backupPath: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let sessions):
            return (sessions, nil)
        case .invalid:
            let ts = Int(Date().timeIntervalSince1970)
            var backup = directory.appendingPathComponent("sessions.json.corrupt-\(ts)")
            var suffix = 1
            let fm = FileManager.default
            while fm.fileExists(atPath: backup.path) {
                backup = directory.appendingPathComponent("sessions.json.corrupt-\(ts)-\(suffix)")
                suffix += 1
            }
            do {
                try fm.moveItem(at: fileURL, to: backup)
            } catch {
                // Never silently discard: if the invalid file cannot be kept, fail
                // loudly — **in the sessions file's own words**. The store's
                // `StoreIOError` would report this as a store failure, and the
                // store is not what is broken: `todos.json` is fine, and this
                // file is the one the human has to look for.
                throw DomainError.corruptSessionsFile(
                    path: fileURL.path,
                    detail: (error as NSError).localizedDescription)
            }
            return ([], backup.path)
        }
    }

    /// A lock-free read (the `list` fast path). Takes the lock only to recover
    /// an invalid file.
    private func readAll() throws -> (sessions: [Session], note: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let sessions):
            return (sessions, nil)
        case .invalid:
            let (sessions, backup) = try withLock { try readLocked() }
            return (sessions, backup.map(Self.recoveryNote))
        }
    }

    /// A read-modify-write under the exclusive lock. `body` receives the current
    /// records and returns `(value, changed, note)`; when `changed` is false no
    /// write is performed — the file's bytes are untouched — but the lock is
    /// still taken and recovery is still checked. A note from `body` (a second
    /// `start`, a second `end`) and the recovery note cannot both happen: the
    /// recovery note wins when it does, because it says the caller's model of
    /// the file is void.
    private func mutate<T: Sendable>(
        _ body: (inout [Session]) throws -> (value: T, changed: Bool, note: String?)
    ) throws -> (value: T, note: String?) {
        try withLock {
            let (initial, backup) = try readLocked()
            let recovery = backup.map(Self.recoveryNote)
            var sessions = initial
            do {
                let (value, changed, note) = try body(&sessions)
                if changed {
                    try writeLocked(sessions)
                }
                return (value, recovery ?? note)
            } catch {
                if let recovery {
                    // The failure is the sessions file's, whatever it is: the
                    // store's `StoreIOError` would name a file that is not
                    // broken.
                    let underlying: any TandemError = error as? any TandemError
                        ?? DomainError.sessionsFileIO(
                            path: fileURL.path,
                            op: "update",
                            detail: (error as NSError).localizedDescription)
                    throw RecoveryFailure(underlying: underlying, note: recovery)
                }
                throw error
            }
        }
    }

    /// Write the file atomically — temp, fsync, `rename` — via the shared
    /// storage primitive, translating the primitive's failures into the
    /// sessions file's own vocabulary. Call only while holding the lock.
    ///
    /// **The failure is this file's, not the store's.** A write that cannot
    /// happen — a full disk, a read-only home, a `rename` that will not land —
    /// says so in the sessions file's words, naming the file the human has to
    /// look for. The store (`todos.json`) is a different file and it is fine.
    private func writeLocked(_ sessions: [Session]) throws {
        do {
            try storage.atomicWrite(try Self.encode(sessions), to: fileURL)
        } catch LockedDirectoryError.posix(let op, let code) {
            throw DomainError.sessionsFileIO(
                path: fileURL.path, op: op, detail: Self.posixDetail(code))
        } catch {
            // The primitive's POSIX failures are not the only ones: the write
            // can fail inside Foundation (a full disk, a read-only directory)
            // with an `NSError`. Both are this file's failure.
            throw DomainError.sessionsFileIO(
                path: fileURL.path, op: "write", detail: (error as NSError).localizedDescription)
        }
    }

    // MARK: - Schema (private)

    private static func encode(_ sessions: [Session]) throws -> Data {
        let envelope = FileEnvelope(version: 1, sessions: sessions)
        let encoder = JSONEncoder()
        // Pretty-printed, like the store's file: it is human-legible. Sorted
        // keys, so the same records always write the same bytes — Foundation's
        // encoder otherwise emits the keys in hash order, and the file would
        // reshuffle under a reader's eyes on every write.
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(envelope)
    }

    private static func decode(_ data: Data) throws -> [Session] {
        let envelope = try JSONDecoder().decode(FileEnvelope.self, from: data)
        guard envelope.version == 1 else {
            throw CorruptSessions.unsupportedVersion(envelope.version)
        }
        return envelope.sessions
    }

    // MARK: - Helpers (private)

    /// The session's handle, trimmed. An empty result is a domain failure: a
    /// record with no handle is a record nothing can find, and the core is the
    /// one place a later caller cannot slip past — a guard in the CLI is a
    /// courtesy on top of this one.
    private static func handle(_ id: String) throws -> String {
        let trimmed = id.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw DomainError.emptySessionID }
        return trimmed
    }

    /// The OS's own words for a POSIX failure: the code and `strerror`'s
    /// description, in the shape the store's own I/O errors use.
    private static func posixDetail(_ code: Int32) -> String {
        "\(code): \(String(cString: strerror(code)))"
    }

    /// The recovery note for an invalid sessions file. The store's recovery note
    /// (`Store.recoveryNote`) in this file's own nouns: the store's
    /// `todos.json` was not touched, so the note must not say the store
    /// restarted empty.
    private static func recoveryNote(backupPath: String) -> String {
        "Previous sessions file was corrupt and backed up to \(backupPath); the session list restarted empty."
    }

    /// The unknown-session failure, wrapped in a `RecoveryFailure` when this
    /// call also recovered the file — so the note is not lost.
    private func unknownSession(_ id: String, known: [String], note: String?) -> any Error {
        let error = DomainError.unknownSession(id: id, known: known)
        guard let note else { return error }
        return RecoveryFailure(underlying: error, note: note)
    }

    /// Newest `startedAt` first; records that share a timestamp keep the order
    /// they were recorded in.
    private static func newestFirst(_ sessions: [Session]) -> [Session] {
        sessions.enumerated()
            .sorted { a, b in
                if a.element.startedAt != b.element.startedAt {
                    return a.element.startedAt > b.element.startedAt
                }
                return a.offset < b.offset
            }
            .map(\.element)
    }

    /// Now, truncated to the stored form of a timestamp: the file keeps whole
    /// seconds, so the in-memory value is the same shape — a record equals what
    /// a later read gives back.
    private static func now() -> Date {
        Date(timeIntervalSince1970: Date.now.timeIntervalSince1970.rounded())
    }
}
