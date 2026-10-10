import Foundation
import Darwin

/// The single source of truth for todos (GLOSSARY: Store): one JSON file at
/// `~/.tandem/todos.json`, read and written by the GUI and the per-session MCP server
/// processes. Implements the protocol from ADR 0001:
///
/// - **Locking** — writers take an exclusive `flock` on the sidecar lockfile
///   (`~/.tandem/tandem.lock`, never renamed), held across the whole
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
///
/// The public surface is the todo operations (see the `Operations` extension); the
/// locking/file mechanics are private.
public struct Store: Sendable {
    /// The store's directory (created on first write). Public so the GUI can watch it
    /// for external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The data file.
    let fileURL: URL
    /// The sidecar lockfile (created if missing, never renamed).
    let lockURL: URL

    // Contention + cleanup tuning (ADR 0001).
    private static let lockTimeout: TimeInterval = 2.0
    private static let backoffRangeMicros: ClosedRange<UInt32> = 10_000...50_000   // 10–50 ms, in µs
    private static let orphanMaxAge: TimeInterval = 60        // seconds

    /// The default store at `~/.tandem`.
    public init() {
        self.init(directory: URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent(".tandem"))
    }

    /// A store rooted at an explicit directory (used by tests with a temp dir).
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("todos.json")
        self.lockURL = directory.appendingPathComponent("tandem.lock")
    }

    // MARK: - Low-level store protocol (private)

    private enum Peek {
        case absent
        case valid([Todo])
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
            return .valid(try Self.decode(data))
        } catch {
            return .invalid
        }
    }

    /// Read while holding the exclusive lock. Re-checks under the lock so a process that
    /// already recovered is not double-recovered; if still corrupt, backs the file up and
    /// restarts empty. Returns the todos and, when *this* call performed the backup, the
    /// backup path.
    private func readLocked() throws -> (todos: [Todo], backupPath: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let todos):
            return (todos, nil)
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
            return ([], backup.path)
        }
    }

    /// Write while holding the lock: same-directory temp file, fsync, atomic rename.
    private func writeLocked(_ todos: [Todo]) throws {
        let fm = FileManager.default
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try Self.encode(todos)
        let tmp = directory.appendingPathComponent(".todos-\(UUID().uuidString).tmp")
        do {
            try data.write(to: tmp, options: [])
            try Self.fsync(tmp)
            if rename(tmp.path, fileURL.path) != 0 {
                throw StoreIOError.posix(op: "rename", code: errno)
            }
        } catch {
            try? fm.removeItem(at: tmp)
            throw error
        }
    }

    /// Remove orphaned `.todos-*.tmp` files older than 60 s. Called after acquiring the
    /// lock; the age threshold guarantees in-flight writes are never touched.
    private func cleanOrphans() {
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(atPath: directory.path) else { return }
        let cutoff = Date().addingTimeInterval(-Self.orphanMaxAge)
        for name in entries where name.hasPrefix(".todos-") && name.hasSuffix(".tmp") {
            let url = directory.appendingPathComponent(name)
            guard
                let attrs = try? fm.attributesOfItem(atPath: url.path),
                let mod = attrs[.modificationDate] as? Date,
                mod < cutoff
            else { continue }
            try? fm.removeItem(at: url)
        }
    }

    /// Acquire the exclusive `flock` on the sidecar lockfile with bounded retry. Returns
    /// an open fd that must be `close`d (closing releases the lock).
    private func acquireLock() throws -> Int32 {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let fd = open(lockURL.path, O_RDWR | O_CREAT, 0o644)
        guard fd >= 0 else {
            throw StoreIOError.posix(op: "open lockfile", code: errno)
        }
        let deadline = Date().addingTimeInterval(Self.lockTimeout)
        while true {
            if flock(fd, LOCK_EX | LOCK_NB) == 0 { return fd }
            if Date() >= deadline {
                close(fd)
                throw DomainError.locked
            }
            usleep(UInt32.random(in: Self.backoffRangeMicros))
        }
    }

    private func releaseLock(_ fd: Int32) {
        if fd >= 0 { close(fd) }
    }

    // MARK: - Schema v1 (private)

    private struct FileEnvelope: Codable {
        var version: Int
        var todos: [Todo]
    }

    private static func encode(_ todos: [Todo]) throws -> Data {
        let envelope = FileEnvelope(version: 1, todos: todos)
        let encoder = JSONEncoder()
        // Pretty-printed: the file is intentionally human-legible (spec: Further Notes).
        encoder.outputFormatting = [.prettyPrinted]
        return try encoder.encode(envelope)
    }

    private static func decode(_ data: Data) throws -> [Todo] {
        // Standard Codable: unknown extra fields are ignored (forward-compat); a missing
        // or mistyped required field throws (invalid file → recovery).
        let envelope = try JSONDecoder().decode(FileEnvelope.self, from: data)
        guard envelope.version == 1 else {
            throw CorruptStore.unsupportedVersion(envelope.version)
        }
        return envelope.todos
    }

    private static func fsync(_ url: URL) throws {
        let fd = open(url.path, O_RDONLY)
        guard fd >= 0 else { return }
        defer { close(fd) }
        guard Darwin.fsync(fd) == 0 else {
            throw StoreIOError.posix(op: "fsync", code: errno)
        }
    }

    // MARK: - Read / mutate primitives

    /// A lock-free read (the `list` fast path). Takes the lock only to recover a corrupt
    /// file. Returns the todos and an optional recovery note. Internal (not public): the
    /// public surface is the `Operations` extension; this is the primitive it builds on.
    func readAll() throws -> (todos: [Todo], note: String?) {
        switch peek() {
        case .absent:
            return ([], nil)
        case .valid(let todos):
            return (todos, nil)
        case .invalid:
            let fd = try acquireLock()
            defer { releaseLock(fd) }
            let (todos, backup) = try readLocked()
            return (todos, backup.map(Self.recoveryNote))
        }
    }

    /// A read-modify-write under the exclusive lock. `body` receives the current todos
    /// (after any recovery) and returns `(value, changed)`; when `changed` is false no
    /// write is performed — but the lock is still taken and recovery is still checked.
    /// If `body` throws and this call performed a recovery, the failure is wrapped in a
    /// `RecoveryFailure` so the note is not lost. Internal (not public): the public
    /// surface is the `Operations` extension; this is the primitive it builds on.
    func mutate<T: Sendable>(
        _ body: (inout [Todo]) throws -> (value: T, changed: Bool)
    ) throws -> (value: T, note: String?) {
        let fd = try acquireLock()
        defer { releaseLock(fd) }
        cleanOrphans()
        let (initial, backup) = try readLocked()
        let note = backup.map(Self.recoveryNote)
        var list = initial
        do {
            let (value, changed) = try body(&list)
            if changed { try writeLocked(list) }
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

    /// The recovery note text, naming the backup path (spec: result `note`).
    static func recoveryNote(backupPath: String) -> String {
        "Previous store was corrupt and backed up to \(backupPath); the store restarted empty."
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
