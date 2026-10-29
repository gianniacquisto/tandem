import Foundation

/// The one-generation migration from the JSON store files to `tandem.db` (ADR 0031,
/// ticket 6B96343A) — the ticket domain's half of it; the sidecars follow in their own
/// ticket.
///
/// Two rules, one on each side of the write:
///
/// - **Read-fallback.** A read of a home that has no database but still has
///   `tickets.json` (or the legacy `todos.json`) reads the file. An installed build's
///   first read never lies, and a hand-restored file is still readable — one generation
///   deeper than the dual-name reader, so both names and both shapes are read forever.
///   A fresh home (no db, no file) is an empty store, never an error.
/// - **Import on the first write.** When a write finds no import recorded in
///   `store_imports`, it reads the legacy file once (`tickets.json` or `todos.json`),
///   copies it aside as `<name>.pre-sqlite-<unix-ts>` (a copy that cannot be taken
///   fails the write), and writes the rows **and the marker in one transaction**. After
///   that the JSON file is never read for a write again. Because the marker is durable
///   state read inside the write transaction, two first writers import once and a kill
///   before the commit leaves the file as the store's truth for the next write.
///
///   A copy taken by an import that then rolls back stays where it is: it is a byte
///   copy of a file that is still the store, so it is evidence of an attempt, never of
///   a landed import, and the next import writes its own suffixed copy (`copyAside`).
///
/// The import is install-gated in ADR 0017's sense: the trigger is a build that carries
/// the database reaching the home, not a flag or a click.
enum StoreMigration {
    /// The JSON store file the fallback would read: `tickets.json` when it is a regular
    /// file, else `todos.json` when that is. A present-but-not-a-regular-file
    /// `tickets.json` is invalid — the old name is never a fall-through for a broken new
    /// one.
    static func legacyStore(in directory: URL) throws -> URL? {
        for name in ["tickets.json", "todos.json"] {
            let url = directory.appendingPathComponent(name)
            guard let mode = lstatMode(of: url) else { continue }
            guard mode & S_IFMT == S_IFREG else {
                throw StoreIOError.io(
                    "\(url.path) is present but is not a regular file; move it aside and retry")
            }
            return url
        }
        return nil
    }

    /// Read the legacy ticket file once, in either shape: the renamed keys (`tickets`,
    /// `lanes`, a ticket's `lane`) beside the v1 ones (`todos`, `sections`, `section`),
    /// the renamed name winning. Version 1 is current; 2 is read because the version was
    /// once mistakenly bumped (ADR 0006's incident). Anything else is refused loudly —
    /// never read as an empty list.
    static func read(_ url: URL) throws -> StoreContents {
        let data: Data
        do {
            data = try Data(contentsOf: url)
        } catch {
            throw StoreIOError.io("could not read the legacy store at \(url.path): \(error)")
        }
        do {
            let envelope = try JSONDecoder().decode(LegacyFileEnvelope.self, from: data)
            guard envelope.version == 1 || envelope.version == 2 else {
                throw CorruptStore.unsupportedVersion(envelope.version)
            }
            return StoreContents(laneNames: envelope.lanes ?? [], todos: envelope.todos)
        } catch {
            throw StoreIOError.io(
                "the legacy store at \(url.path) is corrupt (\(error)); "
                + "move it aside and retry — the store is not rewritten")
        }
    }

    /// The import decision **inside the caller's write transaction**, made from durable
    /// state rather than from a snapshot taken before the connection: the
    /// `store_imports` marker is absent exactly while no build carrying the database has
    /// completed a first write here. A second writer that waited sees the marker and
    /// does not import; a process killed before the commit leaves no marker, so the next
    /// write imports the file it still finds.
    ///
    /// The rows, the marker and the copy-aside all belong to that one transaction: a
    /// throw takes the rows and the marker back, and the JSON stays the store's truth
    /// until a write commits. Returns the contents the caller's mutation starts from,
    /// and whether this is the home's first write (which rolls no backup).
    static func importIfNeeded(
        in directory: URL, into db: SQLiteDatabase
    ) throws -> (contents: StoreContents, firstWrite: Bool) {
        if try StoreSchema.isImported(StoreSchema.ticketsDomain, in: db) {
            return (try StoreSchema.readContents(db), false)
        }
        let source = try legacyStore(in: directory)
        let contents = try source.map { try read($0) }
            ?? StoreContents(lanes: [], todos: [])
        if let source { try copyAside(source) }
        try StoreSchema.writeContents(contents, to: db)
        try StoreSchema.markImported(
            StoreSchema.ticketsDomain, from: source?.lastPathComponent, at: Date(), in: db)
        return (contents, true)
    }

    // MARK: - Sidecar domains (P2)

    /// A sidecar domain's read: the database when its import marker is present, else
    /// `nil` so the caller serves the JSON file it still has. Never writes, and never
    /// imports — a diagnostic or a first read before the first write is the file's.
    static func sidecarRead<T>(
        in directory: URL, domain: String, _ body: (SQLiteDatabase) throws -> T
    ) throws -> T? {
        let database = directory.appendingPathComponent("tandem.db")
        guard FileManager.default.fileExists(atPath: database.path) else { return nil }
        let db = try SQLiteDatabase(url: database, mode: .readOnly)
        defer { db.close() }
        guard try StoreSchema.isImported(domain, in: db) else { return nil }
        return try body(db)
    }

