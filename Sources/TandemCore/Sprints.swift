import Foundation

/// A sprint: a record of one sprint's run — its lane, batch, run log and close
/// (spec `4F746EFE`, R1).
///
/// **A sprint is not a ticket.** The human's answer (2026-10-23) makes the
/// sprint its own record — the session record's shape (ADR 0027) applied to
/// sprints — so it lives in its own file (`sprints.json`) beside `todos.json`,
/// never as a key inside it: the installed app rewrites `todos.json` whole and
/// drops the keys it does not know (ADR 0017's comments amendment), so a
/// `sprints` array there would be erased by the next human edit until a build
/// carrying the field is installed.
///
/// **The vocabulary is the record's, not the board's.** `open` and `closed`
/// are its two states. The board's `Status` (`backlog`…`done`) says nothing
/// about a sprint — a sprint is not work on a board — so these types do not
/// reference it.
///
/// **The pair (lane, number) is the identity.** There is no UUID on this
/// record: a sprint's human-facing name is `Sprint 3 — Tandem`, numbering is
/// per-lane, and the lane is in the name.
///
/// **A record is a fact, not a log.** It is created once (`open`), appended
/// to (`ticket`, `run`), and stamped once (`close`); there is no edit and no
/// delete. A sprint's close is prose about what landed, slipped and waits —
/// deliberately no outcome enum, which would flatten it.
public struct Sprint: Codable, Sendable, Hashable {
    /// The cycle lane the sprint ran in — the first half of the identity.
    public let lane: String
    /// The sprint's number **within its lane** — derived at `open` as one more
    /// than the highest number the lane's records hold, open and closed alike.
    /// The second half of the identity.
    public let number: Int
    /// The opening session's id (`ses_…`), when one was named at `open`. The
    /// dead-sprint check reads this, not a title. `nil` when the opening
    /// session did not name itself.
    public let session: String?
    /// How the sprint's batch was built: `serial` or `parallel` — the human's
    /// choice at open, stored.
    public let mode: SprintMode
    /// The tickets the sprint carried, in stored order — appended one per
    /// `ticket` call, deduped. An id in here may no longer be on the board (a
    /// ticket deleted after the write); that reads "not on the board", never
    /// an error.
    public var batch: [UUID]
    /// One entry per landing. **Dated by the core**: each entry is
    /// `"<ISO-8601 UTC> — <note>"`, the timestamp the core stamped when the
    /// entry was appended — the caller supplies the note only.
    public var runs: [String]
    /// What landed, slipped and waits — written once, by `close`. `nil` while
    /// the sprint is open (and for a close that named no text).
    public var close: String?
    /// `open` or `closed` — never the board's `Status`.
    public var status: SprintStatus
    /// When the sprint was opened, UTC.
    public let openedAt: Date
    /// When it was closed, UTC. Stamped once, by `close`.
    public var closedAt: Date?

    public init(
        lane: String,
        number: Int,
        session: String? = nil,
        mode: SprintMode = .serial,
        batch: [UUID] = [],
        runs: [String] = [],
        close: String? = nil,
        status: SprintStatus = .open,
        openedAt: Date = .now,
        closedAt: Date? = nil
    ) {
        self.lane = lane
        self.number = number
        self.session = session
        self.mode = mode
        self.batch = batch
        self.runs = runs
        self.close = close
        self.status = status
        self.openedAt = openedAt
        self.closedAt = closedAt
    }

    /// The sprint's human-facing identity: `Sprint 3 — Tandem`. The pair
    /// (lane, number) is the whole handle — there is no UUID on this record.
    public var name: String { "Sprint \(number) — \(lane)" }

    /// Add one ticket to the batch. A ticket already in the batch is not added
    /// twice, and the stored order is the order the additions were made in.
    public mutating func add(ticket: UUID) {
        guard !batch.contains(ticket) else { return }
        batch.append(ticket)
    }

    /// Append one run entry, dated here: the entry is
    /// `"<ISO-8601 UTC> — <note>"`, and the note is the caller's words.
    public mutating func run(_ note: String, at now: Date = .now) {
        runs.append("\(Timestamp.encode(now)) — \(note)")
    }

