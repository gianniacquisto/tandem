import Foundation
import Testing
@testable import TandemCore

/// The one-time import and the read-fallback (ADR 0031, ticket 6B96343A P1): a home
/// that still has `tickets.json` (or the legacy `todos.json`) reads it while no database
/// exists, the first write imports it and copies each file aside as
/// `<name>.pre-sqlite-<ts>`, a second write never re-imports, and no post-migration write
/// emits a JSON store file.
@Suite("Store migration and fallback")
struct StoreMigrationTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-migrate-\(UUID().uuidString)")
    }

    private func makeDir() throws -> URL {
        let dir = tempDir()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func entries(in dir: URL) -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
    }

    private func copies(of name: String, in dir: URL) -> [String] {
        entries(in: dir).filter { $0.hasPrefix("\(name).pre-sqlite-") }.sorted()
    }

    private func write(_ text: String, named name: String, to dir: URL) throws {
        try text.write(to: dir.appendingPathComponent(name), atomically: false, encoding: .utf8)
    }

    /// The renamed shape: `tickets`, `lanes`, a ticket's `lane` — what 0.24-era builds
    /// wrote. Two tickets in one lane, in stored order.
    private let renamedFixture = """
        {"version":1,"lanes":["Work"],"tickets":[
        {"id":"00000000-0000-0000-0000-000000000001","title":"first","status":"ready","archived":false,"description":"body one","assignee":"builder","origin":"human","lane":"Work","link":"projects/App.md","createdAt":"2026-01-01T00:00:00Z","completedAt":null},
        {"id":"00000000-0000-0000-0000-000000000002","title":"second","status":"done","archived":false,"description":null,"assignee":null,"origin":"human","lane":null,"link":null,"createdAt":"2026-01-02T00:00:00Z","completedAt":"2026-01-03T00:00:00Z"}]}
        """

    /// The v1 shape: `todos`, `sections`, a ticket's `section`, and the legacy `done`
    /// mirror — what a 0.15-era build left.
    private let v1Fixture = """
        {"version":1,"sections":["Work"],"todos":[
        {"id":"00000000-0000-0000-0000-000000000003","title":"legacy one","done":false,"status":"backlog","section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null},
        {"id":"00000000-0000-0000-0000-000000000004","title":"legacy done","done":true,"status":"backlog","section":null,"createdAt":"2026-01-02T00:00:00Z","completedAt":"2026-01-03T00:00:00Z"}]}
        """

    private func ticket(_ store: Store, titled title: String) throws -> Ticket {
        try #require(try store.list(.all).value.first { $0.title == title })
    }

    // MARK: - Read-fallback

    @Test("a home with no database reads the renamed JSON, and creates nothing")
    func readFallbackRenamed() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        let result = try store.list(.all)
        #expect(result.value.map(\.title) == ["first", "second"])
        #expect(result.note == nil)
        #expect(try store.lanes().value == ["Work"])
        #expect(!FileManager.default.fileExists(atPath: store.databaseURL.path),
                "a read never imports")
        #expect(try copies(of: "tickets.json", in: dir).isEmpty)
        #expect(store.storeFileURL.lastPathComponent == "tickets.json")
    }

    /// TAN-195: schema v4 replaces the retired on-hold spelling in every stored row. One
    /// transaction, one version row: a second open finds the marker and changes nothing.
    @Test("a v4 migration maps the retired spelling to on-hold once, and a second open is a no-op")
    func retiredSpellingMigratesToOnHold() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("tandem.db")

        // A v3 store as the installed build left it: one row still spells the old word.
        let seed = try SQLiteDatabase(url: url, mode: .readWrite)
        try StoreSchema.migrate(
            seed, migrations: StoreSchema.migrations.filter { $0.version <= 3 })
        try seed.run("""
            INSERT INTO tickets (id, title, status, archived, origin, position, createdAt)
            VALUES (?, ?, ?, 0, 'human', 0, ?)
            """, [.text(UUID().uuidString), .text("parked the old way"),
                  .text("refined"), .text(Timestamp.encode(Date()))])
        try seed.run("""
            INSERT INTO store_imports (domain, source, importedAt) VALUES (?, NULL, ?)
            """, [.text(StoreSchema.ticketsDomain), .text(Timestamp.encode(Date()))])
        seed.close()

        // A read of the un-migrated store never falls back to `backlog`.
        let store = Store(directory: dir)
        #expect(try ticket(store, titled: "parked the old way").status == .onHold)

        // The first write migrates: the row on disk carries the live word, and every
        // version is recorded exactly once.
        _ = try store.add("a new one")
        let db = try SQLiteDatabase(url: url, mode: .readOnly)
        defer { db.close() }
        #expect(try db.scalarString(
            "SELECT status FROM tickets WHERE title = 'parked the old way'") == "on-hold")
        let versions = try db.rows("SELECT version FROM schema_migrations ORDER BY version") {
            $0.int(0)
        }
        #expect(versions == Array(1...StoreSchema.currentVersion))

        // Re-running the seam finds nothing to apply.
        let reopen = try SQLiteDatabase(url: url, mode: .readWrite)
        try StoreSchema.migrate(reopen)
        #expect(try reopen.scalarInt("SELECT COUNT(*) FROM schema_migrations")
                == StoreSchema.currentVersion)
        reopen.close()
    }

    @Test("a home with only the legacy todos.json reads it (both shapes, forever)")
    func readFallbackLegacyName() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(v1Fixture, named: "todos.json", to: dir)
        let store = Store(directory: dir)

        let result = try store.list(.all)
        #expect(result.value.map(\.title) == ["legacy one", "legacy done"])
        #expect(result.value[0].lane == "Work")
        #expect(result.value[1].done, "the legacy mirror wins on read, as it always did")
        #expect(!FileManager.default.fileExists(atPath: store.databaseURL.path))
        #expect(store.storeFileURL.lastPathComponent == "todos.json")
    }

    @Test("both files present: tickets.json's contents win, and the read writes nothing")
    func readFallbackPrefersTickets() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        try write(v1Fixture, named: "todos.json", to: dir)
        let before = Set(entries(in: dir))
        let store = Store(directory: dir)

        #expect(try store.list(.all).value.map(\.title) == ["first", "second"])
        #expect(Set(entries(in: dir)) == before, "the read creates and removes nothing")
    }

    @Test("a fresh home is an empty store, never an error, and creates nothing")
    func freshHomeIsEmpty() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        #expect(try store.list(.all).value.isEmpty)
        #expect(try store.lanes().value.isEmpty)
        #expect(store.health() == .absent)
        #expect(try entries(in: dir).isEmpty)
    }

    @Test("a legacy file that cannot be decoded fails the read and is not moved aside")
    func corruptLegacyFailsLoudly() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write("this is not json", named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        #expect(throws: StoreIOError.self) { _ = try store.list(.all) }
        #expect(FileManager.default.fileExists(atPath: dir.appendingPathComponent("tickets.json").path),
                "the unreadable file is left where it is")
        #expect(store.health() == .unreadable)
        #expect(!FileManager.default.fileExists(atPath: store.databaseURL.path))
    }

    // MARK: - The import

    @Test("the first write imports the renamed file: every field reads back, the copy is the evidence")
    func importRenamed() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let before = try Data(contentsOf: dir.appendingPathComponent("tickets.json"))
        let store = Store(directory: dir)

        _ = try store.add("new")

        // The database is the store now, and it holds the imported tickets identically.
        #expect(FileManager.default.fileExists(atPath: store.databaseURL.path))
        let all = try store.list(.all).value
        // The stored order stays grouped by lane, default area first (ADR 0006).
        #expect(all.map(\.title) == ["new", "second", "first"])
        let first = try ticket(store, titled: "first")
        #expect(first.id == UUID(uuidString: "00000000-0000-0000-0000-000000000001"))
        #expect(first.status == .ready)
        #expect(first.description == "body one")
        #expect(first.assignee == "builder")
        #expect(first.origin == "human")
        #expect(first.lane == "Work")
        #expect(first.link == "projects/App.md")
        #expect(Timestamp.encode(first.createdAt) == "2026-01-01T00:00:00Z")
        let second = try ticket(store, titled: "second")
        #expect(second.status == .done)
        #expect(second.completedAt != nil)
        #expect(try store.lanes().value == ["Work"])

        // The file is kept as it was, under the artifact's name.
        let copies = copies(of: "tickets.json", in: dir)
        #expect(copies.count == 1)
        #expect(try Data(contentsOf: dir.appendingPathComponent(copies[0])) == before)
        // And it is not read again.
        #expect(try Data(contentsOf: dir.appendingPathComponent("tickets.json")) == before)
    }

    @Test("the import reads todos.json too, both shapes, and copies that name aside")
    func importLegacyName() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(v1Fixture, named: "todos.json", to: dir)
        let before = try Data(contentsOf: dir.appendingPathComponent("todos.json"))
        let store = Store(directory: dir)

        _ = try store.add("new")

        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["new", "legacy done", "legacy one"])
        #expect(try ticket(store, titled: "legacy one").lane == "Work")
        #expect(try ticket(store, titled: "legacy done").done)
        #expect(try store.lanes().value == ["Work"])

        let copies = copies(of: "todos.json", in: dir)
        #expect(copies.count == 1)
        #expect(try Data(contentsOf: dir.appendingPathComponent(copies[0])) == before)
    }

    @Test("a crash between the migration and the import leaves the JSON readable, then imports it")
    func crashBeforeImportIsNotLost() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        // The state a kill between `migrate` and the import leaves: the database and
        // its schema exist, but the import marker does not and no ticket row is written.
        let db = try SQLiteDatabase(url: store.databaseURL, mode: .readWrite)
        try StoreSchema.migrate(db)
        db.close()
        #expect(store.schemaVersion() == StoreSchema.currentVersion)

        // The database alone is not the store: the JSON is still the truth, so a read
        // answers from it rather than returning an empty board.
        #expect(try store.list(.all).value.map(\.title) == ["first", "second"])
        #expect(store.health() == .counts(open: 1, done: 1))

        // The next write completes the import once, and never leaves the file unread.
        _ = try store.add("new")
        #expect(try store.list(.all).value.map(\.title) == ["new", "second", "first"])
        #expect(try copies(of: "tickets.json", in: dir).count == 1)
        #expect(store.health() == .counts(open: 2, done: 1))
    }

    @Test("a copy-aside that cannot be taken fails the write and imports nothing")
    func failedCopyAsideFailsTheWrite() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        // A live connection keeps the WAL files in place, so the write under test reaches
        // the import rather than failing to open the database in the read-only directory.
        let live = try SQLiteDatabase(url: store.databaseURL, mode: .readWrite)
        try StoreSchema.migrate(live)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o500], ofItemAtPath: dir.path)
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o700], ofItemAtPath: dir.path)
        }

        do {
            _ = try store.add("new")
            Issue.record("expected the copy-aside to fail the write")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("copy the legacy store"), "got \(text)")
        }
        #expect(try copies(of: "tickets.json", in: dir).isEmpty, "a failed copy leaves no artifact")

        // Nothing was imported or marked: the JSON is still the store, and the next
        // write — with the copy possible again — imports it exactly once.
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: dir.path)
        #expect(try store.list(.all).value.map(\.title) == ["first", "second"])
        _ = try store.add("new")
        #expect(try store.list(.all).value.count == 3)
        #expect(try copies(of: "tickets.json", in: dir).count == 1)
        live.close()
    }

    @Test("a rolled-back import keeps its copy as accepted evidence, and the retry adds a suffixed one")
    func rolledBackImportKeepsItsCopy() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        struct Boom: Error {}
        do {
            _ = try store.mutate { _ -> (value: Void, changed: Bool) in throw Boom() }
            Issue.record("expected the caller's mutation to throw")
        } catch {}

        // The copy is taken inside the transaction and nothing can take it back when
        // the rows and marker roll back. Accepted, not silent: it is a byte copy of the
        // file that is still the store, so it records an attempt, never a landed import.
        #expect(try copies(of: "tickets.json", in: dir).count == 1)
        #expect(!store.hasImportedTickets(), "the marker rolled back with the rows")
        #expect(try store.list(.all).value.map(\.title) == ["first", "second"])

        _ = try store.add("new")
        #expect(try copies(of: "tickets.json", in: dir).count == 2,
                "the retry imports once more and writes a second copy")
        #expect(try store.list(.all).value.count == 3)
    }

    @Test("a tickets.json that is a directory fails the read and the write loudly")
    func directoryTicketsFileFailsLoudly() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(
            at: dir.appendingPathComponent("tickets.json"), withIntermediateDirectories: true)
        try write(v1Fixture, named: "todos.json", to: dir)
        let store = Store(directory: dir)

        #expect(throws: StoreIOError.self) { _ = try store.list(.all) }
        #expect(throws: StoreIOError.self) { _ = try store.add("new") }
        var isDirectory: ObjCBool = false
        #expect(FileManager.default.fileExists(
            atPath: dir.appendingPathComponent("tickets.json").path, isDirectory: &isDirectory))
        #expect(isDirectory.boolValue, "the invalid tickets.json is left where it is")
        #expect(store.health() == .unreadable)
    }

    @Test("a second write never re-imports: one copy, no duplicate rows")
    func secondWriteNeverReimports() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        _ = try store.add("new")
        #expect(try copies(of: "tickets.json", in: dir).count == 1)

        _ = try store.add("another")
        _ = try store.setLink(try ticket(store, titled: "new").id.uuidString, to: "x.md")

        #expect(try copies(of: "tickets.json", in: dir).count == 1, "no second import, no second copy")
        #expect(try store.list(.all).value.count == 4, "the rows were not imported twice")
    }

    @Test("a fresh home's write emits no JSON store file, only the database")
    func freshHomeWritesNoJSON() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("first")

        #expect(try entries(in: dir).contains("tandem.db"))
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("tickets.json").path))
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("todos.json").path))
        #expect(try copies(of: "tickets.json", in: dir).isEmpty)
        #expect(try copies(of: "todos.json", in: dir).isEmpty)
    }

    @Test("a renamed file beats the legacy one for the import, as it does for the read")
    func importPrefersTicketsFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        try write(v1Fixture, named: "todos.json", to: dir)
        let legacyBefore = try Data(contentsOf: dir.appendingPathComponent("todos.json"))
        let store = Store(directory: dir)

        _ = try store.add("new")

        #expect(try store.list(.all).value.map(\.title) == ["new", "second", "first"])
        #expect(try copies(of: "tickets.json", in: dir).count == 1)
        #expect(try copies(of: "todos.json", in: dir).isEmpty,
                "tickets.json is the store; the legacy file is a fork and is left alone")
        #expect(try Data(contentsOf: dir.appendingPathComponent("todos.json")) == legacyBefore)
    }

    @Test("the comments directory and the sidecar files are untouched by the ticket import")
    func importLeavesSidecarsAlone() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(renamedFixture, named: "tickets.json", to: dir)
        let comments = dir.appendingPathComponent("comments")
        try FileManager.default.createDirectory(at: comments, withIntermediateDirectories: true)
        let log = comments.appendingPathComponent("00000000-0000-0000-0000-000000000001.jsonl")
        try "{\"author\":\"human\",\"at\":\"2026-01-01T00:00:00Z\",\"body\":\"kept\"}\n"
            .write(to: log, atomically: false, encoding: .utf8)
        try write("{\"version\":1,\"sessions\":[]}", named: "sessions.json", to: dir)

        _ = try Store(directory: dir).add("new")

        #expect(try String(contentsOf: log, encoding: .utf8).contains("kept"))
        #expect(FileManager.default.fileExists(atPath: dir.appendingPathComponent("sessions.json").path))
        #expect(try copies(of: "sessions.json", in: dir).isEmpty)
    }

    @Test("a status the build does not know imports as backlog, never as a failure")
    func unknownStatusImportsAsBacklog() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"from the future","status":"reviewed","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#,
                  named: "todos.json", to: dir)
        let store = Store(directory: dir)

        _ = try store.add("new")

        #expect(try ticket(store, titled: "from the future").status == .backlog)
    }
}
