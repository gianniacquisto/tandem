import Foundation

/// The single source of truth for tickets (GLOSSARY: Store): one JSON file at
/// `<home>/todos.json` — the real `~/.tandem` for the shipped app, `~/.tandem-dev`
/// for anything built and run in place (`TandemHome`) — read and written by the GUI
/// and by `tandem-cli` invocations. Implements the protocol from ADR 0001:
///
/// - **Locking** — writers take an exclusive `flock` on the sidecar lockfile
///   (`<home>/tandem.lock`, never renamed), held across the whole
///   read → modify → write-temp → `rename` cycle. Readers take no lock: atomic rename
///   guarantees a reader sees a complete old or new file. `flock` releases on process
///   death, so there is no stale-lock recovery to build.
/// - **Contention** — bounded retry (10–50 ms backoff, ~2 s) then a clear
///   "store locked" error.
/// - **Atomic writes** — same-directory temp `.todos-<random>.tmp`, fsync, `rename`.
///   After acquiring the lock, orphaned temps older than 60 s (crash leftovers) are
///   removed; the age threshold never touches in-flight writes.
/// - **Lenient read, strict write** — unknown extra fields are ignored; a missing or
///   mistyped required field invalidates the whole file.
/// - **Recovery** — on an invalid file, *under the lock* (so two processes can't
///   double-recover), it is renamed to `todos.json.corrupt-<unix-ts>` and the store
///   restarts empty. The backup path is surfaced as a note.
/// - **Migration** — a file that still carries the legacy `done` mirror is copied aside
///   as `todos.json.pre-migrator-<unix-ts>` on the first write, then rewritten with
///   `status` alone. Reads never migrate: a read-only store keeps its old shape, and the
///   decoder keeps honouring it.
/// - **Rolling backup** — each write first copies the file it is about to replace to
///   `todos.json.bak` (one generation, same temp + fsync + `rename` under the same lock,
///   ADR 0018), so an ordinary bad write — a kill mid-rename, a bad regex — leaves one
///   previous good copy to fall back on. The first write has no `.bak` (no file yet), and
///   a failed write leaves the previous one untouched. Reads write nothing.
/// - **Two names, one shape** — the decoder reads the renamed keys (`tickets`, `lanes`,
///   and a ticket's `lane`) *beside* the v1 ones (`todos`, `sections`, `section`), and
///   the renamed name wins when a file carries both. The writer still emits v1, so this
///   build's file is the file an installed 0.15.0 reader expects — that reader looks the
///   array up by name and calls a file without `todos` **corrupt**, moving it to
///   `todos.json.corrupt-<ts>` and restarting empty (ADR 0006's incident). **Only a
///   writer moves the names; the reader tolerates both forever**, the same shape as the
///   `done` mirror's decoding half.
///
/// The public surface is the ticket operations (see the `Operations` extension); the
/// locking/file mechanics are the shared `LockedDirectory` primitive (ADR 0001 / 0004),
/// which the bikeshed uses too.
public struct Store: Sendable {
    /// The store's directory (created on first write). Public so the GUI can watch it
    /// for external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The data file.
    let fileURL: URL
    /// The rolling backup of the file the last write replaced — the one previous
    /// good copy. Not written by reads, and not on the first write (ADR 0018).
    let backupURL: URL
    /// The sidecar lockfile (created if missing, never renamed).
    let lockURL: URL
    /// The shared storage mechanics (ADR 0001): flock + atomic write + orphan
    /// sweep, identical to the bikeshed's (ADR 0004).
    private let storage: LockedDirectory