    /// Stamp the record as closed: the close text and the close time change
    /// the record **once**. A record that has already closed is left alone —
    /// the first close is the fact, and a second `close` cannot restate it.
    /// `nil` closes with no text (the stamp alone); a close *text*, when one
    /// is given, is the caller's words, trimmed upstream.
    public mutating func close(_ note: String?, at now: Date = .now) {
        guard status == .open else { return }
        self.close = note
        self.closedAt = now
        self.status = .closed
    }

    // MARK: - Custom Codable (ISO-8601 UTC dates, strict on required fields)

    /// The on-disk keys. Written explicitly — every key, `null` included — so
    /// the file shows the whole shape of a record rather than a shape that
    /// varies from one sprint to the next.
    private enum CodingKeys: String, CodingKey {
        case lane, number, session, mode, batch, runs, close, status, openedAt, closedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // Required fields: a missing value, or one of the wrong type, throws —
        // which the sprints file treats as an invalid file (moved aside, list
        // restarts empty) — never silently discarded. The `status` and `mode`
        // are required too, and an unrecognised *word* for either is its own
        // case, written below.
        self.lane = try c.decode(String.self, forKey: .lane)
        self.number = try c.decode(Int.self, forKey: .number)
        self.batch = try c.decode([UUID].self, forKey: .batch)
        self.runs = try c.decode([String].self, forKey: .runs)

        // **The mode is strict, and that is a deliberate divergence from the
        // store's own rule.** `Ticket`'s decoder flattens a status it does not
        // recognise to `backlog` and keeps the file — with six workflow stages,
        // "least progressed" is a safe guess. A sprint has two modes and **no
        // safe default between them**: `serial` would report a parallel build
        // as serial, and the other way round — and either lie is written back.
        // An unrecognised mode therefore makes the file invalid: it is moved
        // aside (`sprints.json.corrupt-<ts>`) and the list restarts empty.
        let storedMode = try c.decode(String.self, forKey: .mode)
        guard let mode = SprintMode(rawValue: storedMode) else {
            throw CorruptSprints.unknownMode(storedMode)
        }
        self.mode = mode

        // **The status is strict for the same reason** — two states, no safe
        // default between them, and the state is what `close` stamps against
        // (see `Session`'s decoder for the sessions file's identical rule).
        let storedStatus = try c.decode(String.self, forKey: .status)
        guard let status = SprintStatus(rawValue: storedStatus) else {
            throw CorruptSprints.unknownStatus(storedStatus)
        }
        self.status = status

        // Optional fields: absent reads as absent.
        self.session = try c.decodeIfPresent(String.self, forKey: .session)
        self.close = try c.decodeIfPresent(String.self, forKey: .close)

        let openedRaw = try c.decode(String.self, forKey: .openedAt)
        guard let openedAt = Timestamp.decode(openedRaw) else {
            throw CorruptSprints.invalidTimestamp(field: "openedAt", value: openedRaw)
        }
        self.openedAt = openedAt

        if let closedRaw = try c.decodeIfPresent(String.self, forKey: .closedAt) {
            guard let closedAt = Timestamp.decode(closedRaw) else {
                throw CorruptSprints.invalidTimestamp(field: "closedAt", value: closedRaw)
            }
            self.closedAt = closedAt
        } else {
            self.closedAt = nil
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(lane, forKey: .lane)
        try c.encode(number, forKey: .number)
        if let session {
            try c.encode(session, forKey: .session)
        } else {
            try c.encodeNil(forKey: .session)
        }
        try c.encode(mode.rawValue, forKey: .mode)
        try c.encode(batch, forKey: .batch)
        try c.encode(runs, forKey: .runs)
        if let close {
            try c.encode(close, forKey: .close)
        } else {
            try c.encodeNil(forKey: .close)
        }
        try c.encode(status.rawValue, forKey: .status)
        try c.encode(Timestamp.encode(openedAt), forKey: .openedAt)
        if let closedAt {
            try c.encode(Timestamp.encode(closedAt), forKey: .closedAt)
        } else {
            try c.encodeNil(forKey: .closedAt)
        }
    }
}

/// How a sprint's batch was built (spec `4F746EFE`, R1): `serial` or
/// `parallel` — the human's choice at open, stored.
///
/// The board's `Status` is **not** reused here, and neither is any other
/// vocabulary: a sprint is not work on a board.
///
/// **There is no third mode to fall back on**, and that is why the file's
/// decoder treats an unrecognised mode as invalid rather than defaulting it
/// (see `Sprint`'s decoder for the divergence from the store's own rule, and
/// the price it carries).
public enum SprintMode: String, Sendable, CaseIterable, Hashable {
    /// One ticket built at a time.
    case serial
    /// The batch built all at once.
    case parallel

