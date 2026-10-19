import Foundation
import Testing
@testable import TandemCore

/// The ticket model (ADR 0017): Status, Archived, description, assignee, and the legacy
/// `done` mirror. Proved at the store's external behaviour — what an operation returns and
/// what the file contains afterwards — never at internal mechanics.
@Suite("Ticket model")
struct TicketModelTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-ticket-\(UUID().uuidString)")
    }

    /// Write a raw store file by hand (older-file and hand-edit fixtures).
    private func writeRaw(_ text: String, to store: Store) throws {
        try FileManager.default.createDirectory(at: store.directory, withIntermediateDirectories: true)
        try text.write(to: store.fileURL, atomically: false, encoding: .utf8)
    }

    /// The store file as a JSON object, for asserting the on-disk shape.
    private func fileObject(_ store: Store) throws -> [String: Any] {
        let data = try Data(contentsOf: store.fileURL)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("test fixture is not a JSON object")
        }
        return object
    }

    /// The first todo object in the store file, as written.
    private func firstTodoObject(_ store: Store) throws -> [String: Any] {
        guard let todos = try fileObject(store)["todos"] as? [[String: Any]],
              let first = todos.first
        else {
            throw StoreIOError.io("test fixture has no todos")
        }
        return first
    }

    /// The on-disk object for the todo with this title.
    private func todoObject(_ store: Store, titled title: String) throws -> [String: Any]? {
        let todos = try fileObject(store)["todos"] as? [[String: Any]] ?? []
        return todos.first { ($0["title"] as? String) == title }
    }

    // MARK: - The fields, end to end

    @Test("a ticket round-trips status, archived, description, assignee and lane")
    func ticketRoundTrips() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Core")
        let added = try store.add("Ship the model", lane: "Core", assignee: "UI").value
        let set = try store.set(
            added.id.uuidString,
            status: .set(.ready),
            description: .set("Three axes, two new fields.")).value

        #expect(set.status == .ready)
        #expect(set.done == false)
        #expect(set.archived == false)
        #expect(set.assignee == "UI")
        #expect(set.description == "Three axes, two new fields.")
        #expect(set.lane == "Core")

        // A fresh Store over the same directory reads the same ticket.
        let read = try #require(try Store(directory: dir).list(.all).value.first)
        #expect(read.status == .ready)
        #expect(read.assignee == "UI")
        #expect(read.description == "Three axes, two new fields.")
        #expect(read.lane == "Core")
        #expect(read.done == false)
    }

    @Test("the on-disk shape keeps version 1 and the section key, and writes every field")
    func onDiskShape() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("Bare")

        let object = try fileObject(store)
        #expect((object["version"] as? Int) == 1)
        #expect(object["sections"] is [Any], "the lane list keeps its v1 key")

        let todo = try firstTodoObject(store)
        #expect((todo["done"] as? Bool) == false)
        #expect((todo["status"] as? String) == "backlog")
        #expect((todo["archived"] as? Bool) == false)
        #expect(todo["description"] is NSNull)
        #expect(todo["assignee"] is NSNull)
        #expect(todo["section"] is NSNull, "the lane is written under its v1 key")
        #expect(todo["lane"] == nil, "the code's word never reaches the file")
    }

    @Test("a file written before the ticket fields existed reads as backlog and unassigned")
    func olderFileReads() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","done":false,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let todo = try #require(try store.list(.all).value.first)
        #expect(todo.status == .backlog)
        #expect(todo.archived == false)
        #expect(todo.assignee == nil)
        #expect(todo.description == nil)
        #expect(todo.lane == nil)
        #expect(try store.list(.all).note == nil, "older is not corrupt")
    }

    // MARK: - The legacy done mirror

    @Test("the mirror wins on read: done true with status backlog reads as done, and heals")
    func mirrorDoneWins() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // What an older writer leaves behind: it completed the ticket, knowing only `done`.
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"finished by the old app","done":true,"status":"backlog","createdAt":"2026-01-01T00:00:00Z","completedAt":"2026-01-02T00:00:00Z"}]}"#, to: store)

        let todo = try #require(try store.list(.all).value.first)
        #expect(todo.status == .done)
        #expect(todo.done == true)
        #expect(try store.list(.done).value.count == 1)

        // The next write persists the reconciled status: the file self-heals.
        _ = try store.add("another")
        let healed = try todoObject(store, titled: "finished by the old app")
        #expect((healed?["status"] as? String) == "done")
        #expect((healed?["done"] as? Bool) == true)
    }

    @Test("done false with status done reads as open and reconciles to backlog on the next write")
    func mirrorOpenWins() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // What an older writer leaves behind: it reopened the ticket, knowing only `done`.
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"reopened by the old app","done":false,"status":"done","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let todo = try #require(try store.list(.all).value.first)
        #expect(todo.status == .backlog)
        #expect(todo.done == false)
        #expect(try store.list(.open).value.count == 1)

        _ = try store.add("another")
        let healed = try todoObject(store, titled: "reopened by the old app")
        #expect((healed?["status"] as? String) == "backlog")
        #expect((healed?["done"] as? Bool) == false)
    }

    @Test("completing writes status done and the legacy mirror in the same file")
    func completeWritesMirror() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task").value
        _ = try store.complete(added.id.uuidString)

        let object = try firstTodoObject(store)
        #expect((object["done"] as? Bool) == true)
        #expect((object["status"] as? String) == "done")
        #expect(object["completedAt"] is String)
    }

    @Test("reopening after done lands in backlog — completion erased the stage")
    func reopenLandsInBacklog() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task", status: .ready).value
        #expect(added.status == .ready, "a ticket can be filed ready")

        _ = try store.complete(added.id.uuidString)
        let reopened = try store.reopen(added.id.uuidString).value
        #expect(reopened.status == .backlog)
        #expect(reopened.done == false)
        #expect(reopened.completedAt == nil)

        // Idempotent: a ticket that is not done is left alone.
        _ = try store.set(added.id.uuidString, status: .set(.refined))
        let untouched = try store.reopen(added.id.uuidString).value
        #expect(untouched.status == .refined)
    }

    // MARK: - set

    @Test("set changes only the fields it names")
    func setChangesOnlyNamedFields() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Core")
        let added = try store.add("Title", lane: "Core", assignee: "UI").value
        _ = try store.set(added.id.uuidString, description: .set("Body"))

        let after = try store.set(added.id.uuidString, status: .set(.inProgress)).value
        #expect(after.title == "Title")
        #expect(after.description == "Body")
        #expect(after.assignee == "UI")
        #expect(after.lane == "Core")
        #expect(after.status == .inProgress)
        #expect(after.archived == false)
    }

    @Test("set clears an optional field with .set(nil) and leaves it with .unchanged")
    func setClearsOptionalFields() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("kept", status: .ready, assignee: "core").value
        _ = try store.set(added.id.uuidString, description: .set("Body"))

        let cleared = try store.set(
            added.id.uuidString,
            assignee: .set(nil),
            description: .set(nil)).value
        #expect(cleared.assignee == nil)
        #expect(cleared.description == nil)
        #expect(cleared.title == "kept", "an unnamed field is untouched")
        #expect(cleared.status == .ready)
    }

    @Test("set trims text, treats an empty value as cleared, and stores human lowercase")
    func setNormalisesText() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("old").value
        let set = try store.set(
            added.id.uuidString,
            title: .set("  New  "),
            assignee: .set("  HUMAN "),
            description: .set("  body\n")).value
        #expect(set.title == "New")
        #expect(set.description == "body")
        #expect(set.assignee == "human")

        let cleared = try store.set(
            added.id.uuidString,
            assignee: .set(""),
            description: .set("  \n ")).value
        #expect(cleared.description == nil)
        #expect(cleared.assignee == nil)

        // An agent's name is stored as given, not lowercased.
        let named = try store.set(added.id.uuidString, assignee: .set("Core-Agent")).value
        #expect(named.assignee == "Core-Agent")
    }

    @Test("set rejects an empty title and leaves the ticket untouched")
    func setRejectsEmptyTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task").value
        for title in ["", "   ", "\t\n  "] {
            #expect(throws: DomainError.emptyTitle) {
                _ = try store.set(added.id.uuidString, title: .set(title))
            }
        }
        #expect(try store.list(.all).value.first?.title == "Task")
    }

    @Test("set with an unknown id throws unknownID carrying the open list")
    func setUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let open = try store.add("A").value
        do {
            _ = try store.set(UUID().uuidString, status: .set(.ready))
            Issue.record("Expected unknownID to be thrown")
        } catch let error as DomainError {
            guard case .unknownID(_, let listed) = error else {
                Issue.record("Expected unknownID, got \(error)")
                return
            }
            #expect(listed.map(\.id) == [open.id])
        }
    }

    @Test("setting status to done stamps completedAt; leaving done clears it")
    func setStatusMaintainsCompletedAt() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task").value
        let done = try store.set(added.id.uuidString, status: .set(.done)).value
        #expect(done.done)
        #expect(done.completedAt != nil)

        let ready = try store.set(added.id.uuidString, status: .set(.ready)).value
        #expect(ready.done == false)
        #expect(ready.completedAt == nil)

        // Free-form: backwards and forwards both work, without a transition table.
        let backlog = try store.set(added.id.uuidString, status: .set(.backlog)).value
        #expect(backlog.status == .backlog)
    }

    // MARK: - archive

    @Test("archiving takes a ticket out of the default views and never deletes it")
    func archiveIsReversible() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task").value
        _ = try store.set(added.id.uuidString, archived: .set(true))

        #expect(try store.list(.open).value.isEmpty)
        #expect(try store.list(.all).value.count == 1)

        let unarchived = try store.set(added.id.uuidString, archived: .set(false)).value
        #expect(unarchived.archived == false)
        #expect(try store.list(.open).value.count == 1)
    }

    // MARK: - add

    @Test("add files a ticket in a Status and with an owner; a title alone stays default")
    func addCarriesTicketFields() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let plain = try store.add("plain").value
        #expect(plain.status == .backlog)
        #expect(plain.assignee == nil)
        #expect(plain.archived == false)
        #expect(plain.done == false)

        let filed = try store.add("filed", status: .ready, assignee: "core").value
        #expect(filed.status == .ready)
        #expect(filed.assignee == "core")
        #expect(filed.done == false)
        #expect(filed.completedAt == nil)
    }
}
