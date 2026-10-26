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
    /// The token counts the session spent, reported once by `end` — `nil`
    /// until reported, and never restated after. `nil` members are the counts
    /// the harness did not report.
    public var tokens: SessionTokens?
    /// What the session cost, in dollars — the harness's own number, reported
    /// once by `end`.
    public var cost: Double?
    /// The model that spent the tokens, reported once by `end`.
    public var model: String?

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
        endedAt: Date? = nil,
        tokens: SessionTokens? = nil,
        cost: Double? = nil,
        model: String? = nil
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
        self.tokens = tokens
        self.cost = cost
        self.model = model
    }

    /// Stamp the record as ended: the outcome, the end time, the summary — and
    /// the usage, when the harness reported one — change together, **once**. A
    /// record that has already ended is left alone — the first outcome is the
    /// fact, and a second `end` cannot restate it (nor its usage).
    public mutating func end(
        outcome: SessionOutcome,
        summary: String?,
        tokens: SessionTokens? = nil,
        cost: Double? = nil,
        model: String? = nil,
        at now: Date = .now
    ) {
        guard status == .running else { return }
        self.outcome = outcome
        self.endedAt = now
        self.summary = summary
        self.tokens = tokens
        self.cost = cost
        self.model = model
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
        case tokens, cost, model
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

        // The usage: absent reads as absent, and the three are written
        // together by `end` — a record that carries one carries what its
        // harness reported, nothing invented.
        self.tokens = try c.decodeIfPresent(SessionTokens.self, forKey: .tokens)
        self.cost = try c.decodeIfPresent(Double.self, forKey: .cost)
        self.model = try c.decodeIfPresent(String.self, forKey: .model)

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
        // The usage is the one deliberate divergence from the "every key, null
        // included" style above: **omitted when absent**, per the usage spec —
        // a record without usage is the shape every record written before the
        // field existed has, and a triple of nulls would dress absence up as
        // a reported zero.
        if let tokens {
            try c.encode(tokens, forKey: .tokens)
        }
        if let cost {
            try c.encode(cost, forKey: .cost)
        }
        if let model {
            try c.encode(model, forKey: .model)
        }
    }
}

/// The token counts one session spent, as the reporting harness holds them
/// (spec `F68948C0`): five integers, each the count its own flag names.
///
/// A member the harness did not report is `nil` — absent on the wire, never a
/// zero dressed up as a count — so a partial report stays partial, and the
/// record shows exactly what was reported.
public struct SessionTokens: Sendable, Equatable, Codable {
    /// Prompt tokens in.
    public var input: Int?
    /// Completion tokens out.
    public var output: Int?
    /// Reasoning tokens, where the model spends them separately.
    public var reasoning: Int?
    /// Cache reads.
    public var cacheRead: Int?
    /// Cache writes.
    public var cacheWrite: Int?

    public init(
        input: Int? = nil,
        output: Int? = nil,
        reasoning: Int? = nil,
        cacheRead: Int? = nil,
        cacheWrite: Int? = nil
    ) {
        self.input = input
        self.output = output
        self.reasoning = reasoning
        self.cacheRead = cacheRead
        self.cacheWrite = cacheWrite
    }

    private enum CodingKeys: String, CodingKey {
        case input, output, reasoning, cacheRead, cacheWrite
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.input = try c.decodeIfPresent(Int.self, forKey: .input)
        self.output = try c.decodeIfPresent(Int.self, forKey: .output)
        self.reasoning = try c.decodeIfPresent(Int.self, forKey: .reasoning)
        self.cacheRead = try c.decodeIfPresent(Int.self, forKey: .cacheRead)
        self.cacheWrite = try c.decodeIfPresent(Int.self, forKey: .cacheWrite)
    }

