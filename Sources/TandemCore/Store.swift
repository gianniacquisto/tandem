import Foundation

/// The single source of truth for todos (GLOSSARY: Store): one JSON file at
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
///
/// The public surface is the todo operations (see the `Operations` extension); the
/// locking/file mechanics are the shared `LockedDirectory` primitive (ADR 0001 / 0004),
/// which the bikeshed uses too.
public struct Store: Sendable {
    /// The store's directory (created on first write). Public so the GUI can watch it
    /// for external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The data file.
    let fileURL: URL
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
        self.lockURL = directory.appendingPathComponent("tandem.lock")
        self.storage = LockedDirectory(
            lockURL: self.lockURL,
            tempPrefix: ".todos-",
            cleanRoot: directory,
            cleanRecursively: false)
    }

    /// Take the store lock (translating the primitive's errors into the store's
    /// own vocabulary) and run `body` under it, orphan temps cleaned first.
    private func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
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
    /// (the caller writing must then copy it aside first).
    private func readLocked() throws -> (contents: StoreContents, backupPath: String?, legacyMirror: Bool) {
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
    /// write-temp+fsync+rename primitive.
    private func writeLocked(_ contents: StoreContents) throws {
        let data = try Self.encode(contents)
        try storage.atomicWrite(data, to: fileURL)
    }

    // MARK: - Schema (private)

    /// The envelope. `lanes` and each todo's `lane` are **additive** to schema v1,
    /// which is why the version below does not move.
    private struct FileEnvelope: Codable {
        var version: Int
        /// The ordered lane names. Absent in a v1 file written before lanes existed.
        var lanes: [String]?
        var todos: [Todo]

        /// `lanes` is spelled **`sections`** on disk, like a todo's `lane` (ADR 0017).
        private enum CodingKeys: String, CodingKey {
            case version, todos
            case lanes = "sections"
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
        // written during the window when the version was mistakenly bumped.
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
    /// migrator's safety net). If `body` throws and this call performed a recovery, the
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

    /// Whether any todo object in the raw file still carries the legacy `done` key.
    /// Only asked on the write path (under the lock), never on a plain read.
    private static func carriesLegacyMirror(_ data: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let todos = object["todos"] as? [[String: Any]]
        else {
            return false
        }
        return todos.contains { $0["done"] != nil }
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

    /// Whether the store's sidecar lock is currently held by another process —
    /// a non-destructive probe (see `LockedDirectory.isLocked`).
    public func lockIsHeld() -> Bool {
        storage.isLocked()
    }
}

/// A domain failure that also triggered corrupt-store recovery. Carries the underlying
/// failure (for its message and, for `unknownID`, the open list) plus the recovery note,
/// so a caller that both fails and recovers still learns that its prior model is void.
public struct RecoveryFailure: Error, Sendable {
    public let underlying: any TandemError
    public let note: String
}

extension RecoveryFailure: TandemError {
    public var message: String { underlying.message }
}