    /// Every wire value, in declaration order.
    public static var wireValues: [String] { allCases.map(\.rawValue) }
}

/// The two states a sprint record has (spec `4F746EFE`, R1): `open` and
/// `closed`.
///
/// The board's `Status` is **not** reused here: `backlog`…`done` are the
/// board's vocabulary, and a sprint is not work on a board.
///
/// **There is no third state to fall back on**, and that is why the file's
/// decoder treats an unrecognised status as invalid rather than defaulting it
/// (see `Sprint`'s decoder).
public enum SprintStatus: String, Sendable, CaseIterable, Hashable {
    /// The sprint is running — it carries no close.
    case open
    /// The sprint closed. Its close text is whatever `close` wrote first.
    case closed

    /// Every wire value, in state order.
    public static var wireValues: [String] { allCases.map(\.rawValue) }
}

/// The sprints file: `<home>/sprints.json`, beside `todos.json` (spec
/// `4F746EFE`, R1).
///
/// **Its own file, not a key in `todos.json`.** The installed app rewrites
/// `todos.json` whole and drops the keys it does not know (ADR 0017's comments
/// amendment), so a `sprints` array there is erased by the next human edit
/// until a build carrying the field is installed. A file no older writer
/// touches is the only shape that survives — the same reasoning that put the
/// sessions file beside the store (ADR 0027).
///
/// The file mechanics are the sessions file's own (ADR 0001 / ADR 0007 /
/// ADR 0027):
/// - **Locking** — an exclusive `flock` on `<home>/sprints.lock` (created if
///   missing, never renamed), held across the whole read → modify →
///   write-temp → `rename` cycle. Readers take no lock: the atomic rename
///   guarantees a reader sees a complete old or new file.
/// - **Atomic writes** — same-directory temp `.sprints-<random>.tmp`, fsync,
///   `rename`; orphaned temps older than 60 s are swept under the lock.
/// - **Lenient read, strict write** — unknown keys are ignored; a missing or
///   mistyped required field invalidates the file.
/// - **Recovery** — an invalid file is renamed to `sprints.json.corrupt-<unix-ts>`
///   under the lock and the list restarts empty, with a note naming *this*
///   file — never the store's `todos.json`.
///
/// **Its own errors, too.** The file's decode failures are its own internal
/// signal (`CorruptSprints`, the sprints file's counterpart to the sessions
/// file's `CorruptSessions`) — never surfaced, exactly like the store's and
/// the sessions file's. A file that cannot be moved aside is a contract
/// failure that names `sprints.json` and what the human has to do about it
/// (`DomainError.corruptSprintsFile`). A lock that cannot be taken, and a
/// write that cannot happen, are the same kind of failure —
/// `DomainError.sprintsFileLocked` and `DomainError.sprintsFileIO`, both
/// naming the file and the operation. The sessions file's vocabulary is the
/// sessions file's: `sessions.json` is a different file, and it is fine.
///
/// **No rolling backup.** A record is recoverable from the tickets it names
/// and the transcript; `todos.json.bak` exists to protect hand-written work,
/// and this file holds none.
public struct Sprints: Sendable {
    /// The home directory the file lives in (the store's directory, so the
    /// app's existing watcher sees a sprint write for free). Created on first
    /// write.
    public let directory: URL
    /// The sprints file.
    let fileURL: URL
    /// The sidecar lockfile (created if missing, never renamed).
    let lockURL: URL
    /// The shared storage mechanics (ADR 0001): flock + atomic write + orphan
    /// sweep, identical to the store's.
    private let storage: LockedDirectory

    /// The default sprints file: the real `~/.tandem` from the shipped app,
    /// `~/.tandem-dev` from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A sprints file rooted at an explicit directory (used by tests with a
    /// temp dir). The file is `<directory>/sprints.json` and the lockfile its
    /// sibling `<directory>/sprints.lock`.
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("sprints.json")
        self.lockURL = directory.appendingPathComponent("sprints.lock")
        self.storage = LockedDirectory(
            lockURL: self.lockURL,
            tempPrefix: ".sprints-",
            cleanRoot: directory,
            cleanRecursively: false)
    }