    /// The default store: the real `~/.tandem` from the shipped app, `~/.tandem-dev`
    /// from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A store rooted at an explicit directory (used by tests with a temp dir).
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("todos.json")
        self.backupURL = directory.appendingPathComponent("todos.json.bak")
        self.lockURL = directory.appendingPathComponent("tandem.lock")
        self.storage = LockedDirectory(
            lockURL: self.lockURL,
            tempPrefix: ".todos-",
            cleanRoot: directory,
            cleanRecursively: false)
    }

    /// Take the store lock (translating the primitive's errors into the store's
    /// own vocabulary) and run `body` under it, orphan temps cleaned first.
    /// Internal (not private): the comment operations run under the store's own
    /// lock too — the sidecar append must serialize with every store writer.
    func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
        do {
            return try storage.withLock(body)
        } catch LockedDirectoryError.locked {
            throw DomainError.locked
        } catch LockedDirectoryError.posix(let op, let code) {
            throw StoreIOError.posix(op: op, code: code)
        }
    }

    // MARK: - Low-level store protocol (private)

    private enum Peek {
        case absent
        /// A readable store, plus the bytes it was decoded from (the migration check
        /// needs the raw file; the lock-free readers ignore it).
        case valid(StoreContents, raw: Data)
        case invalid
    }

    /// A lock-free read of the current file state. Distinguishes "no file yet" (a valid
    /// empty store) from "file present but corrupt" (needs recovery).
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
            return .valid(try Self.decode(data), raw: data)
        } catch {
            return .invalid
        }
    }

    /// Read while holding the exclusive lock. Re-checks under the lock so a process that
    /// already recovered is not double-recovered; if still corrupt, backs the file up and
    /// restarts empty. Returns the contents, when *this* call performed the backup the
    /// backup path, and whether the file it read still carries the legacy `done` mirror
    /// (the caller writing must then copy it aside first). Internal (not private):
    /// `withLockedRead` builds on it for operations that read the store and write
    /// a sidecar.
    func readLocked() throws -> (contents: StoreContents, backupPath: String?, legacyMirror: Bool) {
        switch peek() {
        case .absent:
            return (StoreContents(lanes: [], todos: []), nil, false)
        case .valid(let contents, let raw):
            return (contents, nil, Self.carriesLegacyMirror(raw))
        case .invalid:
            let ts = Int(Date().timeIntervalSince1970)
            var backup = directory.appendingPathComponent("todos.json.corrupt-\(ts)")
            var suffix = 1
            let fm = FileManager.default
            while fm.fileExists(atPath: backup.path) {
                backup = directory.appendingPathComponent("todos.json.corrupt-\(ts)-\(suffix)")
                suffix += 1
            }
            do {
                try fm.moveItem(at: fileURL, to: backup)
            } catch {
                // Never silently discard: if we cannot back the corrupt file up, fail loudly.
                throw StoreIOError.io("could not back up corrupt store: \(error)")
            }
            return (StoreContents(lanes: [], todos: []), backup.path, false)
        }
    }

    /// Write while holding the lock: encode and hand to the shared atomic
    /// write-temp+fsync+rename primitive, taking the rolling backup of the file the
    /// write replaces just before the commit (see `backUpCurrentFile`).
    private func writeLocked(_ contents: StoreContents) throws {
        let data = try Self.encode(contents)
        try storage.atomicWrite(data, to: fileURL) { try backUpCurrentFile() }
    }

    /// Roll the file this write is about to replace to `todos.json.bak` — **one
    /// generation**, the state before the write. Runs under the lock, from inside the
    /// atomic write (new bytes durable, `rename` not yet done), so the backup is itself
    /// temp + fsync + `rename` and no failure mode leaves it half-written or stale-wrong:
    /// a failed write leaves the previous `.bak` untouched. No file yet means nothing to
    /// roll — the first write writes no backup. A backup we cannot take fails the write
    /// rather than going unrecorded (the `backUpBeforeMigrating` rule).
    private func backUpCurrentFile() throws {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        guard let data = try? Data(contentsOf: fileURL) else {
            throw StoreIOError.io("could not read \(fileURL.path) to roll its backup")
        }
        do {
            try storage.atomicWrite(data, to: backupURL)
        } catch {
            throw StoreIOError.io("could not back up the store to \(backupURL.path): \(error)")
        }
    }

    // MARK: - Schema (private)

    /// The envelope. `lanes` and each ticket's `lane` are **additive** to schema v1,
    /// which is why the version below does not move.
    ///
    /// **The reader takes both names; the writer still emits v1.** The renamed keys
    /// (`tickets`, `lanes`) are decoded *beside* the v1 ones (`todos`, `sections`), and
    /// the renamed name wins when a file carries both — a hand edit is not an error, and
    /// the next write makes the file one shape. The writer is untouched: it still writes
    /// `todos` / `sections`, so the file this build produces is the file an installed
    /// 0.15.0 reader expects. That reader looks the array up by name, and a file without
    /// `todos` is **corrupt** to it: it moves the file to `todos.json.corrupt-<ts>` and
    /// restarts empty (ADR 0006's incident). So the order is fixed — **only a writer
    /// moves the names**, and this reader keeps tolerating both forever, the same shape
    /// as the `done` mirror's decoding half.
    private struct FileEnvelope: Codable {
        var version: Int
        /// The ordered lane names. Absent in a v1 file written before lanes existed.
        var lanes: [String]?
        /// The tickets. The key is spelled **`todos`** on disk — the v1 key, unchanged here.
        var todos: [Ticket]

        /// `lanes` is spelled **`sections`** on disk, like a ticket's `lane` (ADR 0017).
        /// The renamed keys are read beside them; the writer emits none of them.
        private enum CodingKeys: String, CodingKey {
            case version, todos
            case lanes = "sections"
            /// The renamed array key, read beside `todos`.
            case tickets
            /// The renamed lane-list key, read beside `sections`.
            case renamedLanes = "lanes"
        }

        /// The memberwise initializer, written out because the custom `init(from:)`
        /// below suppresses the compiler's.
        init(version: Int, lanes: [String]?, todos: [Ticket]) {
            self.version = version
            self.lanes = lanes
            self.todos = todos
        }

        /// Decode the renamed names beside the v1 ones, the renamed name winning when a
        /// file carries both. A file that carries **neither** array key is invalid: this
        /// decode throws, so the store's recovery path backs the file up and restarts
        /// empty — never a silent empty list, which is the failure the recovery path
        /// exists to prevent.
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            self.version = try c.decode(Int.self, forKey: .version)
            if let renamed = try c.decodeIfPresent([Ticket].self, forKey: .tickets) {
                self.todos = renamed
            } else {
                // Absent on both: `keyNotFound` → the file is invalid → recovery.
                self.todos = try c.decode([Ticket].self, forKey: .todos)
            }
            // The lane list is additive: absent in a file written before lanes existed,
            // and it reads as no lanes either way.
            if let renamed = try c.decodeIfPresent([String].self, forKey: .renamedLanes) {
                self.lanes = renamed
            } else {
                self.lanes = try c.decodeIfPresent([String].self, forKey: .lanes)
            }
        }

        /// The writer, spelled out: **v1 keys only** — `version`, `sections`, `todos`.
        /// It is hand-written rather than synthesized because the decoder's key set
        /// carries the renamed names too, and a synthesized encoder cannot have a
        /// `CodingKeys` case that no property matches. Same keys, same order, same
        /// values as the synthesized encoder produced before the rename — the file
        /// this build writes is the file today's build writes.
        func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(version, forKey: .version)
            try c.encodeIfPresent(lanes, forKey: .lanes)
            try c.encode(todos, forKey: .todos)
        }
    }

    /// **The envelope version is bumped only for a change an older reader would
    /// misread — never for additive fields.** `lanes` and `lane` are additive: an
    /// older build ignores the unknown keys and reads the file fine. Bumping to 2 made
    /// it do the opposite — an installed v1-era `tandem-mcp` read a version-2 file,
    /// called it *corrupt*, backed it up and started empty. That happened to a real
    /// store on 2026-10-17. So the writer keeps emitting version 1. Reading still
    /// accepts 2, so any file written during that window loads. The mirror's retirement
    /// is not a version bump either: a v1-era reader fails on a missing key whatever the
    /// version says, and the migrator's gate is reachability, not the file's label.
    ///
    /// **The writer still emits the v1 keys** — `todos`, `sections`, `section` — even
    /// though the reader now takes the renamed ones too. Flipping the writer is its own
    /// change, gated on the build that reads both being installed; until then this
    /// build's file is byte-for-byte the file today's build writes.
    private static func encode(_ contents: StoreContents) throws -> Data {
        let envelope = FileEnvelope(version: 1, lanes: contents.lanes, todos: contents.todos)
        let encoder = JSONEncoder()
        // Pretty-printed: the file is intentionally human-legible (spec: Further Notes).
        encoder.outputFormatting = [.prettyPrinted]
        // The file retires the legacy `done` mirror; the CLI's JSON keeps it.
        encoder.userInfo[.omitLegacyDoneMirror] = true
        return try encoder.encode(envelope)
    }

    private static func decode(_ data: Data) throws -> StoreContents {
        // Lenient read: unknown extra fields are ignored (forward-compat); a missing or
        // mistyped required field throws (invalid file → recovery). Version 1 is the
        // current shape (with the additive lanes fields); 2 is tolerated for files
        // written during the window when the version was mistakenly bumped. The array
        // and the lane names are read under either name — the renamed one or the v1 one.
        let envelope = try JSONDecoder().decode(FileEnvelope.self, from: data)
        guard envelope.version == 1 || envelope.version == 2 else {
            throw CorruptStore.unsupportedVersion(envelope.version)
        }
        return StoreContents(lanes: envelope.lanes ?? [], todos: envelope.todos)
    }

    // MARK: - Read / mutate primitives

    /// A lock-free read (the `list` fast path). Takes the lock only to recover a corrupt
    /// file. Returns the contents and an optional recovery note. Internal (not public): the
    /// public surface is the `Operations` extension; this is the primitive it builds on.
    func readAll() throws -> (contents: StoreContents, note: String?) {
        switch peek() {
        case .absent:
            return (StoreContents(lanes: [], todos: []), nil)
        case .valid(let contents, _):
            return (contents, nil)
        case .invalid:
            let (contents, backup, _) = try withLock { try readLocked() }
            return (contents, backup.map(Self.recoveryNote))
        }
    }

    /// A read-modify-write under the exclusive lock. `body` receives the current contents
    /// (after any recovery, and normalized to the grouped-by-lane invariant) and returns
    /// `(value, changed)`; when `changed` is false no write is performed — but the lock is
    /// still taken and recovery is still checked. The contents are normalized again before
    /// the write, so every operation leaves the invariant holding. A first write over a
    /// file that still carries the legacy `done` mirror copies that file aside first (the
    /// migrator's safety net), and every write rolls the file it replaces to
    /// `todos.json.bak`. If `body` throws and this call performed a recovery, the
    /// failure is wrapped in a `RecoveryFailure` so the note is not lost. Internal (not
    /// public): the public surface is the `Operations` extension.
    func mutate<T: Sendable>(
        _ body: (inout StoreContents) throws -> (value: T, changed: Bool)
    ) throws -> (value: T, note: String?) {
        try withLock {
            let (initial, backup, legacyMirror) = try readLocked()
            let note = backup.map(Self.recoveryNote)
            var contents = initial
            contents.normalize()
            do {
                let (value, changed) = try body(&contents)
                if changed {
                    contents.normalize()
                    if legacyMirror { try backUpBeforeMigrating() }
                    try writeLocked(contents)
                }
                return (value, note)
            } catch {
                if let note {
                    let underlying: any TandemError = error as? any TandemError
                        ?? StoreIOError.io("\(error)")
                    throw RecoveryFailure(underlying: underlying, note: note)
                }
                throw error
            }
        }
    }

    /// Run `body` under the store's exclusive lock with the store read under the
    /// **same** lock (recovery and normalization included). For operations that
    /// consult the store and write elsewhere — the comment append verifies the
    /// ticket exists before appending to the sidecar, so check and write are one
    /// locked act and a ticket deleted in between cannot slip through. `body`
    /// gets the normalized contents and the recovery note when *this* call
    /// performed the backup; a throw with a note is wrapped in a
    /// `RecoveryFailure` like `mutate`, so the note is not lost. Internal (not
    /// public): the public surface is the `Operations` extension.
    /// (The closure's parameters carry no labels — function types cannot —
    /// so the body receives `(contents, note)` positionally: the normalized
    /// contents, then the recovery note, non-nil only when *this* call
    /// performed the backup.)
    func withLockedRead<T: Sendable>(
        _ body: (StoreContents, String?) throws -> T
    ) throws -> T {
        try withLock {
            let (initial, backup, _) = try self.readLocked()
            let note = backup.map(Self.recoveryNote)
            var contents = initial
            contents.normalize()
            do {
                return try body(contents, note)
            } catch {
                if let note {
                    let underlying: any TandemError = error as? any TandemError
                        ?? StoreIOError.io("\(error)")
                    throw RecoveryFailure(underlying: underlying, note: note)
                }
                throw error
            }
        }
    }

    /// Copy a file that still carries the legacy `done` mirror aside, once, before the
    /// first write in the retired shape. "A list moved aside, never a list lost": the old
    /// format stays one `mv` away for a binary that still needs it.
    private func backUpBeforeMigrating() throws {
        let ts = Int(Date().timeIntervalSince1970)
        var backup = directory.appendingPathComponent("todos.json.pre-migrator-\(ts)")
        var suffix = 1
        let fm = FileManager.default
        while fm.fileExists(atPath: backup.path) {
            backup = directory.appendingPathComponent("todos.json.pre-migrator-\(ts)-\(suffix)")
            suffix += 1
        }
        do {
            try fm.copyItem(at: fileURL, to: backup)
        } catch {
            throw StoreIOError.io("could not back up the store before retiring the done mirror: \(error)")
        }
    }

    /// Whether any ticket object in the raw file still carries the legacy `done` key.
    /// Only asked on the write path (under the lock), never on a plain read. The array
    /// is looked up under either name — the renamed one or the v1 one — so a file in the
    /// renamed shape gets the same safety copy before its `done` keys are dropped.
    private static func carriesLegacyMirror(_ data: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return false
        }
        let tickets = (object["tickets"] as? [[String: Any]])
            ?? (object["todos"] as? [[String: Any]])
            ?? []
        return tickets.contains { $0["done"] != nil }
    }

    /// The recovery note text, naming the backup path (spec: result `note`).
    static func recoveryNote(backupPath: String) -> String {
        "Previous store was corrupt and backed up to \(backupPath); the store restarted empty."
    }

    // MARK: - Diagnostics (read-only; ADR 0014)

    /// The store's health for `tandem-cli doctor`: the open/done counts, or
    /// "unreadable" for a present-but-invalid file. Unlike `readAll`, this
    /// performs **no recovery** — a corrupt store is reported, never backed up
    /// or rewritten — so a diagnostic never modifies the store. Lock-free, like
    /// every read.
    public enum Health: Sendable, Equatable {
        /// No file yet — a valid empty store (created on first write).
        case absent
        /// A readable store: the load-free counts.
        case counts(open: Int, done: Int)
        /// A file is present but cannot be decoded (corrupt) or read (permission).
        case unreadable
    }

    /// A lock-free, non-mutating health read (see `Health`).
    public func health() -> Health {
        switch peek() {
        case .absent:
            return .absent
        case .valid(let contents, _):
            let done = contents.todos.lazy.filter { $0.done }.count
            return .counts(open: contents.todos.count - done, done: done)
        case .invalid:
            return .unreadable
        }
    }

    /// The rolling backup's state (ADR 0018) for `tandem-cli doctor`: whether
    /// `todos.json.bak` exists, and how long ago it was last written. Separate from
    /// `Health` because it is a property *beside* the store, not of its contents —
    /// and never a failure: a store written once, or read-only, simply has no backup.
    /// Lock-free and non-mutating, like every diagnostic read.
    public enum BackupHealth: Sendable, Equatable {
        /// No `todos.json.bak` — the normal state before the second write.
        case absent
        /// A backup exists; it was written `age` seconds ago.
        case present(age: TimeInterval)
    }

    /// A lock-free read of the rolling backup's presence and age (see `BackupHealth`).
    public func backupHealth() -> BackupHealth {
        guard
            let attrs = try? FileManager.default.attributesOfItem(atPath: backupURL.path),
            let modified = attrs[.modificationDate] as? Date
        else { return .absent }
        return .present(age: Date().timeIntervalSince(modified))
    }

    /// Whether the store's sidecar lock is currently held by another process —
    /// a non-destructive probe (see `LockedDirectory.isLocked`).
    public func lockIsHeld() -> Bool {
        storage.isLocked()
    }
}

/// A domain failure that also triggered corrupt-file recovery — the store's
/// `todos.json` or the sessions file. Carries the underlying failure (for its
/// message and, for `unknownID`, the open list) plus the recovery note, so a
/// caller that both fails and recovers still learns that its prior model is void.
public struct RecoveryFailure: Error, Sendable {
    public let underlying: any TandemError
    public let note: String
}

extension RecoveryFailure: TandemError {
    public var message: String { underlying.message }
}
