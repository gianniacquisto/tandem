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
/// - **Import on the first write.** When a write finds no database and a legacy file,
///   it reads the file once, copies it aside as `<name>.pre-sqlite-<unix-ts>` (a copy
///   that cannot be taken fails the write), and writes the database. After that the
///   JSON file is never read for a write again — a second write never re-imports.
///
/// The import is install-gated in ADR 0017's sense: the trigger is a build that carries
/// the database reaching the home, not a flag or a click.
enum StoreMigration {
    /// The JSON store file the fallback would read: `tickets.json` when it is a regular
    /// file, else `todos.json` when that is. A present-but-not-a-regular-file
    /// `tickets.json` is invalid — the old name is never a fall-through for a broken new
    /// one.
    static func legacyStore(in directory: URL) throws -> URL? {
        let fm = FileManager.default
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
            return StoreContents(lanes: envelope.lanes ?? [], todos: envelope.todos)
        } catch {
            throw StoreIOError.io(
                "the legacy store at \(url.path) is corrupt (\(error)); "
                + "move it aside and retry — the store is not rewritten")
        }
    }

    /// The first write's import: read the JSON, copy it aside, write the rows. A home
    /// with no legacy file is simply a fresh home — nothing is written here.
    static func importLegacyStoreIfPresent(in directory: URL, into db: SQLiteDatabase) throws {
        guard let url = try legacyStore(in: directory) else { return }
        let contents = try read(url)
        try copyAside(url)
        try db.transaction {
            try StoreSchema.writeContents(contents, to: db)
        }
    }

    // MARK: - Private

    /// Copy the legacy file aside as `<name>.pre-sqlite-<unix-ts>`, colliding names
    /// suffixed. The bytes as they were: the file is the evidence the import is true,
    /// and a copy that cannot be taken fails the write rather than being skipped.
    private static func copyAside(_ url: URL) throws {
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