    public func encode(to encoder: Encoder) throws {
        // Absent counts are omitted, not nulled — the same rule the record's
        // own usage keys follow.
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let input { try c.encode(input, forKey: .input) }
        if let output { try c.encode(output, forKey: .output) }
        if let reasoning { try c.encode(reasoning, forKey: .reasoning) }
        if let cacheRead { try c.encode(cacheRead, forKey: .cacheRead) }
        if let cacheWrite { try c.encode(cacheWrite, forKey: .cacheWrite) }
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

/// The sessions store: the `sessions` table in `<home>/tandem.db` (spec
/// `FC770B8A`, re-backed by ADR 0031).
///
/// **Its own records, not a key in the ticket store.** The old file existed
/// because the installed app rewrote `todos.json` whole and dropped the keys it
/// did not know; the database has a table instead, and a session write never
/// touches the board's rows.
///
/// **One transaction per write.** `start`, `end` and `link` read the rows,
/// change the record and replace the table in one `BEGIN IMMEDIATE`, so two
/// writers cannot lose each other's records. The first write imports
/// `sessions.json` once — its records inserted, the file copied aside as
/// `<name>.pre-sqlite-<ts>` and left where it was — and records the import in
/// `store_imports` in the same transaction. Reads are lock-free WAL snapshots;
/// before the marker lands they still serve the legacy JSON.
///
/// An invalid legacy file is moved aside as `sessions.json.corrupt-<unix-ts>`
/// on the fallback read and the list restarts empty with a note (the
/// pre-migration recovery). A file that cannot be moved aside is
/// `DomainError.corruptSessionsFile`. Database failures wear the store's own
/// vocabulary — `DomainError.locked` on contention, `StoreIOError` otherwise.
public struct Sessions: Sendable {
    /// The home directory the file lives in (the store's directory, so the app's
    /// existing watcher sees a session write for free). Created on first write.
    public let directory: URL
    /// The legacy sessions file — the read-fallback's source and the import's
    /// source, never written again.
    let fileURL: URL

    /// The default sessions file: the real `~/.tandem` from the shipped app,
    /// `~/.tandem-dev` from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A sessions store rooted at an explicit directory (used by tests with a
    /// temp dir). The legacy file is `<directory>/sessions.json`; the records
    /// live in `<directory>/tandem.db`.
    ///
    /// `atomicWrite` is kept in the signature for source compatibility with the
    /// file-backed build; the database's transaction is the write now, so no
    /// caller-supplied write runs.
    public init(
        directory: URL,
        atomicWrite: (@Sendable (Data, URL) throws -> Void)? = nil
    ) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("sessions.json")
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
    ///
    /// `at` is the caller's own timestamp for the start — a record can stand
    /// for a session that started earlier (the one that migrated the pre-record
    /// tickets does), whole seconds like the file's own stamps; absent, it is
    /// now.
    public func start(
        id: String,
        project: String,
        agent: String? = nil,
        brief: String,
        at: Date? = nil
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
                startedAt: Self.when(at))
            sessions.append(session)
            return (session, true, nil)
        }
        return OperationResult(session, note: note)
    }

