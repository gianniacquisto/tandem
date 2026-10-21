import Foundation
import Testing
@testable import TandemCore

/// The ticket model (ADR 0017, origin ADR 0020): Status, Archived, description, assignee,
/// origin, and the legacy `done` mirror. Proved at the store's external behaviour — what
/// an operation returns and what the file contains afterwards — never at internal
/// mechanics.
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

    /// The first ticket object in the store file, as written.
    private func firstTicketObject(_ store: Store) throws -> [String: Any] {
        guard let todos = try fileObject(store)["todos"] as? [[String: Any]],
              let first = todos.first
        else {
            throw StoreIOError.io("test fixture has no todos")
        }
        return first
    }

    /// The on-disk object for the ticket with this title.
    private func todoObject(_ store: Store, titled title: String) throws -> [String: Any]? {
        let todos = try fileObject(store)["todos"] as? [[String: Any]] ?? []
        return todos.first { ($0["title"] as? String) == title }
    }

    /// The migrator's safety copies, oldest first.
    private func preMigratorBackups(in dir: URL) -> [URL] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        return names.filter { $0.hasPrefix("todos.json.pre-migrator-") }.sorted()
            .map { dir.appendingPathComponent($0) }
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

    @Test("in-review round-trips through the store and the file")
    func inReviewRoundTrips() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Waiting for review", status: .inReview).value
        #expect(added.status == .inReview)
        #expect(added.done == false)
        #expect((try todoObject(store, titled: "Waiting for review")?["status"] as? String) == "in-review")

        // A fresh Store over the same directory reads the same stage.
        let read = try #require(try Store(directory: dir).list(.all).value.first)
        #expect(read.status == .inReview)
        #expect(read.done == false)
    }

    @Test("an unknown status still decodes as backlog, never as a corrupt file")
    func unknownStatusReadsAsBacklog() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // A value from a later build than this one — the compatibility rule.
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"from the future","status":"reviewed","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let todo = try #require(try store.list(.all).value.first)
        #expect(todo.status == .backlog)
        #expect(try store.list(.all).note == nil, "an unknown stage is not corruption")
    }

    @Test("workflow order puts in-review between in-progress and done")
    func workflowOrder() {
        #expect(Status.workflow == [.backlog, .refined, .ready, .inProgress, .inReview, .done])
        #expect(Status.wireValues == ["backlog", "refined", "ready", "in-progress", "in-review", "done"])
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

        let todo = try firstTicketObject(store)
        #expect(todo["done"] == nil, "the legacy mirror retired from the file")
        #expect((todo["status"] as? String) == "backlog")
        #expect((todo["archived"] as? Bool) == false)
        #expect(todo["description"] is NSNull)
        #expect(todo["assignee"] is NSNull)
        #expect((todo["origin"] as? String) == "human", "the filer is never null")
        #expect(todo["section"] is NSNull, "the lane is written under its v1 key")
        #expect(todo["lane"] == nil, "the code's word never reaches the file")
    }

    @Test("a file written before the ticket fields existed reads as backlog and unassigned")
    func olderFileReads() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","done":false,"createdAt":"2026-01-01T00:00:00Z","completedAt":null},{"id":"00000000-0000-0000-0000-000000000002","title":"finished by the old app","done":true,"createdAt":"2026-01-01T00:00:00Z","completedAt":"2026-01-02T00:00:00Z"}]}"#, to: store)

        let todo = try #require(try store.list(.all).value.first { $0.title == "old" })
        #expect(todo.status == .backlog)
        #expect(todo.archived == false)
        #expect(todo.assignee == nil)
        #expect(todo.description == nil)
        #expect(todo.lane == nil)

        // `done` true with no `status` key at all is the plain upgrade case: still done.
        let finished = try #require(try store.list(.all).value.first { $0.title == "finished by the old app" })
        #expect(finished.status == .done)
        #expect(finished.done == true)

        #expect(try store.list(.all).note == nil, "older is not corrupt")
    }

    // MARK: - The legacy done mirror

    @Test("the mirror wins on read: done true with status backlog reads as done, and migrates")
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

        // Reading never migrates: the file keeps its old shape, and nothing is copied.
        #expect(try todoObject(store, titled: "finished by the old app")?["done"] as? Bool == true)
        #expect(preMigratorBackups(in: dir).isEmpty)

        // The next write persists the reconciled status, retires the mirror, and copies
        // the old file aside first.
        _ = try store.add("another")
        let healed = try todoObject(store, titled: "finished by the old app")
        #expect((healed?["status"] as? String) == "done")
        #expect(healed?["done"] == nil, "the migrated file carries status alone")

        let backups = preMigratorBackups(in: dir)
        #expect(backups.count == 1)
        let backedUp = try Data(contentsOf: try #require(backups.first))
        let object = try JSONSerialization.jsonObject(with: backedUp) as? [String: Any]
        let first = (object?["todos"] as? [[String: Any]])?.first
        #expect(first?["done"] as? Bool == true, "the copy keeps the pre-migration shape")
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
        #expect(healed?["done"] == nil)
        #expect(preMigratorBackups(in: dir).count == 1)
    }

    @Test("a file written after the migrator has no done key and reads from status alone")
    func postMigratorFileReads() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"finished by the new app","status":"done","archived":false,"description":null,"assignee":null,"section":null,"link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":"2026-01-02T00:00:00Z"},{"id":"00000000-0000-0000-0000-000000000002","title":"ready by the new app","status":"ready","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all).value
        #expect(all.first { $0.title == "finished by the new app" }?.status == .done)
        #expect(all.first { $0.title == "ready by the new app" }?.status == .ready)

        // A write leaves the shape alone and makes no migrator backup: there is nothing old.
        _ = try store.add("another")
        #expect(preMigratorBackups(in: dir).isEmpty)
        #expect(try todoObject(store, titled: "finished by the new app")?["done"] == nil)
    }

    @Test("the CLI wire keeps the derived done flag even though the file drops it")
    func wireKeepsDone() throws {
        let todo = Ticket(title: "done", status: .done)
        let text = try StoreEncoding.ticketResult(todo, note: nil)
        let object = try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        #expect(object?["done"] as? Bool == true, "agents read done from the wire")
        #expect(object?["status"] as? String == "done")
    }

    // MARK: - origin

    @Test("a ticket's origin round-trips through the store")
    func originRoundTrips() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Filed by an agent", origin: "blue").value
        #expect(added.origin == "blue")

        // A fresh Store over the same directory reads the same filer.
        let read = try #require(try Store(directory: dir).list(.all).value.first)
        #expect(read.origin == "blue")
    }

    @Test("an absent origin reads as human, and the next write stamps it")
    func originAbsentReadsHuman() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // A file written before the field existed: the whole shape but no `origin` key.
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"predates the field","status":"backlog","archived":false,"description":null,"assignee":null,"section":null,"link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let old = try #require(try store.list(.all).value.first)
        #expect(old.origin == "human", "absent reads as the filer every old ticket has")
        #expect(try todoObject(store, titled: "predates the field")?["origin"] == nil,
               "a read never writes: the file keeps its old shape")

        // The next write stamps the filer on the pre-existing ticket — a reconciliation,
        // not a recovery: no file is copied aside.
        _ = try store.add("another")
        #expect((try todoObject(store, titled: "predates the field")?["origin"] as? String) == "human")
        #expect(preMigratorBackups(in: dir).isEmpty)
    }

    @Test("add stamps its origin, defaulting to human, with the assignee's spelling rules")
    func addStampsOrigin() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        #expect(try store.add("default").value.origin == "human")
        #expect(try store.add("empty", origin: "   ").value.origin == "human")
        #expect(try store.add("reserved", origin: "  HUMAN ").value.origin == "human")
        // An agent's name is stored as given, not lowercased.
        #expect(try store.add("named", origin: " Blue Agent ").value.origin == "Blue Agent")
    }

    @Test("origin is written once, at add: set, move, place, complete and reopen leave it")
    func originIsImmutable() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Core")
        let added = try store.add("Task", assignee: "core", origin: "blue").value

        var after = try store.set(
            added.id.uuidString,
            title: .set("Renamed"),
            status: .set(.inProgress),
            assignee: .set("human"),
            description: .set("Body"),
            archived: .set(true)).value
        #expect(after.origin == "blue", "set has no origin to change")

        after = try store.move(added.id.uuidString, toLane: "Core", at: 0).value
        #expect(after.origin == "blue")

        after = try store.place(added.id.uuidString, lane: "Core", status: .done).value
        #expect(after.origin == "blue")

        #expect(try store.reopen(added.id.uuidString).value.origin == "blue")
    }

    @Test("the CLI wire carries origin, present and never null")
    func wireCarriesOrigin() throws {
        let filed = Ticket(title: "filed", origin: "blue")
        let human = Ticket(title: "filed by a human")
        for (todo, expected) in [(filed, "blue"), (human, "human")] {
            let text = try StoreEncoding.ticketResult(todo, note: nil)
            let object = try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
            #expect(object?["origin"] as? String == expected)
        }
    }

    @Test("completing writes status done, completedAt, and no legacy mirror")
    func completeWritesStatusOnly() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task").value
        _ = try store.complete(added.id.uuidString)

        let object = try firstTicketObject(store)
        #expect(object["done"] == nil)
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
        #expect(plain.origin == "human")
        #expect(plain.archived == false)
        #expect(plain.done == false)

        let filed = try store.add("filed", status: .ready, assignee: "core").value
        #expect(filed.status == .ready)
        #expect(filed.assignee == "core")
        #expect(filed.done == false)
        #expect(filed.completedAt == nil)
    }
}
