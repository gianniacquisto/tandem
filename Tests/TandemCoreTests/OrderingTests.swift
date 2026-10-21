import Foundation
import Testing
@testable import TandemCore

/// The drag arithmetic: `TicketOrdering.dropIndex` must produce the index that makes the
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
        guard let index = TicketOrdering.dropIndex(in: all, moving: movingID,
                                                relativeTo: anchorID, after: after) else { return false }
        let lane = all.first { $0.id == anchorID }?.lane
        _ = try store.move(movingID.uuidString, toLane: lane, at: index)
        return true
    }

    // MARK: - within a lane

    @Test("dropping on a row's lower half moves the ticket just past it")
    func moveDown() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("c"); _ = try store.add("b"); _ = try store.add("a")
        #expect(try titles(store) == ["a", "b", "c"])

        try drop(store, "a", on: "c", after: true)          // a below c
        #expect(try titles(store) == ["b", "c", "a"])
    }

    @Test("dropping on a row's upper half moves the ticket just above it")
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
        #expect(TicketOrdering.dropIndex(in: all, moving: id, relativeTo: id, after: true) == nil)
        #expect(try titles(store) == ["a", "b"])
    }

    // MARK: - across lanes

    @Test("dropping a default ticket onto a lane row files it into that lane")
    func moveIntoLane() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addLane("Work")
        _ = try store.add("d1")
        _ = try store.add("w1", lane: "Work")
        _ = try store.add("w2", lane: "Work")
        // Stored: default [d1], Work [w2, w1].

        try drop(store, "d1", on: "w1", after: false)       // d1 above w1, in Work
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["w2", "d1", "w1"])
        #expect(all.map(\.lane) == ["Work", "Work", "Work"])
    }

    @Test("dropping a lane ticket onto a default row returns it to the default area")
    func moveOutOfLane() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addLane("Work")
        _ = try store.add("d1")
        _ = try store.add("d2")
        _ = try store.add("w1", lane: "Work")
        // Stored: default [d2, d1], Work [w1].

        try drop(store, "w1", on: "d1", after: true)        // w1 below d1, in default
        let all = try store.list(.all).value
        #expect(all.map(\.title) == ["d2", "d1", "w1"])
        #expect(all.allSatisfy { $0.lane == nil })
    }

    // MARK: - the hidden-row case

    @Test("a done ticket hidden by the Open filter still anchors the drop correctly")
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
