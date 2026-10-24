import Foundation
import Testing
@testable import TandemCore

/// Stored order + lanes (additive to store schema v1): the array order is the order
/// (never recomputed from `createdAt`), `add` lands on top, lanes group the list,
/// `move` covers both reorder and move-between, and an older file still reads.
@Suite("Order and lanes")
struct OrderAndLanesTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-order-\(UUID().uuidString)")
    }

    /// Write a raw store file by hand (migration / hand-edit fixtures).
    private func writeRaw(_ text: String, to store: Store) throws {
        try FileManager.default.createDirectory(at: store.directory, withIntermediateDirectories: true)
        try text.write(to: store.storeFileURL, atomically: false, encoding: .utf8)
    }

    private func titles(_ store: Store) throws -> [String] {
        try store.list(.all).value.map(\.title)
    }

    // MARK: - stored order

    @Test("a manual order survives a fresh Store — it comes off disk, not from createdAt")
    func storedOrderPersists() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let c = try store.add("c").value.id
        _ = try store.add("b")
        _ = try store.add("a")
        #expect(try titles(store) == ["a", "b", "c"])          // newest on top

        // Move the oldest to the top.
        _ = try store.move(c.uuidString, toLane: nil, at: 0)
        #expect(try titles(store) == ["c", "a", "b"])

        // A fresh Store over the same dir sees the same order.
        let fresh = Store(directory: dir)
        #expect(try titles(fresh) == ["c", "a", "b"])
    }

    @Test("move within the default area reorders; the index clamps to the ends")
    func moveWithinDefault() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("c")
        _ = try store.add("b")
        let a = try store.add("a").value.id
        #expect(try titles(store) == ["a", "b", "c"])

        _ = try store.move(a.uuidString, toLane: nil, at: 0)
        #expect(try titles(store) == ["a", "b", "c"])          // already first

        _ = try store.move(a.uuidString, toLane: nil, at: 99)
        #expect(try titles(store) == ["b", "c", "a"])          // clamped to the end

        _ = try store.move(a.uuidString, toLane: nil, at: -5)
        #expect(try titles(store) == ["a", "b", "c"])          // clamped to the front
    }

    @Test("filter preserves the stored order (all / open / done)")
    func filterPreservesStoredOrder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("a").value.id
        let b = try store.add("b").value.id
        let c = try store.add("c").value.id
        _ = try store.complete(c.uuidString)                   // stored: c(done), b, a

        #expect(try store.list(.all).value.map(\.id) == [c, b, a])
        #expect(try store.list().value.map(\.id) == [b, a])
        #expect(try store.list(.done).value.map(\.id) == [c])
    }

    // MARK: - lanes

    @Test("addLane appends; empty and duplicate names are rejected")
    func addLaneValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Work")
        _ = try store.addLane("  Home  ")                    // trimmed
        #expect(try store.lanes().value == ["Work", "Home"])
        // Each verb returns the lane list after its write, so the CLI reports it
        // without a second read.
        #expect(try store.addLane("Play").value == ["Work", "Home", "Play"])
        #expect(throws: DomainError.emptyLaneName) { _ = try store.addLane("   ") }
        #expect(throws: DomainError.duplicateLane("Work")) { _ = try store.addLane("Work") }
    }

    @Test("add(title, lane:) files on top of that lane; unknown lane is rejected")
    func addIntoLane() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Work")
        _ = try store.add("w1", lane: "Work")
        _ = try store.add("d1")                                 // default area
        _ = try store.add("w2", lane: "Work")

        // Groups in order: default first, then Work; newest on top within a group.
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["d1", "w2", "w1"])
        #expect(all.map(\.lane) == [nil, "Work", "Work"])

        #expect(throws: DomainError.unknownLane("Nope")) {
            _ = try store.add("x", lane: "Nope")
        }
    }

    @Test("renameLane rewrites the lane list and every ticket that belongs to it")
    func renameLane() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Work")
        _ = try store.add("w1", lane: "Work")
        _ = try store.add("d1")
        _ = try store.renameLane("Work", to: "Job")
        #expect(try store.renameLane("Job", to: "Job").value == ["Job"])   // no-op still reports

        #expect(try store.lanes().value == ["Job"])
        let all = try store.list(.all).value
        #expect(all.first { $0.title == "w1" }?.lane == "Job")
        #expect(all.first { $0.title == "d1" }?.lane == nil)

        // Rename to itself is a no-op; the other guards reject.
        #expect(try store.renameLane("Job", to: "Job").note == nil)
        #expect(throws: DomainError.unknownLane("Ghost")) { _ = try store.renameLane("Ghost", to: "X") }
        #expect(throws: DomainError.emptyLaneName) { _ = try store.renameLane("Job", to: " ") }
        _ = try store.addLane("Home")
        #expect(throws: DomainError.duplicateLane("Home")) { _ = try store.renameLane("Job", to: "Home") }
    }

    @Test("deleteLane moves its tickets to the default area and never deletes them")
    func deleteLaneMovesToDefault() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Work")
        _ = try store.add("w1", lane: "Work")
        _ = try store.add("d1")
        _ = try store.add("w2", lane: "Work")

        let moved = try store.deleteLane("Work")
        #expect(moved.value.moved == 2)
        #expect(moved.value.lanes.isEmpty)
        #expect(try store.lanes().value.isEmpty)

        let all = try store.list(.all).value
        #expect(all.count == 3)                                 // nothing deleted
        #expect(all.allSatisfy { $0.lane == nil })
        #expect(all.map(\.title) == ["d1", "w2", "w1"])         // order preserved

        #expect(throws: DomainError.unknownLane("Work")) { _ = try store.deleteLane("Work") }
    }

    @Test("move places a ticket at a 0-based index within another lane")
    func moveBetweenLanes() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Work")
        _ = try store.add("d1")
        _ = try store.add("d2")
        _ = try store.add("w1", lane: "Work")
        _ = try store.add("w2", lane: "Work")
        // Stored: default [d2, d1], then Work [w2, w1].

        let d1 = try store.list(.all).value.first { $0.title == "d1" }!.id
        // Move d1 into Work at index 1 → between w2 and w1.
        _ = try store.move(d1.uuidString, toLane: "Work", at: 1)
        let moved = try store.list(.all).value
        #expect(moved.map(\.title) == ["d2", "w2", "d1", "w1"])
        #expect(moved.map(\.lane) == [nil, "Work", "Work", "Work"])
    }

    @Test("move rejects an unknown id and an unknown lane")
    func moveValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("a").value.id
        #expect(throws: DomainError.unknownLane("Nope")) {
            _ = try store.move(a.uuidString, toLane: "Nope", at: 0)
        }
        do {
            _ = try store.move(UUID().uuidString, toLane: nil, at: 0)
            Issue.record("expected unknownID")
        } catch let error as DomainError {
            guard case .unknownID = error else {
                Issue.record("expected unknownID, got \(error)")
                return
            }
        }
    }

    // MARK: - lane order

    @Test("moveLane reorders the lane list; the index clamps to the ends")
    func moveLaneReorders() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addLane("A")
        _ = try store.addLane("B")
        _ = try store.addLane("C")
        #expect(try store.lanes().value == ["A", "B", "C"])

        #expect(try store.moveLane("B", to: 2).value == ["A", "C", "B"])   // one down
        #expect(try store.moveLane("B", to: 0).value == ["B", "A", "C"])   // to the top
        #expect(try store.moveLane("B", to: 99).value == ["A", "C", "B"])  // clamps to the end

        // It is the stored order, not a UI sort, so it survives a re-read.
        #expect(try Store(directory: dir).lanes().value == ["A", "C", "B"])
    }

    @Test("moveLane rejects an unknown name")
    func moveLaneValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        #expect(throws: DomainError.unknownLane("Nope")) {
            _ = try store.moveLane("Nope", to: 0)
        }
    }

    @Test("moving a lane carries its tickets, and the default area stays first")
    func moveLaneKeepsTickets() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addLane("A")
        _ = try store.addLane("B")
        _ = try store.add("d1")
        _ = try store.add("a1", lane: "A")
        _ = try store.add("b1", lane: "B")
        // Stored: default [d1], A [a1], B [b1].
        #expect(try titles(store) == ["d1", "a1", "b1"])

        _ = try store.moveLane("B", to: 0)
        // B leads the lanes now, but the default area is still the top group.
        #expect(try store.lanes().value == ["B", "A"])
        #expect(try titles(store) == ["d1", "b1", "a1"])
        #expect(try store.list(.all).value.map(\.lane) == [nil, "B", "A"])
    }

    @Test("lanes() reports a dangling name (a hand edit) so the GUI can draw it")
    func danglingLaneReported() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"sections":[],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"x","done":false,"section":"Solo","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        #expect(try store.lanes().value == ["Solo"])
    }

    // MARK: - schema v2 + migration

    @Test("the on-disk array is grouped by lane (default first) after any write")
    func fileStaysGrouped() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // Hand-written file with the Work ticket before the default one.
        try writeRaw(#"{"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"w","done":false,"section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null},{"id":"00000000-0000-0000-0000-000000000002","title":"d","done":false,"section":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        // Any write re-establishes the invariant: default group first.
        _ = try store.add("new")
        #expect(try titles(store) == ["new", "d", "w"])
    }

    @Test("a file from before lanes existed reads, and a write keeps version 1")
    func olderFileReadsAndVersionStays() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","done":false,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all[0].title == "old")
        #expect(all[0].lane == nil)
        #expect(try store.lanes().value.isEmpty)
        #expect(try store.list(.all).note == nil)               // not "corrupt"

        // A write adds the additive fields but must NOT bump the envelope version —
        // a v1-era reader has to keep reading this file (ADR 0006).
        _ = try store.add("new")
        let obj = try JSONSerialization.jsonObject(with: Data(contentsOf: store.storeFileURL)) as? [String: Any]
        #expect((obj?["version"] as? Int) == 1)
        #expect(obj?["lanes"] is [Any])
    }

    @Test("a version-2 file (written during the window the version was bumped) still reads")
    func versionTwoFileStillReads() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":2,"sections":["Solo"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"kept","done":false,"section":"Solo","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all[0].lane == "Solo")
        #expect(try store.lanes().value == ["Solo"])
        #expect(try store.list(.all).note == nil, "a version-2 file is not corrupt")
    }

    // MARK: - The renamed keys, read beside the v1 ones

    /// The store's contents as read from a hand-written file, in a directory of its own.
    private func contents(_ raw: String) throws -> StoreContents {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(raw, to: store)
        return try store.readAll().contents
    }

    /// The store file as a JSON object, for asserting the on-disk shape.
    private func fileObject(_ store: Store) throws -> [String: Any] {
        let data = try Data(contentsOf: store.storeFileURL)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw StoreIOError.io("test fixture is not a JSON object")
        }
        return object
    }

    /// The same ticket in the renamed shape and in the v1 shape — one test, two literals.
    private let renamedShape = #"{"version":1,"lanes":["Work"],"tickets":[{"id":"00000000-0000-0000-0000-000000000001","title":"w","status":"backlog","archived":false,"description":null,"assignee":null,"origin":"human","lane":"Work","link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#
    private let v1Shape = #"{"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"w","status":"backlog","archived":false,"description":null,"assignee":null,"origin":"human","section":"Work","link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#

    @Test("the renamed keys read beside the v1 ones: a renamed file and its v1 twin agree")
    func renamedShapeReadsBesideV1() throws {
        let renamed = try contents(renamedShape)
        let v1 = try contents(v1Shape)

        // The same contents, whichever name the file carries.
        #expect(renamed.lanes == v1.lanes)
        #expect(renamed.todos == v1.todos)
        #expect(renamed.lanes == ["Work"])
        #expect(renamed.todos.first?.lane == "Work")
    }

    @Test("a file carrying both key sets reads the renamed one (a hand edit is not an error)")
    func bothKeySetsReadTheRenamedOne() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"sections":["Old"],"lanes":["New"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","status":"backlog","section":"Old","createdAt":"2026-01-01T00:00:00Z","completedAt":null}],"tickets":[{"id":"00000000-0000-0000-0000-000000000001","title":"new","status":"backlog","lane":"New","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all)
        #expect(all.note == nil, "a file carrying both key sets is not corrupt")
        #expect(all.value.map(\.title) == ["new"])
        #expect(all.value.first?.lane == "New")
        #expect(try store.lanes().value == ["New"])
    }

    // MARK: - The writer's flip: the renamed keys, and the pre-rename copy

    /// The `todos.json.pre-rename-<ts>` copies in a store directory, oldest first.
    private func preRenameCopies(in dir: URL) -> [URL] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        return names.filter { $0.hasPrefix("todos.json.pre-rename-") }.sorted()
            .map { dir.appendingPathComponent($0) }
    }

    /// The `todos.json.pre-migrator-<ts>` copies in a store directory, oldest first.
    private func preMigratorCopies(in dir: URL) -> [URL] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        return names.filter { $0.hasPrefix("todos.json.pre-migrator-") }.sorted()
            .map { dir.appendingPathComponent($0) }
    }

    /// A v1-named store: two tickets, one in a lane, the shape the installed 0.15.0
    /// writer leaves behind.
    private let v1HandoverShape = #"{"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"kept","status":"backlog","section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null},{"id":"00000000-0000-0000-0000-000000000002","title":"default","status":"ready","section":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#

    @Test("the first write over a v1-named file takes both copies, byte-equal to the pre-write file")
    func firstWriteOverV1ShapeCopiesFileAside() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(v1HandoverShape, to: store)
        let before = try Data(contentsOf: store.storeFileURL)

        _ = try store.add("new")

        // The file is one shape now: the renamed keys, the v1 ones gone.
        let object = try fileObject(store)
        #expect((object["version"] as? Int) == 1)
        #expect(object["tickets"] is [Any], "the writer emits the renamed array key")
        #expect(object["lanes"] is [Any], "the writer emits the renamed lane key")
        #expect(object["todos"] == nil, "the v1 array key is gone")
        #expect(object["sections"] == nil, "the v1 lane key is gone")

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.count == 3)
        #expect(tickets.allSatisfy { $0["section"] == nil }, "the v1 per-ticket key is gone")
        #expect(tickets.contains { $0["lane"] as? String == "Work" },
                "the lane survives the round trip under the renamed key")

        // Both copies, one write: the pre-rename snapshot and the rolling .bak, each
        // byte-equal to the file as it was before the write.
        let copies = preRenameCopies(in: dir)
        #expect(copies.count == 1, "the handover takes exactly one pre-rename copy")
        #expect(try Data(contentsOf: try #require(copies.first)) == before)
        #expect(try Data(contentsOf: store.backupURL) == before,
                "the .bak is the state before the write, the one-command recovery")
    }

    @Test("the handover is one generation: a second write takes no second copy and re-rolls the .bak")
    func secondWriteTakesNoSecondCopy() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(v1HandoverShape, to: store)

        _ = try store.add("new")
        let afterFirst = try Data(contentsOf: store.storeFileURL)
        #expect(preRenameCopies(in: dir).count == 1)

        _ = try store.add("one more")

        #expect(preRenameCopies(in: dir).count == 1, "no second pre-rename copy")
        #expect(try Data(contentsOf: store.backupURL) == afterFirst,
                "the .bak is replaced, not rotated")
    }

    @Test("a write over a renamed-shape file emits the renamed keys and takes no copy")
    func writeOverRenamedShapeKeepsRenamed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(renamedShape, to: store)

        _ = try store.add("new", lane: "Work")

        let object = try fileObject(store)
        #expect((object["version"] as? Int) == 1)
        #expect(object["tickets"] is [Any], "the writer emits the renamed array key")
        #expect(object["lanes"] is [Any], "the writer emits the renamed lane key")
        #expect(object["todos"] == nil, "the writer does not emit the v1 array key")
        #expect(object["sections"] == nil, "the writer does not emit the v1 lane key")

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.allSatisfy { $0["section"] == nil },
                "the writer does not emit the v1 per-ticket key")
        #expect(tickets.contains { $0["lane"] as? String == "Work" },
                "the lane survives the round trip under the renamed key")
        #expect(try store.list(.all).value.count == 2)
        // The handover's trigger is the legacy *name*, not the keys: a renamed-key
        // store still named `todos.json` is copied aside (and renamed) all the same.
        #expect(preRenameCopies(in: dir).count == 1,
                "the handover copies todos.json aside whatever keys it carries")
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("todos.json").path),
                "the handover removed the legacy file")
    }

    @Test("a v1-named file that also carries the done mirror takes both copies, one write")
    func v1FileWithMirrorTakesBothCopies() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // The oldest possible shape: v1 names and the legacy mirror at once.
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","done":true,"status":"backlog","createdAt":"2026-01-01T00:00:00Z","completedAt":"2026-01-02T00:00:00Z"}]}"#, to: store)
        let before = try Data(contentsOf: store.storeFileURL)

        _ = try store.add("new")

        // Both safety copies, each byte-equal to the pre-write file.
        let migrator = preMigratorCopies(in: dir)
        #expect(migrator.count == 1)
        #expect(try Data(contentsOf: try #require(migrator.first)) == before)
        let rename = preRenameCopies(in: dir)
        #expect(rename.count == 1)
        #expect(try Data(contentsOf: try #require(rename.first)) == before)
    }
}
