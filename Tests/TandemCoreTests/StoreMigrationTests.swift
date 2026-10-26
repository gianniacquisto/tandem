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
