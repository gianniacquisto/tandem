import Foundation
import SQLite3

/// The single source of truth for tickets (GLOSSARY: Store): one SQLite database at
/// `<home>/tandem.db`, read and written by the GUI and by `tandem-cli` invocations
/// (ADR 0031, ticket 6B96343A). The real `~/.tandem` belongs to the shipped app,
/// `~/.tandem-dev` to anything built and run in place (`TandemHome`).
///
/// The public surface is unchanged — the ticket operations in the `Operations`
/// extension. The app and the CLI are thin adapters over it and do not know the storage
/// changed; only the mechanics below did:
///
/// - **Writers** — one connection per operation, WAL, `busy_timeout` at the store's
///   ~2 s window. A timeout surfaces as the store's existing "locked" domain error,
///   the same words ADR 0007's translation rule gave the sidecar lock. The temp +
///   fsync + rename dance and the `tandem.lock` write lock are gone from the store: a
///   transaction is the atomic write, and SQLite's journal covers the kill-mid-write
///   case. The comments domain moved into the database too (P2), so no store operation
///   takes a sidecar lock and the `tandem.lock` file is retired — a stale one beside a
///   home is inert.
/// - **Readers** — lock-free. A WAL snapshot is the consistent read ADR 0001 promised,
///   and a read never writes.
/// - **Rolling backup** — kept (ADR 0018): before each write transaction the database is
///   copied to `tandem.db.bak` through SQLite's backup API, one generation. A copy that
///   fails fails the write; reads never write. Restoring it is
///   `rm tandem.db-wal tandem.db-shm tandem.db && mv tandem.db.bak tandem.db` — the
///   journal files must go first, or the restored file is read under the dead WAL.
/// - **Schema** — `schema_migrations` is the version seam: an integer version and the
///   UTC timestamp it was applied at, one transaction per migration. A failed migration
///   rolls back and fails the open loudly; nothing half-applies.
/// - **Read-fallback and the one-time import** — a home that has no database but still
///   has the JSON store reads the JSON (both names, both shapes) so an installed build's
///   first read never lies. The first write imports it, copies the file aside as
///   `<name>.pre-sqlite-<ts>`, and records that in `store_imports` — rows, marker and
///   copy in one transaction. Both decisions are made from that durable marker inside
///   the write transaction, never from a filesystem snapshot taken before the
///   connection: two writers import once, and a kill before the commit leaves the JSON
///   as the store's truth for the next read and the next write (`StoreMigration`).
/// - **Recovery** — there is no corrupt-file recovery left to build: a write is one
///   transaction, so the erasure-by-unknown-writer and half-written-file class the JSON
///   protocol's recovery path existed for cannot happen. A legacy JSON file that cannot
///   be decoded fails the read loudly instead of being moved aside and restarted empty.
///
/// The comments, sessions, sprints and settings domains are re-backed on the same
/// database (P2): each imports its JSON file once, on its own first write, and reads
/// its legacy file only until that marker lands.
public struct Store: Sendable {
    /// The store's directory (created on first write). Public so the GUI can watch it
    /// for external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The JSON store's new name: `<home>/tickets.json`. No longer written; it is the
    /// read-fallback's first name and the legacy import's source.
    let fileURL: URL
    /// The database: `<home>/tandem.db` (WAL; `-wal`/`-shm` beside it). Public for
    /// `doctor`, which names it as the store.
    public var databaseURL: URL {
        directory.appendingPathComponent("tandem.db")
    }
    /// The JSON file the read-fallback would read right now — `tickets.json` when it is
    /// a regular file, else `todos.json`, else `tickets.json` (absent). Public for
    /// `doctor`'s fork note and for tests that park a legacy fixture.
    public var storeFileURL: URL {
        (try? StoreMigration.legacyStore(in: directory)) ?? fileURL
    }
    /// The rolling backup of the database the last write replaced — the one previous
    /// good copy (ADR 0018). Not written by reads, and not on the first write.
    var backupURL: URL {
        directory.appendingPathComponent("tandem.db.bak")
    }

    /// The store's directory's database file, as a fact about the filesystem.
    private var databaseExists: Bool {
        FileManager.default.fileExists(atPath: databaseURL.path)
    }