    // MARK: - Operations

    /// `open(lane:mode:session:)`: record a sprint, once per lane.
    ///
    /// The lane is trimmed, and an empty result is refused
    /// (`DomainError.emptyLaneName` — it *is* a cycle lane name). The
    /// opening session's id is trimmed, and an empty result is no session
    /// (`nil`) — the core never reads the environment; naming the opening
    /// session is the caller's job.
    ///
    /// **A lane that already has an open sprint does not get a second one.**
    /// The open record is returned unchanged, with a note — a re-run of the
    /// open step must not fork the sprint. A lane whose sprints are all
    /// closed opens a new one, numbered one more than the highest number the
    /// lane's records hold — open and closed alike, so a closed sprint is
    /// never renumbered over.
    public func open(
        lane rawLane: String,
        mode: SprintMode = .serial,
        session rawSession: String? = nil
    ) throws -> OperationResult<Sprint> {
        let laneName = try Self.laneName(rawLane)
        let who = rawSession?.trimmingCharacters(in: .whitespacesAndNewlines)
        let session = (who?.isEmpty == false) ? who : nil
        let (sprint, note) = try mutate { sprints in
            if let existing = sprints.first(where: { $0.lane == laneName && $0.status == .open }) {
                return (existing, false, "\(existing.name) is already open")
            }
            let highest = sprints
                .filter { $0.lane == laneName }
                .map(\.number)
                .max() ?? 0
            let sprint = Sprint(
                lane: laneName,
                number: highest + 1,
                session: session,
                mode: mode,
                openedAt: Self.now())
            sprints.append(sprint)
            return (sprint, true, nil)
        }
        return OperationResult(sprint, note: note)
    }

    /// `ticket(lane:number:ticketID:on:)`: add one ticket to a sprint's batch.
    ///
    /// The ticket is appended once — a ticket already in the batch is not
    /// added twice — and the stored order is the order the additions were
    /// made in. **An id the store does not carry is refused**
    /// (`DomainError.unknownTicketID`), checked with one lock-free read of
    /// the store — the whole board, archive included, because the rule is
    /// about what the *store* carries, not what is open this minute. A ticket
    /// deleted *after* the write is not an error: the batch keeps the id, and
    /// a reader that cannot find it on the board reads "not on the board".
    ///
    /// A (lane, number) the file does not carry is
    /// `DomainError.unknownSprint` — checked before the board is asked, so a
    /// mistyped sprint handle is reported as one.
    public func ticket(
        lane rawLane: String,
        number: Int,
        ticketID: UUID,
        on store: Store
    ) throws -> OperationResult<Sprint> {
        let laneName = try Self.laneName(rawLane)
        // The handle first: an unknown (lane, number) is that failure, whatever
        // the ticket id goes on to say. A lock-free read — the `mutate` below
        // re-reads under the lock, so this is the error's shape, not the truth.
        let (known, knownNote) = try readAll()
        guard known.contains(where: { $0.lane == laneName && $0.number == number }) else {
            throw Self.unknownSprint(laneName, number, in: known, note: knownNote)
        }
        // One lock-free read: the store carries the id or it does not. A
        // corrupt store recovers here the store's own way; this file is not
        // touched until the check has passed.
        let board = try store.list(.all).value
        guard board.contains(where: { $0.id == ticketID }) else {
            throw DomainError.unknownTicketID(ticketID.uuidString)
        }
        let (sprint, note) = try mutate { sprints in
            guard let index = Self.index(of: laneName, number: number, in: sprints) else {
                throw Self.unknownSprint(laneName, number, in: sprints)
            }
            guard !sprints[index].batch.contains(ticketID) else {
                return (sprints[index], false, nil)
            }
            var record = sprints[index]
            record.add(ticket: ticketID)
            sprints[index] = record
            return (record, true, nil)
        }
        return OperationResult(sprint, note: note)
    }

