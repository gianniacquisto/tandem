import Foundation
import Testing
@testable import TandemCore

/// Stored order + sections (additive to store schema v1): the array order is the order
/// (never recomputed from `createdAt`), `add` lands on top, sections group the list,
/// `move` covers both reorder and move-between, and an older file still reads.
@Suite("Order and sections")
struct OrderAndSectionsTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-order-\(UUID().uuidString)")
    }

    /// Write a raw store file by hand (migration / hand-edit fixtures).
    private func writeRaw(_ text: String, to store: Store) throws {
        try FileManager.default.createDirectory(at: store.directory, withIntermediateDirectories: true)
        try text.write(to: store.fileURL, atomically: false, encoding: .utf8)
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
        _ = try store.move(c.uuidString, toSection: nil, at: 0)
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

        _ = try store.move(a.uuidString, toSection: nil, at: 0)
        #expect(try titles(store) == ["a", "b", "c"])          // already first

        _ = try store.move(a.uuidString, toSection: nil, at: 99)
        #expect(try titles(store) == ["b", "c", "a"])          // clamped to the end

        _ = try store.move(a.uuidString, toSection: nil, at: -5)
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

    // MARK: - sections

    @Test("addSection appends; empty and duplicate names are rejected")
    func addSectionValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addSection("Work")
        _ = try store.addSection("  Home  ")                    // trimmed
        #expect(try store.sections().value == ["Work", "Home"])
        #expect(throws: DomainError.emptySectionName) { _ = try store.addSection("   ") }
        #expect(throws: DomainError.duplicateSection("Work")) { _ = try store.addSection("Work") }
    }

    @Test("add(title, section:) files on top of that section; unknown section is rejected")
    func addIntoSection() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addSection("Work")
        _ = try store.add("w1", section: "Work")
        _ = try store.add("d1")                                 // default area
        _ = try store.add("w2", section: "Work")

        // Groups in order: default first, then Work; newest on top within a group.
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["d1", "w2", "w1"])
        #expect(all.map(\.section) == [nil, "Work", "Work"])

        #expect(throws: DomainError.unknownSection("Nope")) {
            _ = try store.add("x", section: "Nope")
        }
    }

    @Test("renameSection rewrites the section list and every todo that belongs to it")
    func renameSection() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addSection("Work")
        _ = try store.add("w1", section: "Work")
        _ = try store.add("d1")
        _ = try store.renameSection("Work", to: "Job")

        #expect(try store.sections().value == ["Job"])
        let all = try store.list(.all).value
        #expect(all.first { $0.title == "w1" }?.section == "Job")
        #expect(all.first { $0.title == "d1" }?.section == nil)

        // Rename to itself is a no-op; the other guards reject.
        #expect(try store.renameSection("Job", to: "Job").note == nil)
        #expect(throws: DomainError.unknownSection("Ghost")) { _ = try store.renameSection("Ghost", to: "X") }
        #expect(throws: DomainError.emptySectionName) { _ = try store.renameSection("Job", to: " ") }
        _ = try store.addSection("Home")
        #expect(throws: DomainError.duplicateSection("Home")) { _ = try store.renameSection("Job", to: "Home") }
    }

    @Test("deleteSection moves its todos to the default area and never deletes them")
    func deleteSectionMovesToDefault() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addSection("Work")
        _ = try store.add("w1", section: "Work")
        _ = try store.add("d1")
        _ = try store.add("w2", section: "Work")

        let moved = try store.deleteSection("Work")
        #expect(moved.value == 2)
        #expect(try store.sections().value.isEmpty)

        let all = try store.list(.all).value
        #expect(all.count == 3)                                 // nothing deleted
        #expect(all.allSatisfy { $0.section == nil })
        #expect(all.map(\.title) == ["d1", "w2", "w1"])         // order preserved

        #expect(throws: DomainError.unknownSection("Work")) { _ = try store.deleteSection("Work") }
    }

    @Test("move places a todo at a 0-based index within another section")
    func moveBetweenSections() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addSection("Work")
        _ = try store.add("d1")
        _ = try store.add("d2")
        _ = try store.add("w1", section: "Work")
        _ = try store.add("w2", section: "Work")
        // Stored: default [d2, d1], then Work [w2, w1].

        let d1 = try store.list(.all).value.first { $0.title == "d1" }!.id
        // Move d1 into Work at index 1 → between w2 and w1.
        _ = try store.move(d1.uuidString, toSection: "Work", at: 1)
        let moved = try store.list(.all).value
        #expect(moved.map(\.title) == ["d2", "w2", "d1", "w1"])
        #expect(moved.map(\.section) == [nil, "Work", "Work", "Work"])
    }

    @Test("move rejects an unknown id and an unknown section")
    func moveValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("a").value.id
        #expect(throws: DomainError.unknownSection("Nope")) {
            _ = try store.move(a.uuidString, toSection: "Nope", at: 0)
        }
        do {
            _ = try store.move(UUID().uuidString, toSection: nil, at: 0)
            Issue.record("expected unknownID")
        } catch let error as DomainError {
            guard case .unknownID = error else {
                Issue.record("expected unknownID, got \(error)")
                return
            }
        }
    }

    // MARK: - section order

    @Test("moveSection reorders the section list; the index clamps to the ends")
    func moveSectionReorders() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addSection("A")
        _ = try store.addSection("B")
        _ = try store.addSection("C")
        #expect(try store.sections().value == ["A", "B", "C"])

        #expect(try store.moveSection("B", to: 2).value == ["A", "C", "B"])   // one down
        #expect(try store.moveSection("B", to: 0).value == ["B", "A", "C"])   // to the top
        #expect(try store.moveSection("B", to: 99).value == ["A", "C", "B"])  // clamps to the end

        // It is the stored order, not a UI sort, so it survives a re-read.
        #expect(try Store(directory: dir).sections().value == ["A", "C", "B"])
    }

    @Test("moveSection rejects an unknown name")
    func moveSectionValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        #expect(throws: DomainError.unknownSection("Nope")) {
            _ = try store.moveSection("Nope", to: 0)
        }
    }

    @Test("moving a section carries its todos, and the default area stays first")
    func moveSectionKeepsTodos() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addSection("A")
        _ = try store.addSection("B")
        _ = try store.add("d1")
        _ = try store.add("a1", section: "A")
        _ = try store.add("b1", section: "B")
        // Stored: default [d1], A [a1], B [b1].
        #expect(try titles(store) == ["d1", "a1", "b1"])

        _ = try store.moveSection("B", to: 0)
        // B leads the sections now, but the default area is still the top group.
        #expect(try store.sections().value == ["B", "A"])
        #expect(try titles(store) == ["d1", "b1", "a1"])
        #expect(try store.list(.all).value.map(\.section) == [nil, "B", "A"])
    }

    @Test("sections() reports a dangling name (a hand edit) so the GUI can draw it")
    func danglingSectionReported() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"sections":[],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"x","done":false,"section":"Solo","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        #expect(try store.sections().value == ["Solo"])
    }

    // MARK: - schema v2 + migration

    @Test("the on-disk array is grouped by section (default first) after any write")
    func fileStaysGrouped() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // Hand-written file with the Work todo before the default one.
        try writeRaw(#"{"version":1,"sections":["Work"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"w","done":false,"section":"Work","createdAt":"2026-01-01T00:00:00Z","completedAt":null},{"id":"00000000-0000-0000-0000-000000000002","title":"d","done":false,"section":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        // Any write re-establishes the invariant: default group first.
        _ = try store.add("new")
        #expect(try titles(store) == ["new", "d", "w"])
    }

    @Test("a file from before sections existed reads, and a write keeps version 1")
    func olderFileReadsAndVersionStays() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"old","done":false,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all[0].title == "old")
        #expect(all[0].section == nil)
        #expect(try store.sections().value.isEmpty)
        #expect(try store.list(.all).note == nil)               // not "corrupt"

        // A write adds the additive fields but must NOT bump the envelope version —
        // a v1-era reader has to keep reading this file (ADR 0006).
        _ = try store.add("new")
        let obj = try JSONSerialization.jsonObject(with: Data(contentsOf: store.fileURL)) as? [String: Any]
        #expect((obj?["version"] as? Int) == 1)
        #expect(obj?["sections"] is [Any])
    }

    @Test("a version-2 file (written during the window the version was bumped) still reads")
    func versionTwoFileStillReads() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":2,"sections":["Solo"],"todos":[{"id":"00000000-0000-0000-0000-000000000001","title":"kept","done":false,"section":"Solo","createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}"#, to: store)

        let all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all[0].section == "Solo")
        #expect(try store.sections().value == ["Solo"])
        #expect(try store.list(.all).note == nil, "a version-2 file is not corrupt")
    }
}
