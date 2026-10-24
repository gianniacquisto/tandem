import Foundation
import Testing
@testable import TandemCore

/// The store's handover: the write that renames the store from the legacy
/// `todos.json` to `tickets.json` — copy the old file aside, write the new one,
/// fsync the directory, remove the old one only then, all in one locked act.
/// A fork (both files present) is left alone, a fresh home writes
/// `tickets.json` alone, and a failed copy fails the write while a failed
/// remove never loses it.
@Suite("Store handover")
struct StoreHandoverTests {

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-handover-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func entries(in dir: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: dir.path)
    }

    /// The `todos.json.pre-rename-<ts>` copies in a store directory, oldest first.
    private func preRenameCopies(in dir: URL) throws -> [String] {
        try entries(in: dir).filter { $0.hasPrefix("todos.json.pre-rename-") }.sorted()
    }

    /// A v1-key store: two tickets, one in a lane — the shape the installed
    /// 0.16.0 writer leaves behind.
    private let v1Fixture = #"{"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"kept","status":"backlog","section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null},{"id":"00000000-0000-0000-0000-000000000002","title":"default","status":"ready","section":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#

    /// A renamed-key store, the shape the current writer leaves.
    private let renamedFixture = #"{"version":1,"lanes":[],"tickets":[{"id":"00000000-0000-0000-0000-000000000003","title":"renamed","status":"backlog","lane":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#

    // MARK: - The handover

    @Test("a v1-named store hands over: tickets.json holds every ticket, the copies are byte-equal, todos.json is gone")
    func handover() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try v1Fixture.write(to: dir.appendingPathComponent("todos.json"), atomically: false, encoding: .utf8)
        let before = try Data(contentsOf: dir.appendingPathComponent("todos.json"))

        _ = try store.add("new")

        // The store file is tickets.json, with the renamed keys and every
        // original ticket with its status and lane.
        let object = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("tickets.json"))) as? [String: Any])
        #expect(Set(object.keys) == ["lanes", "tickets", "version"], "keys are \(object.keys.sorted())")
        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.count == 3)
        #expect(tickets.contains { $0["id"] as? String == "00000000-0000-0000-0000-000000000001"
            && $0["status"] as? String == "backlog" && $0["lane"] as? String == "Work" })
        #expect(tickets.contains { $0["id"] as? String == "00000000-0000-0000-0000-000000000002"
            && $0["status"] as? String == "ready" })

        // Both artifacts are byte-equal to the pre-write file: the .bak holds
        // the state before the write (the one-command recovery), the snapshot
        // keeps the name of the file it was taken from.
        #expect(try Data(contentsOf: dir.appendingPathComponent("tickets.json.bak")) == before)
        let copies = try preRenameCopies(in: dir)
        #expect(copies.count == 1)
        #expect(try Data(contentsOf: dir.appendingPathComponent(try #require(copies.first))) == before)

        #expect(!(try entries(in: dir)).contains("todos.json"), "the handover removed the legacy file")
        #expect(try store.list(.all).value.count == 3)
    }

    @Test("the handover is one generation: a second write takes no second copy and re-rolls the .bak")
    func handoverIsOneGeneration() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try v1Fixture.write(to: dir.appendingPathComponent("todos.json"), atomically: false, encoding: .utf8)

        _ = try store.add("new")
        let afterFirst = try Data(contentsOf: dir.appendingPathComponent("tickets.json"))
        #expect(try preRenameCopies(in: dir).count == 1)

        _ = try store.add("one more")

        #expect(try preRenameCopies(in: dir).count == 1, "no second pre-rename copy")
        #expect(try Data(contentsOf: dir.appendingPathComponent("tickets.json.bak")) == afterFirst,
                "the .bak is replaced, not rotated")
        #expect(!(try entries(in: dir)).contains("todos.json"))
    }

    // MARK: - Failure of the steps

    @Test("a failed copy-aside fails the write: the store is unchanged, todos.json untouched, no tickets.json")
    func failedCopyFailsTheWrite() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        var store = Store(directory: dir)
        try v1Fixture.write(to: dir.appendingPathComponent("todos.json"), atomically: false, encoding: .utf8)
        let before = try Data(contentsOf: dir.appendingPathComponent("todos.json"))
        store.copyAsideFile = { _, _ in throw StoreIOError.io("test: the copy failed") }

        #expect(throws: StoreIOError.self) { _ = try store.add("new") }

        #expect(try Data(contentsOf: dir.appendingPathComponent("todos.json")) == before,
                "the legacy file is untouched")
        #expect(!(try entries(in: dir)).contains("tickets.json"), "no store file appeared")
        #expect(!(try entries(in: dir)).contains("tickets.json.bak"), "no backup was rolled")
        #expect(try preRenameCopies(in: dir).isEmpty)
    }

    @Test("a failed remove does not lose the write: the store is written and the op succeeds")
    func failedRemoveKeepsTheWrite() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        var store = Store(directory: dir)
        try v1Fixture.write(to: dir.appendingPathComponent("todos.json"), atomically: false, encoding: .utf8)
        store.removeLegacy = { _ in throw StoreIOError.io("test: the remove failed") }

        let added = try store.add("new")

        // The write succeeded and the store reads back complete.
        let result = try store.list(.all)
        #expect(result.value.count == 3)
        #expect(result.value.contains { $0.id == added.value.id })
        // The fork state: both files present, the legacy one untouched.
        #expect(try entries(in: dir).contains("todos.json"))
        #expect(try entries(in: dir).contains("tickets.json"))
    }

    // MARK: - The fork and the fresh home

    @Test("a fork is left alone: the write lands in tickets.json, todos.json is untouched")
    func forkIsLeftAlone() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try v1Fixture.write(to: dir.appendingPathComponent("todos.json"), atomically: false, encoding: .utf8)
        try renamedFixture.write(to: dir.appendingPathComponent("tickets.json"), atomically: false, encoding: .utf8)
        let legacyBefore = try Data(contentsOf: dir.appendingPathComponent("todos.json"))

        _ = try store.add("another")

        // tickets.json is the store: it holds the two renamed-key tickets plus the new one.
        let object = try #require(try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("tickets.json"))) as? [String: Any])
        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.count == 2)
        // The legacy file is byte-identical and still holds its two tickets.
        #expect(try Data(contentsOf: dir.appendingPathComponent("todos.json")) == legacyBefore)
        let legacy = try #require(try JSONSerialization.jsonObject(with: legacyBefore) as? [String: Any])
        #expect((try #require(legacy["todos"] as? [[String: Any]])).count == 2)
        // No snapshot, no removal: a stray todos.json may be a human's restore.
        #expect(try preRenameCopies(in: dir).isEmpty)
        #expect(!(try entries(in: dir)).contains("todos.json.pre-rename-0"))
    }

    @Test("a fresh home: the first write creates tickets.json alone — no todos.json, no .bak")
    func freshHome() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("first")

        let names = try entries(in: dir)
        #expect(names.contains("tickets.json"))
        #expect(!names.contains("todos.json"))
        #expect(!names.contains("tickets.json.bak"))
        #expect(try store.list(.all).value.count == 1)
    }
}