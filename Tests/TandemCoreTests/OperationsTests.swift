import Foundation
import Testing
@testable import TandemCore

@Suite("Operations")
struct OperationsTests {

    // MARK: - Helpers

    /// A unique, empty temp directory for one store. Callers clean it up with `defer`.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-ops-\(UUID().uuidString)")
    }

    // MARK: - add

    @Test("add trims whitespace and returns a fresh todo")
    func addTrimsWhitespace() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let result = try store.add("   Buy milk  \n")
        let todo = result.value
        #expect(todo.title == "Buy milk")
        #expect(todo.done == false)
        #expect(todo.completedAt == nil)
        #expect(UUID(uuidString: todo.id.uuidString) == todo.id)
        #expect(result.note == nil)
    }

    @Test("add rejects empty and whitespace-only titles")
    func addRejectsEmptyTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        for title in ["", "   ", "\t\n  "] {
            #expect(throws: DomainError.emptyTitle) {
                _ = try store.add(title)
            }
        }
    }

    @Test("add allows duplicate titles")
    func addAllowsDuplicates() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("Same")
        let b = try store.add("Same")
        #expect(a.value.title == b.value.title)
        #expect(a.value.id != b.value.id)
        #expect(try store.list(.all).value.count == 2)
    }

    // MARK: - list

    @Test("list filter variants: default open, done, all")
    func listFilterVariants() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("open-1")
        let done = try store.add("done-1")
        _ = try store.complete(done.value.id.uuidString)

        // Default (no argument) is `.open`.
        let open = try store.list()
        #expect(open.value.count == 1)
        #expect(open.value.allSatisfy { !$0.done })

        let doneList = try store.list(.done)
        #expect(doneList.value.count == 1)
        #expect(doneList.value.allSatisfy { $0.done })

        let all = try store.list(.all)
        #expect(all.value.count == 2)
    }

    @Test("list on an empty store returns an empty array (success, no throw)")
    func listEmptyStore() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let open = try store.list()
        #expect(open.value.isEmpty)
        #expect(open.note == nil)
        #expect(try store.list(.all).value.isEmpty)
    }

    @Test("list rawFilter: nil is open, done/all parse, bogus throws invalidFilter")
    func listRawFilter() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("open-1")
        let done = try store.add("done-1")
        _ = try store.complete(done.value.id.uuidString)

        // nil → default open.
        let nilFilter = try store.list(rawFilter: nil)
        #expect(nilFilter.value.count == 1)
        #expect(nilFilter.value.allSatisfy { !$0.done })

        // "done" and "all" parse.
        let doneFilter = try store.list(rawFilter: "done")
        #expect(doneFilter.value.count == 1)
        #expect(doneFilter.value.allSatisfy { $0.done })
        #expect(try store.list(rawFilter: "all").value.count == 2)

        // "bogus" → invalidFilter, and the message names the bad value.
        do {
            _ = try store.list(rawFilter: "bogus")
            Issue.record("Expected invalidFilter to be thrown")
        } catch let error as DomainError {
            guard case .invalidFilter(let bad) = error else {
                Issue.record("Expected invalidFilter, got \(error)")
                return
            }
            #expect(bad == "bogus")
            #expect(error.message.contains("bogus"))
        }
    }

    // MARK: - complete

    @Test("complete marks done and sets completedAt")
    func completeMarksDone() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        #expect(added.value.completedAt == nil)

        let completed = try store.complete(added.value.id.uuidString)
        #expect(completed.value.id == added.value.id)
        #expect(completed.value.done == true)
        #expect(completed.value.completedAt != nil)
    }

    @Test("complete is idempotent: second call returns the same todo, completedAt unchanged")
    func completeIsIdempotent() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        let first = try store.complete(added.value.id.uuidString)
        #expect(first.value.done == true)
        #expect(first.value.completedAt != nil)

        // The persisted timestamp is the source of truth after the first write
        // (canonical ISO-8601 UTC, whole-second precision on disk).
        let persisted = try store.list(.all).value.first { $0.id == added.value.id }
        #expect(persisted?.completedAt != nil)

        // Second call: same todo, no throw, and `completedAt` is unchanged —
        // it must equal the persisted value, not a freshly stamped time.
        let second = try store.complete(added.value.id.uuidString)
        #expect(second.value.id == first.value.id)
        #expect(second.value.done == true)
        #expect(second.value.completedAt == persisted?.completedAt)
    }

    @Test("complete with an unknown or non-UUID id throws unknownID carrying the open list")
    func completeUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("A")
        let b = try store.add("B")
        _ = try store.complete(b.value.id.uuidString)

        let openIDs = try store.list().value.map(\.id)

        // A valid UUID that isn't in the store.
        do {
            _ = try store.complete(UUID().uuidString)
            Issue.record("Expected unknownID to be thrown for an unknown UUID")
        } catch let error as DomainError {
            guard case .unknownID(_, let open) = error else {
                Issue.record("Expected unknownID, got \(error)")
                return
            }
            #expect(open.map(\.id) == openIDs)
        }

        // A string that isn't a UUID at all.
        do {
            _ = try store.complete("not-a-uuid")
            Issue.record("Expected unknownID to be thrown for a non-UUID string")
        } catch let error as DomainError {
            guard case .unknownID(let id, let open) = error else {
                Issue.record("Expected unknownID, got \(error)")
                return
            }
            #expect(id == "not-a-uuid")
            #expect(open.map(\.id) == openIDs)
        }
    }

    @Test("creation order is preserved in list(.all)")
    func creationOrderPreserved() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let ids = try ["first", "second", "third"].map { try store.add($0).value.id }
        // Completing the middle one must not reorder the list.
        _ = try store.complete(ids[1].uuidString)

        #expect(try store.list(.all).value.map(\.id) == ids)
    }

    // MARK: - update

    @Test("update rewrites the title (trimmed) and leaves done/timestamps/order intact")
    func updateRewritesTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let a = try store.add("first")
        let b = try store.add("old title")
        _ = try store.complete(b.value.id.uuidString)

        let updated = try store.update(b.value.id.uuidString, title: "  new title  \n")
        #expect(updated.value.id == b.value.id)
        #expect(updated.value.title == "new title")
        #expect(updated.value.done == true)
        #expect(updated.value.completedAt != nil)

        let all = try store.list(.all).value
        #expect(all.map(\.id) == [a.value.id, b.value.id])   // order unchanged
        #expect(all[1].title == "new title")
    }

    @Test("update rejects an empty or whitespace-only title")
    func updateRejectsEmptyTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        for title in ["", "   ", "\t\n  "] {
            #expect(throws: DomainError.emptyTitle) {
                _ = try store.update(added.value.id.uuidString, title: title)
            }
        }
        // The title is untouched after the rejected attempts.
        #expect(try store.list(.all).value.first?.title == "Task")
    }

    @Test("update with an unchanged title is a no-op, not an error")
    func updateUnchangedTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Same")
        let updated = try store.update(added.value.id.uuidString, title: "Same")
        #expect(updated.value.id == added.value.id)
        #expect(updated.value.title == "Same")
        #expect(try store.list(.all).value.count == 1)
    }

    @Test("update with an unknown or non-UUID id throws unknownID")
    func updateUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("A")

        for id in [UUID().uuidString, "not-a-uuid"] {
            do {
                _ = try store.update(id, title: "whatever")
                Issue.record("Expected unknownID to be thrown for \(id)")
            } catch let error as DomainError {
                guard case .unknownID = error else {
                    Issue.record("Expected unknownID, got \(error)")
                    return
                }
            }
        }
    }

    // MARK: - reopen

    @Test("reopen clears done and completedAt (the inverse of complete)")
    func reopenMarksOpen() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        let completed = try store.complete(added.value.id.uuidString)
        #expect(completed.value.done == true)
        #expect(completed.value.completedAt != nil)

        let reopened = try store.reopen(added.value.id.uuidString)
        #expect(reopened.value.id == added.value.id)
        #expect(reopened.value.done == false)
        #expect(reopened.value.completedAt == nil)

        // Persisted, not just returned.
        let persisted = try store.list(.all).value.first { $0.id == added.value.id }
        #expect(persisted?.done == false)
        #expect(persisted?.completedAt == nil)
    }

    @Test("reopen is idempotent: an already-open todo is returned unchanged")
    func reopenIsIdempotent() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        let reopened = try store.reopen(added.value.id.uuidString)
        #expect(reopened.value.id == added.value.id)
        #expect(reopened.value.done == false)
        #expect(reopened.value.completedAt == nil)
        #expect(try store.list(.all).value.count == 1)
    }

    @Test("reopen with an unknown or non-UUID id throws unknownID")
    func reopenUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("A")

        for id in [UUID().uuidString, "not-a-uuid"] {
            do {
                _ = try store.reopen(id)
                Issue.record("Expected unknownID to be thrown for \(id)")
            } catch let error as DomainError {
                guard case .unknownID = error else {
                    Issue.record("Expected unknownID, got \(error)")
                    return
                }
            }
        }
    }

    @Test("complete -> reopen -> complete stamps a fresh completedAt without reordering")
    func reopenThenCompleteAgain() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let ids = try ["first", "second", "third"].map { try store.add($0).value.id }
        _ = try store.complete(ids[1].uuidString)
        _ = try store.reopen(ids[1].uuidString)
        #expect(try store.list(.all).value.map(\.id) == ids)
        #expect(try store.list().value.map(\.id) == ids)

        let recompleted = try store.complete(ids[1].uuidString)
        #expect(recompleted.value.done == true)
        #expect(recompleted.value.completedAt != nil)
        #expect(try store.list(.all).value.map(\.id) == ids)
    }

    // MARK: - clearDone

    @Test("clearDone removes all done todos and returns the count removed")
    func clearDoneRemoves() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("open")
        let d1 = try store.add("done-1")
        let d2 = try store.add("done-2")
        _ = try store.complete(d1.value.id.uuidString)
        _ = try store.complete(d2.value.id.uuidString)

        #expect(try store.clearDone().value == 2)

        let all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all.first?.done == false)
    }

    @Test("clearDone with none done returns 0 and leaves open todos intact")
    func clearDoneNone() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("open-1")
        _ = try store.add("open-2")

        #expect(try store.clearDone().value == 0)

        let all = try store.list(.all).value
        #expect(all.count == 2)
        #expect(all.allSatisfy { !$0.done })
    }

    // MARK: - delete

    @Test("delete removes a single todo, open or done")
    func deleteRemoves() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let open = try store.add("open")
        let done = try store.add("done")
        _ = try store.complete(done.value.id.uuidString)

        _ = try store.delete(open.value.id.uuidString)
        var all = try store.list(.all).value
        #expect(all.count == 1)
        #expect(all.first?.id == done.value.id)

        _ = try store.delete(done.value.id.uuidString)
        all = try store.list(.all).value
        #expect(all.isEmpty)
    }

    @Test("delete with an unknown id throws unknownID")
    func deleteUnknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("A")

        do {
            _ = try store.delete(UUID().uuidString)
            Issue.record("Expected unknownID to be thrown")
        } catch let error as DomainError {
            guard case .unknownID = error else {
                Issue.record("Expected unknownID, got \(error)")
                return
            }
        }
    }

    // MARK: - on-disk round-trip

    @Test("on-disk file is schema v1 and a fresh Store decodes the same todos")
    func onDiskRoundTrip() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let open = try store.add("open")
        let done = try store.add("done")
        _ = try store.complete(done.value.id.uuidString)

        // Read the raw file straight off disk.
        let text = try String(contentsOf: store.fileURL, encoding: .utf8)
        let obj = try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        #expect((obj?["version"] as? Int) == 1)

        guard let todos = obj?["todos"] as? [[String: Any]] else {
            Issue.record("`todos` is not a JSON array of objects")
            return
        }
        #expect(todos.count == 2)

        // Pretty-printed: the file is human-legible and contains newlines.
        #expect(text.contains("\n"))

        // Every createdAt is an ISO-8601 UTC string ending in Z.
        for todo in todos {
            #expect((todo["createdAt"] as? String)?.hasSuffix("Z") == true)
        }
        // The open todo's completedAt is JSON null.
        let openEntry = todos.first { ($0["done"] as? Bool) == false }
        #expect(openEntry?["completedAt"] is NSNull)
        // The done todo's completedAt is a real (non-null) string.
        let doneEntry = todos.first { ($0["done"] as? Bool) == true }
        #expect(doneEntry?["completedAt"] is String)

        // Decode round-trip: a fresh Store over the same dir sees the same todos.
        let fresh = Store(directory: dir)
        let all = try fresh.list(.all).value
        #expect(all.map(\.id) == [open.value.id, done.value.id])
        #expect(all.map(\.title) == ["open", "done"])
        #expect(all.map(\.done) == [false, true])
        #expect(all[0].completedAt == nil)
        #expect(all[1].completedAt != nil)
    }
}
