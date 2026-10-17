import Foundation
import Testing
@testable import TandemCore

/// The drag arithmetic: `TodoOrdering.dropIndex` must produce the index that makes the
/// real `Store.move` land the row where the human dropped it. Each test asserts the
/// *resulting order* after an actual `move`, so the math is pinned to the operation it
/// feeds rather than to itself.
@Suite("Drop ordering")
struct OrderingTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-order-drop-\(UUID().uuidString)")
    }

    private func titles(_ store: Store, _ filter: Filter = .all) throws -> [String] {
        try store.list(filter).value.map(\.title)
    }

    /// Drop `moving` on the `anchor` row (`after` = the lower half) and perform it.
    @discardableResult
    private func drop(_ store: Store, _ moving: String, on anchor: String, after: Bool) throws -> Bool {
        let all = try store.list(.all).value
        guard let movingID = all.first(where: { $0.title == moving })?.id,
              let anchorID = all.first(where: { $0.title == anchor })?.id
        else { return false }
        guard let index = TodoOrdering.dropIndex(in: all, moving: movingID,
                                                relativeTo: anchorID, after: after) else { return false }
        let section = all.first { $0.id == anchorID }?.section
        _ = try store.move(movingID.uuidString, toSection: section, at: index)
        return true
    }

    // MARK: - within a section

    @Test("dropping on a row's lower half moves the todo just past it")
    func moveDown() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("c"); _ = try store.add("b"); _ = try store.add("a")
        #expect(try titles(store) == ["a", "b", "c"])

        try drop(store, "a", on: "c", after: true)          // a below c
        #expect(try titles(store) == ["b", "c", "a"])
    }

    @Test("dropping on a row's upper half moves the todo just above it")
    func moveUp() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("c"); _ = try store.add("b"); _ = try store.add("a")

        try drop(store, "a", on: "c", after: false)         // a directly above c
        #expect(try titles(store) == ["b", "a", "c"])
    }

    @Test("a row dropped on itself is a no-op")
    func dropOnSelf() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("b"); _ = try store.add("a")

        let all = try store.list(.all).value
        let id = all[0].id
        #expect(TodoOrdering.dropIndex(in: all, moving: id, relativeTo: id, after: true) == nil)
        #expect(try titles(store) == ["a", "b"])
    }

    // MARK: - across sections

    @Test("dropping a default todo onto a section row files it into that section")
    func moveIntoSection() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addSection("Work")
        _ = try store.add("d1")
        _ = try store.add("w1", section: "Work")
        _ = try store.add("w2", section: "Work")
        // Stored: default [d1], Work [w2, w1].

        try drop(store, "d1", on: "w1", after: false)       // d1 above w1, in Work
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["w2", "d1", "w1"])
        #expect(all.map(\.section) == ["Work", "Work", "Work"])
    }

    @Test("dropping a section todo onto a default row returns it to the default area")
    func moveOutOfSection() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addSection("Work")
        _ = try store.add("d1")
        _ = try store.add("d2")
        _ = try store.add("w1", section: "Work")
        // Stored: default [d2, d1], Work [w1].

        try drop(store, "w1", on: "d1", after: true)        // w1 below d1, in default
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["d2", "d1", "w1"])
        #expect(all.allSatisfy { $0.section == nil })
    }

    // MARK: - the hidden-row case

    @Test("a done todo hidden by the Open filter still anchors the drop correctly")
    func hiddenRowsDoNotSkewTheDrop() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("c")
        let done = try store.add("b")
        _ = try store.add("a")
        _ = try store.complete(done.value.id.uuidString)
        // Stored (all): [a, b(done), c]; visible under Open: [a, c].

        // The human sees [a, c] and drops a onto c's lower half.
        try drop(store, "a", on: "c", after: true)
        // a lands past c in the *stored* list, so the hidden b keeps its place above.
        #expect(try titles(store) == ["b", "c", "a"])
        #expect(try titles(store, .open) == ["c", "a"])
    }
}