    /// The default store: the real `~/.tandem` from the shipped app, `~/.tandem-dev`
    /// from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A store rooted at an explicit directory (used by tests with a temp dir).
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("tickets.json")
    }

    // MARK: - Reads

    /// A read: the database when it is the store, else the JSON store, else empty.
    /// Never writes — a read of a fresh home creates nothing and a read of a JSON home
    /// does not import.
    ///
    /// "Is it the store?" is the import marker, not the file's existence: a database
    /// whose first write was killed before its import committed has schema and no rows,
    /// and answering from it would show an empty board beside a file full of tickets.
    /// Until the marker lands, the JSON is the truth. The `note` is always `nil` now;
    /// the shape is kept because every operation's result carries it and the GUI renders
    /// it.
    func readAll() throws -> (contents: StoreContents, note: String?) {
        if databaseExists {
            let stored = try Self.translate { () -> StoreContents? in
                let db = try SQLiteDatabase(url: databaseURL, mode: .readOnly)
                defer { db.close() }
                guard try StoreSchema.isImported(StoreSchema.ticketsDomain, in: db) else {
                    return nil
                }
                return try StoreSchema.readContents(db)
            }
            if let contents = stored { return (contents, nil) }
        }
        if let legacy = try StoreMigration.legacyStore(in: directory) {
            return (try StoreMigration.read(legacy), nil)
        }
        return (StoreContents(lanes: [], todos: []), nil)
    }

    /// A read-modify-write under the store's write transaction. `body` receives the
    /// current contents (normalized to the grouped-by-lane invariant) and returns
    /// `(value, changed)`; when `changed` is false the mutation writes nothing beyond
    /// what the first write's import records.
    ///
    /// One database transaction: the migration is idempotent under its own lock, and the
    /// import decision is read **inside this transaction** from `store_imports`, so two
    /// writers opening a JSON home at once import once and neither loses the other's
    /// rows. A kill before the commit leaves no marker and no rows — the next write
    /// imports the file again. Every changed write after the first rolls
    /// `tandem.db.bak` to the state before it.
    /// Internal (not public): the public surface is the `Operations` extension.
    func mutate<T: Sendable>(
        _ body: (inout StoreContents) throws -> (value: T, changed: Bool)
    ) throws -> (value: T, note: String?) {
        let outcome: (value: T, changed: Bool) = try Self.translate {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
            let db = try SQLiteDatabase(url: databaseURL, mode: .readWrite)
            defer { db.close() }
            try StoreSchema.migrate(db)
            var result: (value: T, changed: Bool)?
            try db.transaction {
                // The import and the caller's change are one atomic act: the marker, the
                // imported rows and the caller's change commit together, so a second
                // writer sees the marker rather than re-importing over the first.
                var (contents, firstWrite) = try StoreMigration.importIfNeeded(
                    in: directory, into: db)
                contents.normalize()
                let step = try body(&contents)
                result = step
                if step.changed {
                    contents.normalize()
                    // Before any page of this transaction is modified, the database is
                    // still the pre-write state — the one generation ADR 0018 keeps.
                    // The copy is taken through its own read-only connection: a WAL
                    // reader sees the last committed state while this write waits, and
                    // the backup API does not run against a connection holding the write
                    // lock. The first write has no earlier state to roll.
                    if !firstWrite {
                        try Self.backUpDatabase(at: databaseURL, to: backupURL)
                    }
                    try StoreSchema.writeContents(contents, to: db)
                }
            }
            guard let result else {
                throw StoreIOError.io("the store transaction produced no result")
            }
            return result
        }
        return (outcome.value, nil)
    }

    // MARK: - Diagnostics (read-only; ADR 0014)

    /// The store's health for `tandem-cli doctor`: the open/done counts, or
    /// "unreadable" for a database this build cannot read — or a legacy file it cannot
    /// decode. Performs **no recovery and no import**; a diagnostic never writes.
    /// Lock-free, like every read.
    public enum Health: Sendable, Equatable {
        /// No imported database and no legacy file — a valid empty store (created on
        /// first write).
        case absent
        /// A readable store: the load-free counts.
        case counts(open: Int, done: Int)
        /// A store is present but cannot be read (corrupt, or not a database).
        case unreadable
    }

    /// A lock-free, non-mutating health read (see `Health`).
    public func health() -> Health {
        do {
            if databaseExists {
                let db = try SQLiteDatabase(url: databaseURL, mode: .readOnly)
                defer { db.close() }
                // An un-imported database is not the store yet (a first write killed
                // before its import): its counts would be a lie, so fall through to the
                // JSON it still has to import.
                if try StoreSchema.isImported(StoreSchema.ticketsDomain, in: db) {
                    guard try StoreSchema.version(of: db) != nil else { return .unreadable }
                    let counts = try StoreSchema.counts(db)
                    return .counts(open: counts.open, done: counts.done)
                }
            }
            if let legacy = try StoreMigration.legacyStore(in: directory) {
                let contents = try StoreMigration.read(legacy)
                let done = contents.todos.lazy.filter(\.done).count
                return .counts(open: contents.todos.count - done, done: done)
            }
            return .absent
        } catch {
            return .unreadable
        }
    }

    /// Whether the ticket domain's import has been recorded — the database is the
    /// store's truth, not a file that a first write created before importing. For
    /// `doctor`'s fork note, which must not call the still-unread JSON a dead copy.
    func hasImportedTickets() -> Bool {
        guard databaseExists else { return false }
        return (try? Self.translate {
            let db = try SQLiteDatabase(url: databaseURL, mode: .readOnly)
            defer { db.close() }
            return try StoreSchema.isImported(StoreSchema.ticketsDomain, in: db)
        }) ?? false
    }

    /// The applied migration version, or `nil` when there is no database yet. For
    /// `doctor`'s store row (ADR 0014): the version seam, reported as a fact.
    public func schemaVersion() -> Int? {
        guard databaseExists else { return nil }
        return try? Self.translate {
            let db = try SQLiteDatabase(url: databaseURL, mode: .readOnly)
            defer { db.close() }
            return try StoreSchema.version(of: db)
        }
    }

    /// The rolling backup's state (ADR 0018) for `tandem-cli doctor`: whether
    /// `tandem.db.bak` exists, and how long ago it was last written. Never a failure: a
    /// store written once, or read-only, simply has no backup. Lock-free and
    /// non-mutating, like every diagnostic read.
    public enum BackupHealth: Sendable, Equatable {
        /// No `tandem.db.bak` — the normal state before the second write.
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

    /// Whether another process currently holds the store's write side — a
    /// non-destructive probe. The `lock` check kept its name (ADR 0014); the thing it
    /// probes is now a write transaction: try to begin one and roll it straight back. A
    /// missing database is free, and the probe never creates or writes anything.
    public func lockIsHeld() -> Bool {
        guard databaseExists else { return false }
        guard let db = try? SQLiteDatabase(url: databaseURL, mode: .readWriteNoWait) else {
            return false
        }
        defer { db.close() }
        do {
            try db.run("BEGIN IMMEDIATE")
            try db.run("ROLLBACK")
            return false
        } catch let error as SQLiteError where error.code == SQLITE_BUSY || error.code == SQLITE_LOCKED {
            return true
        } catch {
            return false
        }
    }

    /// The rolling backup: open the database read-only (a WAL snapshot of the last
    /// committed state) and copy it through SQLite's backup API. A copy that fails fails
    /// the write.
    static func backUpDatabase(at source: URL, to destination: URL) throws {
        let db = try SQLiteDatabase(url: source, mode: .readOnly)
        defer { db.close() }
        try db.backUp(to: destination)
    }

    // MARK: - Error translation

    /// ADR 0007's rule, at the new seam: SQLite's contention becomes the store's
    /// "locked" domain error (the same words), every other engine failure the store's
    /// I/O vocabulary with the engine's detail.
    static func translate<T>(_ body: () throws -> T) throws -> T {
        do {
            return try body()
        } catch let error as SQLiteError {
            if error.code == SQLITE_BUSY || error.code == SQLITE_LOCKED {
                throw DomainError.locked
            }
            throw StoreIOError.io(error.message)
        }
    }
}

/// A domain failure that also triggered corrupt-file recovery — the store's
/// `tickets.json` or the sessions file. Carries the underlying failure (for its
/// message and, for `unknownID`, the open list) plus the recovery note, so a
/// caller that both fails and recovers still learns that its prior model is void.
///
/// The ticket domain no longer recovers — a write is one transaction — but the sessions
/// and sprints files still do, and the GUI renders their note through this type.
public struct RecoveryFailure: Error, Sendable {
    public let underlying: any TandemError
    public let note: String
}

extension RecoveryFailure: TandemError {
    public var message: String { underlying.message }
}