    /// A sidecar domain's write: open the store, migrate, and in **one** transaction
    /// decide the import from its marker, write the legacy rows, and run the caller's
    /// change. `importDomain` reads and inserts the domain's source file(s) and returns
    /// the source name to record (`nil` for a fresh home); the marker is written here,
    /// in the same transaction, after those rows. `body` returns the value, whether it
    /// changed anything, and the write to run — the rolling backup (ADR 0018) is taken
    /// between the decision and that write, so the `.bak` holds the pre-write state.
    static func sidecarWrite<T>(
        in directory: URL, domain: String,
        importDomain: (SQLiteDatabase) throws -> (source: String?, note: String?),
        body: (SQLiteDatabase) throws -> (value: T, changed: Bool, note: String?, write: () throws -> Void)
    ) throws -> (value: T, note: String?) {
        let database = directory.appendingPathComponent("tandem.db")
        let databaseExisted = FileManager.default.fileExists(atPath: database.path)
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let db = try SQLiteDatabase(url: database, mode: .readWrite)
        defer { db.close() }
        // Same order as the ticket store's write: a migration commits in its own
        // transaction, so the pre-migration image is rolled before it (TAN-221).
        let migrating = databaseExisted
            ? try StoreSchema.pendingMigrationVersion(of: db) != nil : false
        if migrating {
            try Store.backUpDatabase(
                at: database, to: directory.appendingPathComponent("tandem.db.bak"))
        }
        try StoreSchema.migrate(db)
        var result: (value: T, note: String?)?
        try db.transaction {
            var note: String?
            if !(try StoreSchema.isImported(domain, in: db)) {
                let imported = try importDomain(db)
                try StoreSchema.markImported(
                    domain, from: imported.source, at: Date(), in: db)
                note = imported.note
            }
            let step = try body(db)
            if step.changed {
                if databaseExisted && !migrating {
                    try Store.backUpDatabase(
                        at: database,
                        to: directory.appendingPathComponent("tandem.db.bak"))
                }
                try step.write()
            }
            result = (step.value, note ?? step.note)
        }
        guard let result else {
            throw StoreIOError.io("the store transaction produced no result")
        }
        return result
    }

    // MARK: - Private

    /// Copy the legacy file aside as `<name>.pre-sqlite-<unix-ts>`, colliding names
    /// suffixed. The bytes as they were: the file is the evidence the import is true,
    /// and a copy that cannot be taken fails the write rather than being skipped.
    ///
    /// The copy is taken inside the caller's transaction, so a throwing import (the
    /// marker, the caller's own change) rolls the rows back and leaves the copy behind.
    /// Accepted, not removed: the copy is byte-for-byte the file that is still the
    /// store, so it records an attempt and not a landed import, and the next import
    /// writes a second, suffixed copy rather than reusing this one — the same way it
    /// would after a kill. Removing it would mean threading a rollback hook through
    /// every domain's import, and a failed write would then leave less evidence.
    static func copyAside(_ url: URL) throws {
        let directory = url.deletingLastPathComponent()
        let stamp = Int(Date().timeIntervalSince1970)
        let prefix = "\(url.lastPathComponent).pre-sqlite-\(stamp)"
        var destination = directory.appendingPathComponent(prefix)
        var suffix = 1
        let fm = FileManager.default
        while fm.fileExists(atPath: destination.path) {
            destination = directory.appendingPathComponent("\(prefix)-\(suffix)")
            suffix += 1
        }
        do {
            try fm.copyItem(at: url, to: destination)
        } catch {
            // A partial destination is not evidence of anything: remove it so the failed
            // write leaves no artifact behind.
            try? fm.removeItem(at: destination)
            throw StoreIOError.io(
                "could not copy the legacy store at \(url.path) aside before importing: \(error)")
        }
    }

    private static func lstatMode(of url: URL) -> mode_t? {
        var info = stat()
        return lstat(url.path, &info) == 0 ? info.st_mode : nil
    }
}

/// The JSON store's envelope, reader half only: the writer is the database now, so the
/// schema outlives its JSON encoding by exactly one read.
///
/// **The reader takes both names.** The renamed keys (`tickets`, `lanes`) are decoded
/// beside the v1 ones (`todos`, `sections`), and the renamed name wins when a file
/// carries both. A file that carries neither array key is invalid (the decode throws),
/// never a silent empty list.
private struct LegacyFileEnvelope: Decodable {
    var version: Int
    var lanes: [String]?
    var todos: [Ticket]

    private enum CodingKeys: String, CodingKey {
        case version, todos
        /// The v1 lane-list key, read beside `lanes` and kept forever (ADR 0017).
        case lanes = "sections"
        /// The renamed array key.
        case tickets
        /// The renamed lane-list key.
        case renamedLanes = "lanes"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.version = try c.decode(Int.self, forKey: .version)
        if let renamed = try c.decodeIfPresent([Ticket].self, forKey: .tickets) {
            self.todos = renamed
        } else {
            // Absent on both: `keyNotFound` → the file is invalid → the read fails.
            self.todos = try c.decode([Ticket].self, forKey: .todos)
        }
        if let renamed = try c.decodeIfPresent([String].self, forKey: .renamedLanes) {
            self.lanes = renamed
        } else {
            self.lanes = try c.decodeIfPresent([String].self, forKey: .lanes)
        }
    }
}
