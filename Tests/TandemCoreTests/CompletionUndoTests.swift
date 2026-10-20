import Foundation
import Testing
@testable import TandemCore

@Suite("Completion undo")
struct CompletionUndoTests {

    // MARK: - Helpers

    /// A unique, empty temp directory for one store. Callers clean it up with `defer`.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-undo-\(UUID().uuidString)")
    }

    // MARK: - complete → undo

    @Test("complete → undo puts the ticket back in backlog, with no completedAt")
    func completeThenUndo() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Buy milk")
        let offer = CompletionUndo(id: added.value.id, title: added.value.title)

        let completed = try store.complete(offer.id.uuidString)
        #expect(completed.value.done == true)
        #expect(completed.value.completedAt != nil)

        let undone = try store.undo(offer)
        #expect(undone.value.id == added.value.id)
        #expect(undone.value.status == .backlog)
        #expect(undone.value.done == false)
        #expect(undone.value.completedAt == nil)

        // Persisted, not just returned: the ticket is open again, so it is back
        // in the list (and in the board's Backlog column).
        let persisted = try store.list(.all).value.first { $0.id == added.value.id }
        #expect(persisted?.status == .backlog)
        #expect(persisted?.completedAt == nil)
        #expect(try store.list().value.map(\.id) == [added.value.id])
        #expect(try store.list(.done).value.isEmpty)
    }

    @Test("undo lands in backlog whatever stage the ticket came from")
    func undoLandsInBacklog() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        _ = try store.set(added.value.id.uuidString, status: .set(.ready))
        let offer = CompletionUndo(id: added.value.id, title: added.value.title)

        _ = try store.complete(offer.id.uuidString)
        let undone = try store.undo(offer)
        #expect(undone.value.status == .backlog)
    }

    @Test("undo is idempotent, and does nothing to a ticket that is not done")
    func undoIsIdempotent() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        let offer = CompletionUndo(id: added.value.id, title: added.value.title)

        // Never completed: the undo has nothing to take back.
        let untouched = try store.undo(offer)
        #expect(untouched.value.status == .backlog)
        #expect(untouched.value.completedAt == nil)

        _ = try store.complete(offer.id.uuidString)
        let undone = try store.undo(offer)
        #expect(undone.value.status == .backlog)

        // A second undo changes nothing.
        let again = try store.undo(offer)
        #expect(again.value.status == .backlog)
        #expect(again.value.completedAt == nil)
        #expect(try store.list(.all).value.count == 1)
    }

    @Test("undo with an unknown id throws unknownID")
    func undoUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let offer = CompletionUndo(id: UUID(), title: "Ghost")
        do {
            _ = try store.undo(offer)
            Issue.record("Expected unknownID to be thrown")
        } catch let error as DomainError {
            guard case .unknownID(let id, _) = error else {
                Issue.record("Expected unknownID, got \(error)")
                return
            }
            #expect(id == offer.id.uuidString)
        }
    }

    // MARK: - the offer

    @Test("the offer runs out after six seconds")
    func offerExpires() {
        let start = Date(timeIntervalSince1970: 1_000)
        let offer = CompletionUndo(id: UUID(), title: "Task", at: start)

        #expect(CompletionUndo.window == 6)
        #expect(offer.expiresAt == Date(timeIntervalSince1970: 1_006))
        #expect(offer.isLive(at: start))
        #expect(offer.isLive(at: Date(timeIntervalSince1970: 1_005)))
        #expect(!offer.isLive(at: Date(timeIntervalSince1970: 1_006)))
    }
}
