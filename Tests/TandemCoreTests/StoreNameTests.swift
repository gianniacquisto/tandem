import Foundation
import Testing
@testable import TandemCore

/// The store's file-name resolution: `tickets.json` or the legacy `todos.json`.
/// The reader reads either name, and the write goes to the name it read from.
/// A fresh home (neither present) writes `todos.json`.
@Suite("Store name resolution")
struct StoreNameTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-store-name-\(UUID().uuidString)")
    }

    private func makeDir() throws -> URL {
        let dir = tempDir()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func entries(in dir: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: dir.path)
    }

    private func write(_ text: String, named name: String, to dir: URL) throws {
        try text.write(to: dir.appendingPathComponent(name), atomically: false, encoding: .utf8)
    }

    /// A new-key fixture: `tickets` + `lanes`, the shape the current writer leaves.
    private let newKeyFixture = """
        {"version":1,"lanes":[],"tickets":[{"id":"00000000-0000-0000-0000-000000000001","title":"new-name","status":"backlog","lane":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}
        """

    /// A v1-key fixture: `todos` + `sections` + a ticket's `section`, the shape the
    /// installed 0.15.0 writer leaves behind.
    private let v1KeyFixture = """
        {"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000002","title":"legacy-name","status":"backlog","section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}
        """

    // MARK: - Resolution

    @Test("only tickets.json: its tickets are listed")
    func onlyTicketsFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(newKeyFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        let result = try store.list()
        #expect(result.value.map(\.title) == ["new-name"])
        #expect(result.note == nil)
        #expect(store.storeFileURL.lastPathComponent == "tickets.json")
    }

    @Test("only todos.json: its tickets are listed")
    func onlyTodosFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(v1KeyFixture, named: "todos.json", to: dir)
        let store = Store(directory: dir)

        let result = try store.list()
        #expect(result.value.map(\.title) == ["legacy-name"])
        #expect(result.note == nil)
        #expect(store.storeFileURL.lastPathComponent == "todos.json")
    }

    @Test("both files: tickets.json's contents win, and the read creates and removes nothing")
    func bothFiles() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(newKeyFixture, named: "tickets.json", to: dir)
        try write(v1KeyFixture, named: "todos.json", to: dir)
        let before = Set(try entries(in: dir))
        let store = Store(directory: dir)

        let result = try store.list()
        #expect(result.value.map(\.title) == ["new-name"])
        #expect(result.note == nil)
        #expect(store.storeFileURL.lastPathComponent == "tickets.json")

        let after = Set(try entries(in: dir))
        #expect(after == before, "the read creates and removes nothing")
    }

    @Test("neither file: empty list, no note, the directory is unchanged")
    func neitherFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let result = try store.list()
        #expect(result.value.isEmpty)
        #expect(result.note == nil)
        #expect(try entries(in: dir).isEmpty, "the read creates and removes nothing")

        let noOp = try store.mutate { contents -> (value: Int, changed: Bool) in
            (value: 0, changed: false)
        }
        #expect(noOp.value == 0)
        #expect(noOp.note == nil)
        // The no-op mutate writes nothing: the sidecar lock is the protocol's own
        // entry, and no store file of either name appears.
        #expect(try entries(in: dir) == ["tandem.lock"])
    }

    @Test("a tickets.json that is a directory: invalid, todos.json beside it is not read")
    func directoryTicketsFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(
            at: dir.appendingPathComponent("tickets.json"),
            withIntermediateDirectories: true)
        try write(v1KeyFixture, named: "todos.json", to: dir)
        let store = Store(directory: dir)

        let result = try store.list()
        let backups = try entries(in: dir).filter { $0.hasPrefix("tickets.json.corrupt-") }
        let backup = try #require(backups.first, "the directory is moved aside as a corrupt copy")
        #expect(result.note?.contains(backup) == true, "the note names the backup")
        #expect(result.value.isEmpty, "todos.json beside it is not read")
    }

    // MARK: - Write-back

    @Test("todo add against a tickets.json-only store leaves two tickets in tickets.json and no todos.json")
    func writeBackToTicketsFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(newKeyFixture, named: "tickets.json", to: dir)
        let store = Store(directory: dir)

        _ = try store.add("second")
        let names = try entries(in: dir)
        #expect(names.contains("tickets.json"))
        #expect(!names.contains("todos.json"))
        let result = try store.list()
        #expect(result.value.count == 2)
    }

    @Test("todo add against a todos.json-only store leaves two tickets in todos.json and no tickets.json")
    func writeBackToTodosFile() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try write(v1KeyFixture, named: "todos.json", to: dir)
        let store = Store(directory: dir)

        _ = try store.add("second")
        let names = try entries(in: dir)
        #expect(names.contains("todos.json"))
        #expect(!names.contains("tickets.json"))
        let result = try store.list()
        #expect(result.value.count == 2)
    }

    @Test("todo add against an empty dir creates todos.json alone")
    func writeBackToFreshHome() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("first")
        let names = try entries(in: dir)
        #expect(names.contains("todos.json"))
        #expect(!names.contains("tickets.json"))
        let result = try store.list()
        #expect(result.value.count == 1)
    }
}