    /// `run(lane:number:note:)`: append one dated entry to the run log.
    ///
    /// The note is trimmed, and an empty result is refused
    /// (`DomainError.emptySprintNote`) before anything is written — an empty
    /// pipe, not a landing without words. The date is the core's: the entry
    /// is `"<ISO-8601 UTC> — <note>"`, stamped as it is appended.
    ///
    /// A (lane, number) the file does not carry is
    /// `DomainError.unknownSprint`.
    public func run(
        lane rawLane: String,
        number: Int,
        note rawNote: String
    ) throws -> OperationResult<Sprint> {
        let laneName = try Self.laneName(rawLane)
        let text = rawNote.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw DomainError.emptySprintNote }
        let (sprint, note) = try mutate { sprints in
            guard let index = Self.index(of: laneName, number: number, in: sprints) else {
                throw Self.unknownSprint(laneName, number, in: sprints)
            }
            var record = sprints[index]
            record.run(text, at: Self.now())
            sprints[index] = record
            return (record, true, nil)
        }
        return OperationResult(sprint, note: note)
    }

    /// `close(lane:number:note:)`: stamp a sprint as closed, **once**.
    ///
    /// The close text and the close time are written together; the text is
    /// trimmed, and a text that is given but empty is refused
    /// (`DomainError.emptySprintNote`) — a close that says nothing is spelled
    /// `nil`, not `""`. A record that has already closed is **returned
    /// unchanged** — the first text and the first stamp stand, with a note —
    /// so a second `close` cannot restate how a sprint ended.
    ///
    /// A (lane, number) the file does not carry is
    /// `DomainError.unknownSprint`.
    public func close(
        lane rawLane: String,
        number: Int,
        note rawNote: String? = nil
    ) throws -> OperationResult<Sprint> {
        let laneName = try Self.laneName(rawLane)
        let text = rawNote?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let text, text.isEmpty { throw DomainError.emptySprintNote }
        let (sprint, note) = try mutate { sprints in
            guard let index = Self.index(of: laneName, number: number, in: sprints) else {
                throw Self.unknownSprint(laneName, number, in: sprints)
            }
            guard sprints[index].status == .open else {
                return (sprints[index], false, "\(sprints[index].name) is already closed")
            }
            var record = sprints[index]
            record.close(text, at: Self.now())
            sprints[index] = record
            return (record, true, nil)
        }
        return OperationResult(sprint, note: note)
    }

    /// `list(lane:status:)`: the recorded sprints, **newest `openedAt`
    /// first**.
    ///
    /// A `nil` filter takes everything. Records that share a timestamp keep
    /// the order they were recorded in, so the list does not reshuffle between
    /// two reads. An empty result is a successful empty list, never an error.
    public func list(
        lane rawLane: String? = nil,
        status: SprintStatus? = nil
    ) throws -> OperationResult<[Sprint]> {
        let laneName = try rawLane.map(Self.laneName)
        let (sprints, note) = try readAll()
        let filtered = sprints
            .filter { laneName == nil || $0.lane == laneName }
            .filter { status == nil || $0.status == status }
        return OperationResult(Self.newestFirst(filtered), note: note)
    }

    /// `show(lane:number:)`: one record by the (lane, number) handle — the
    /// pair is the identity — or `DomainError.unknownSprint`, carrying the
    /// numbers the lane does hold, so the caller recovers in one turn. An
    /// empty lane is `DomainError.emptyLaneName`.
    public func show(
        lane rawLane: String,
        number: Int
    ) throws -> OperationResult<Sprint> {
        let laneName = try Self.laneName(rawLane)
        let (sprints, note) = try readAll()
        guard let sprint = sprints.first(where: { $0.lane == laneName && $0.number == number }) else {
            throw Self.unknownSprint(laneName, number, in: sprints, note: note)
        }
        return OperationResult(sprint, note: note)
    }

    // MARK: - File mechanics (private)

    /// The file's envelope. `version` follows the store's own rule: it moves
    /// only for a change an older reader would misread, never for an additive
    /// field. Version 1 is the only shape written or read.
    private struct FileEnvelope: Codable {
        var version: Int
        var sprints: [Sprint]
    }

    private enum Peek {
        case absent
        case valid([Sprint])
        case invalid
    }

    /// Take the sprints lock (translating the primitive's errors into the
    /// sprints file's own vocabulary) and run `body` under it, orphan temps
    /// cleaned first.
    ///
    /// **The sessions file's vocabulary is not this file's.** A lock that
    /// cannot be taken is a failure of *this* file's lock (`sprints.lock`),
    /// and the human is told which file it was — the sessions file
    /// (`sessions.json`) is fine.
    private func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
        do {
            return try storage.withLock(body)
        } catch LockedDirectoryError.locked {
            throw DomainError.sprintsFileLocked(path: fileURL.path)
        } catch LockedDirectoryError.posix(let op, let code) {
            throw DomainError.sprintsFileIO(
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
    private func readLocked() throws -> (sprints: [Sprint], backupPath: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let sprints):
            return (sprints, nil)
        case .invalid:
            let ts = Int(Date().timeIntervalSince1970)
            var backup = directory.appendingPathComponent("sprints.json.corrupt-\(ts)")
            var suffix = 1
            let fm = FileManager.default
            while fm.fileExists(atPath: backup.path) {
                backup = directory.appendingPathComponent("sprints.json.corrupt-\(ts)-\(suffix)")
                suffix += 1
            }
            do {
                try fm.moveItem(at: fileURL, to: backup)
            } catch {
                // Never silently discard: if the invalid file cannot be kept, fail
                // loudly — **in the sprints file's own words**. The sessions file's
                // `corruptSessionsFile` would report this as a sessions failure, and
                // the sessions file is not what is broken: `sessions.json` is fine,
                // and this file is the one the human has to look for.
                throw DomainError.corruptSprintsFile(
                    path: fileURL.path,
                    detail: (error as NSError).localizedDescription)
            }
            return ([], backup.path)
        }
    }

    /// A lock-free read (the `list` fast path). Takes the lock only to recover
    /// an invalid file.
    private func readAll() throws -> (sprints: [Sprint], note: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let sprints):
            return (sprints, nil)
        case .invalid:
            let (sprints, backup) = try withLock { try readLocked() }
            return (sprints, backup.map(Self.recoveryNote))
        }
    }

    /// A read-modify-write under the exclusive lock. `body` receives the current
    /// records and returns `(value, changed, note)`; when `changed` is false no
    /// write is performed — the file's bytes are untouched — but the lock is
    /// still taken and recovery is still checked. A note from `body` (a second
    /// `open`, a second `close`) and the recovery note cannot both happen: the
    /// recovery note wins when it does, because it says the caller's model of
    /// the file is void.
    private func mutate<T: Sendable>(
        _ body: (inout [Sprint]) throws -> (value: T, changed: Bool, note: String?)
    ) throws -> (value: T, note: String?) {
        try withLock {
            let (initial, backup) = try readLocked()
            let recovery = backup.map(Self.recoveryNote)
            var sprints = initial
            do {
                let (value, changed, note) = try body(&sprints)
                if changed {
                    try writeLocked(sprints)
                }
                return (value, recovery ?? note)
            } catch {
                if let recovery {
                    // The failure is the sprints file's, whatever it is: the
                    // sessions file's `corruptSessionsFile` would name a file
                    // that is not broken.
                    let underlying: any TandemError = error as? any TandemError
                        ?? DomainError.sprintsFileIO(
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
    /// sprints file's own vocabulary. Call only while holding the lock.
    ///
    /// **The failure is this file's, not the sessions file's.** A write that
    /// cannot happen — a full disk, a read-only home, a `rename` that will not
    /// land — says so in the sprints file's words, naming the file the human
    /// has to look for. The sessions file (`sessions.json`) is a different
    /// file and it is fine.
    private func writeLocked(_ sprints: [Sprint]) throws {
        do {
            try storage.atomicWrite(try Self.encode(sprints), to: fileURL)
        } catch LockedDirectoryError.posix(let op, let code) {
            throw DomainError.sprintsFileIO(
                path: fileURL.path, op: op, detail: Self.posixDetail(code))
        } catch {
            // The primitive's POSIX failures are not the only ones: the write
            // can fail inside Foundation (a full disk, a read-only directory)
            // with an `NSError`. Both are this file's failure.
            throw DomainError.sprintsFileIO(
                path: fileURL.path, op: "write", detail: (error as NSError).localizedDescription)
        }
    }

    // MARK: - Schema (private)

    private static func encode(_ sprints: [Sprint]) throws -> Data {
        let envelope = FileEnvelope(version: 1, sprints: sprints)
        let encoder = JSONEncoder()
        // Pretty-printed, like the store's file: it is human-legible. Sorted
        // keys, so the same records always write the same bytes — Foundation's
        // encoder otherwise emits the keys in hash order, and the file would
        // reshuffle under a reader's eyes on every write.
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(envelope)
    }

    private static func decode(_ data: Data) throws -> [Sprint] {
        let envelope = try JSONDecoder().decode(FileEnvelope.self, from: data)
        guard envelope.version == 1 else {
            throw CorruptSprints.unsupportedVersion(envelope.version)
        }
        return envelope.sprints
    }

    // MARK: - Helpers (private)

    /// The lane's name, trimmed. An empty result is a domain failure: a sprint
    /// with no lane is a sprint nothing can find, and the core is the one
    /// place a later caller cannot slip past — a guard in the CLI is a
    /// courtesy on top of this one.
    private static func laneName(_ raw: String) throws -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw DomainError.emptyLaneName }
        return trimmed
    }

    /// The record for the (lane, number) pair, if the file carries one. The
    /// pair is the identity; there is no UUID to look up.
    private static func index(
        of lane: String, number: Int, in sprints: [Sprint]
    ) -> Int? {
        sprints.firstIndex { $0.lane == lane && $0.number == number }
    }

    /// The unknown-sprint failure, carrying the numbers the lane *does* hold —
    /// wrapped in a `RecoveryFailure` when this call also recovered the file,
    /// so the note is not lost.
    private static func unknownSprint(
        _ lane: String, _ number: Int, in sprints: [Sprint], note: String? = nil
    ) -> any Error {
        let known = sprints
            .filter { $0.lane == lane }
            .map(\.number)
            .sorted()
        let error = DomainError.unknownSprint(lane: lane, number: number, known: known)
        guard let note else { return error }
        return RecoveryFailure(underlying: error, note: note)
    }

    /// The OS's own words for a POSIX failure: the code and `strerror`'s
    /// description, in the shape the sessions file's own I/O errors use.
    private static func posixDetail(_ code: Int32) -> String {
        "\(code): \(String(cString: strerror(code)))"
    }

    /// The recovery note for an invalid sprints file. The sessions file's
    /// recovery note (`Sessions.recoveryNote`) in this file's own nouns: the
    /// sessions file's `sessions.json` was not touched, so the note must not
    /// say the session list restarted empty.
    private static func recoveryNote(backupPath: String) -> String {
        "Previous sprints file was corrupt and backed up to \(backupPath); the sprint list restarted empty."
    }

    /// Newest `openedAt` first; records that share a timestamp keep the order
    /// they were recorded in.
    private static func newestFirst(_ sprints: [Sprint]) -> [Sprint] {
        sprints.enumerated()
            .sorted { a, b in
                if a.element.openedAt != b.element.openedAt {
                    return a.element.openedAt > b.element.openedAt
                }
                return a.offset < b.offset
            }
            .map(\.element)
    }

    // MARK: - Display grouping

    /// The records grouped by lane for display (spec `4F746EFE`, R3).
    ///
    /// **The rule lives in the core because the core is the only test seam**
    /// (the `QuickOpen.rank` precedent): the app's Sprints tab and any later
    /// reader of the record must reach the same grouping rather than
    /// lookalikes. The lanes come out in first-appearance order — the newest
    /// sprint's lane first — and the sprints inside a group keep the order
    /// they arrive in, which is `list`'s own: newest `openedAt` first. The
    /// grouping is a fold, not a sort, so it never reshuffles a read.
    public static func groupedByLane(_ sprints: [Sprint]) -> [SprintLaneGroup] {
        var order: [String] = []
        var byLane: [String: [Sprint]] = [:]
        for sprint in sprints {
            if byLane[sprint.lane] == nil { order.append(sprint.lane) }
            byLane[sprint.lane, default: []].append(sprint)
        }
        return order.map { SprintLaneGroup(lane: $0, sprints: byLane[$0] ?? []) }
    }

    /// Now, truncated to the stored form of a timestamp: the file keeps whole
    /// seconds, so the in-memory value is the same shape — a record equals what
    /// a later read gives back.
    private static func now() -> Date {
        Date(timeIntervalSince1970: Date.now.timeIntervalSince1970.rounded())
    }
}

/// One lane's sprints, grouped for display (spec `4F746EFE`, R3) — the lane's
/// name and its records in the order they arrived (newest `openedAt` first,
/// the core's own list order, kept).
public struct SprintLaneGroup: Sendable, Hashable {
    public let lane: String
    public let sprints: [Sprint]

    public init(lane: String, sprints: [Sprint]) {
        self.lane = lane
        self.sprints = sprints
    }
}