    /// `end(id:outcome:summary:tokens:cost:model:at:onStamp:)`: stamp a session
    /// as ended, **once**.
    ///
    /// The outcome, the end time, the summary and the usage are written
    /// together; the summary is trimmed, and an empty result is no summary
    /// (`nil`). The usage — the token counts, the cost and the model the
    /// harness reported — is written beside the outcome and is as immutable
    /// as it: a record that has already ended is **returned unchanged** — the
    /// first outcome and the first usage stand, with a note — so a second
    /// `end` cannot restate how a session ended nor what it spent. An empty id
    /// is `DomainError.emptySessionID`; an id the file does not carry is
    /// `DomainError.unknownSession`.
    ///
    /// `onStamp` is the one hook a caller can use to act on the stamp itself —
    /// the usage comment the CLI posts on the linked tickets (`spec
    /// F68948C0`). It runs **after** the write has landed, receives the ended
    /// record, and runs **only when this call stamped**: a second `end` — the
    /// rule that keeps the outcome immutable — is the same rule that keeps the
    /// comment from double-posting. A throwing hook fails the call after the
    /// stamp; the record is ended, and a retry is safe, because the retry
    /// stamps nothing and so calls nothing.
    ///
    /// `at` is the caller's own timestamp for the stamp — a record can stand
    /// for a session that ended earlier (the one that migrated the pre-record
    /// tickets does), whole seconds like the file's own stamps; absent, it is
    /// now.
    public func end(
        id: String,
        outcome: SessionOutcome,
        summary: String? = nil,
        tokens: SessionTokens? = nil,
        cost: Double? = nil,
        model: String? = nil,
        at: Date? = nil,
        onStamp: ((Session) throws -> Void)? = nil
    ) throws -> OperationResult<Session> {
        let handle = try Self.handle(id)
        let text = summary?.trimmingCharacters(in: .whitespacesAndNewlines)
        let said = (text?.isEmpty == false) ? text : nil
        var stamped = false
        let (session, note) = try mutate { sessions in
            guard let index = sessions.firstIndex(where: { $0.id == handle }) else {
                throw DomainError.unknownSession(id: handle, known: sessions.map(\.id))
            }
            guard sessions[index].status == .running else {
                return (sessions[index], false, "session \(handle) already ended")
            }
            var record = sessions[index]
            record.end(
                outcome: outcome, summary: said,
                tokens: tokens, cost: cost, model: model, at: Self.when(at))
            sessions[index] = record
            stamped = true
            return (record, true, nil)
        }
        if stamped, let onStamp {
            try onStamp(session)
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

    /// Move an invalid legacy file aside as `sessions.json.corrupt-<unix-ts>`,
    /// returning the path it was kept at. The file is evidence, never deleted.
    /// The failure, when the move cannot happen, is the sessions file's own —
    /// the store is not what is broken.
    private func recoverInvalidFile() throws -> String {
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
            throw DomainError.corruptSessionsFile(
                path: fileURL.path,
                detail: (error as NSError).localizedDescription)
        }
        return backup.path
    }

    /// A read: the database once the sessions domain has been imported, else the
    /// legacy JSON still beside it. An invalid JSON file is moved aside and the
    /// list reads empty with a note — the pre-migration behaviour, kept for a
    /// home whose first write has not landed yet.
    private func readAll() throws -> (sessions: [Session], note: String?) {
        let stored = try Store.translate {
            try StoreMigration.sidecarRead(
                in: directory, domain: StoreSchema.sessionsDomain
            ) { db in try Self.read(from: db) }
        }
        if let stored { return (stored, nil) }
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let sessions):
            return (sessions, nil)
        case .invalid:
            let backup = try recoverInvalidFile()
            return ([], Self.recoveryNote(backupPath: backup))
        }
    }

    /// A read-modify-write in one database transaction. The first write imports
    /// `sessions.json` (rows, the copy-aside and the marker commit together) and
    /// the body sees those rows; a changed write after that rolls the store's
    /// backup and replaces the table.
    private func mutate<T: Sendable>(
        _ body: (inout [Session]) throws -> (value: T, changed: Bool, note: String?)
    ) throws -> (value: T, note: String?) {
        try Store.translate {
            try StoreMigration.sidecarWrite(
                in: directory, domain: StoreSchema.sessionsDomain,
                importDomain: { db in try self.importSessions(into: db) },
                body: { db in
                    var sessions = try Self.read(from: db)
                    let (value, changed, note) = try body(&sessions)
                    return (value, changed, note, { try Self.write(sessions, to: db) })
                })
        }
    }

    /// The one-time import: `sessions.json` read once, its records inserted, the
    /// file copied aside as `<name>.pre-sqlite-<ts>` and left where it was. An
    /// invalid file is moved aside (the existing recovery) and the import starts
    /// from empty, with the note the caller surfaces.
    private func importSessions(
        into db: SQLiteDatabase
    ) throws -> (source: String?, note: String?) {
        switch peek() {
        case .absent:
            return (nil, nil)
        case .valid(let sessions):
            try StoreMigration.copyAside(fileURL)
            try Self.write(sessions, to: db)
            return ("sessions.json", nil)
        case .invalid:
            let backup = try recoverInvalidFile()
            try Self.write([], to: db)
            return ("sessions.json", Self.recoveryNote(backupPath: backup))
        }
    }

    // MARK: - Rows (private)

    private static func read(from db: SQLiteDatabase) throws -> [Session] {
        try db.rows("""
            SELECT id, project, agent, brief, status, outcome, summary,
                   startedAt, endedAt, tokensInput, tokensOutput, tokensReasoning,
                   tokensCacheRead, tokensCacheWrite, seconds, cost, model, tickets
            FROM sessions
            ORDER BY rowid
            """) { row -> Session in
            guard let startedAt = Timestamp.decode(row.text(7)) else {
                throw StoreIOError.io(
                    "a session row carries an unreadable startedAt \(row.text(7))")
            }
            let endedAt: Date?
            if let raw = row.textOrNil(8) {
                guard let date = Timestamp.decode(raw) else {
                    throw StoreIOError.io(
                        "a session row carries an unreadable endedAt \(raw)")
                }
                endedAt = date
            } else {
                endedAt = nil
            }
            guard let status = SessionStatus(rawValue: row.text(4)) else {
                throw StoreIOError.io(
                    "a session row carries an unknown status \(row.text(4))")
            }
            let outcome = row.textOrNil(5).map { SessionOutcome(rawValue: $0) ?? .unknown }
            let tickets: [UUID]
            if let raw = row.textOrNil(17) {
                do {
                    tickets = try JSONDecoder().decode([UUID].self, from: Data(raw.utf8))
                } catch {
                    throw StoreIOError.io("a session row carries an unreadable ticket list: \(error)")
                }
            } else {
                tickets = []
            }
            let tokens = SessionTokens(
                input: row.intOrNil(9),
                output: row.intOrNil(10),
                reasoning: row.intOrNil(11),
                cacheRead: row.intOrNil(12),
                cacheWrite: row.intOrNil(13))
            let hasTokens = tokens.input != nil || tokens.output != nil
                || tokens.reasoning != nil || tokens.cacheRead != nil
                || tokens.cacheWrite != nil
            return Session(
                id: row.text(0),
                project: row.text(1),
                agent: row.textOrNil(2),
                brief: row.text(3),
                summary: row.textOrNil(6),
                status: status,
                outcome: outcome,
                tickets: tickets,
                startedAt: startedAt,
                endedAt: endedAt,
                tokens: hasTokens ? tokens : nil,
                cost: row.realOrNil(15),
                model: row.textOrNil(16))
        }
    }

    private static func write(_ sessions: [Session], to db: SQLiteDatabase) throws {
        try db.run("DELETE FROM sessions")
        let encoder = JSONEncoder()
        for session in sessions {
            let tickets = String(
                decoding: try encoder.encode(session.tickets), as: UTF8.self)
            let seconds = session.endedAt.map {
                $0.timeIntervalSince(session.startedAt)
            }
            try db.run("""
                INSERT INTO sessions
                    (id, project, agent, brief, status, outcome, summary,
                     startedAt, endedAt, tokensInput, tokensOutput, tokensReasoning,
                     tokensCacheRead, tokensCacheWrite, seconds, cost, model, tickets)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """, [
                    .text(session.id),
                    .text(session.project),
                    .textOrNull(session.agent),
                    .text(session.brief),
                    .text(session.status.rawValue),
                    .textOrNull(session.outcome?.rawValue),
                    .textOrNull(session.summary),
                    .text(Timestamp.encode(session.startedAt)),
                    session.endedAt.map { .text(Timestamp.encode($0)) } ?? .null,
                    .intOrNull(session.tokens?.input),
                    .intOrNull(session.tokens?.output),
                    .intOrNull(session.tokens?.reasoning),
                    .intOrNull(session.tokens?.cacheRead),
                    .intOrNull(session.tokens?.cacheWrite),
                    seconds.map { .real($0) } ?? .null,
                    session.cost.map { .real($0) } ?? .null,
                    .textOrNull(session.model),
                    .text(tickets)
                ])
        }
    }

    // MARK: - Schema (private)

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

    /// The caller's timestamp for a stamp, or now when the caller carries
    /// none — either way truncated to the file's whole-second form, so the
    /// in-memory value is the same shape the file keeps: a record equals what
    /// a later read gives back.
    private static func when(_ date: Date?) -> Date {
        Date(timeIntervalSince1970: (date ?? .now).timeIntervalSince1970.rounded())
    }
}
