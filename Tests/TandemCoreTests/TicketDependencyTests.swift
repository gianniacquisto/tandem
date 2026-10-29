import Foundation
import Testing
@testable import TandemCore

/// Ticket dependencies (TAN-232): an edge in `ticket_dependencies` holds the UUID of the
/// ticket a ticket waits on, the derived `blocked`/`blockedBy`/`blocks` triple reads it
/// from both ends, and `Store.block`/`unblock` are the only writes. Proved through
/// `Store`'s public API, as `TicketLinkTests` proves the link.
@Suite("Ticket dependencies")
struct TicketDependencyTests {

    // MARK: - Helpers

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-deps-\(UUID().uuidString)")
    }

    /// A store with one lane (`Tandem`, so every ticket carries a ref) and the tickets
    /// `titles` in that lane, in add order. Returned by title.
    private func store(
        _ titles: [String], at dir: URL
    ) throws -> (Store, [String: Ticket]) {
        let store = Store(directory: dir)
        _ = try store.addLane("Tandem")
        var made: [String: Ticket] = [:]
        // Added in the order given, so `titles[0]` carries `TAN-1`, the number follows the
        // write, not the row's place on the board.
        for title in titles {
            made[title] = try store.add(title, lane: "Tandem").value
        }
        return (store, made)
    }

    private func message(_ body: () throws -> Void) -> String? {
        do {
            try body()
            return nil
        } catch let error as DomainError {
            return error.message
        } catch {
            return "\(error)"
        }
    }

    // MARK: - The write

    @Test("block writes one edge and it survives a re-read")
    func blockSurvivesReRead() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)

        let result = try store.block(made["waits"]!.ref!, on: made["finishes"]!.ref!)
        #expect(result.value.blocked)
        #expect(result.value.blockedBy.map(\.ref) == ["TAN-2"])
        #expect(result.value.blockedBy.map(\.title) == ["finishes"])
        #expect(result.value.blockedBy.map(\.status) == [.backlog])

        // A fresh Store value reads the edge from the database, not from the result.
        let reread = try Store(directory: dir).ticket(id: made["waits"]!.ref!)
        #expect(reread.value.blocked)
        #expect(reread.value.blockedBy.count == 1)
        #expect(reread.value.blockedBy.first?.id == made["finishes"]!.id)
    }

    @Test("block twice writes one edge and is a no-op")
    func blockTwiceIsNoOp() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)
        let waits = made["waits"]!.ref!
        let finishes = made["finishes"]!.ref!

        _ = try store.block(waits, on: finishes)
        let database = store.databaseURL
        let before = try FileManager.default.attributesOfItem(atPath: database.path)[.modificationDate]
            as? Date
        #expect(before != nil)

        let again = try store.block(waits, on: finishes)
        #expect(again.value.blockedBy.count == 1, "one edge, not two")

        let after = try FileManager.default.attributesOfItem(atPath: database.path)[.modificationDate]
            as? Date
        #expect(after == before, "a no-op write writes nothing")
    }

    @Test("a ticket may block on two, and blockedBy keeps the order they were set in")
    func twoEdgesKeepTheirOrder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "first", "second"], at: dir)
        let waits = made["waits"]!.ref!

        _ = try store.block(waits, on: made["first"]!.ref!)
        let two = try store.block(waits, on: made["second"]!.ref!)
        #expect(two.value.blockedBy.map(\.ref) == ["TAN-2", "TAN-3"])

        // The order is the write's, and a re-read agrees.
        #expect(try store.ticket(id: waits).value.blockedBy.map(\.ref) == ["TAN-2", "TAN-3"])
    }

    @Test("block accepts a ref and a UUID for the same blocker")
    func refAndUUIDAreOneEdge() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)
        let blocker = made["finishes"]!

        _ = try store.block(made["waits"]!.ref!, on: blocker.id.uuidString)
        #expect(try store.ticket(id: made["waits"]!.id.uuidString).value.blockedBy.count == 1)

        // The other spelling names the same edge, so the second write is the no-op.
        let same = try store.block(made["waits"]!.ref!, on: blocker.ref!)
        #expect(same.value.blockedBy.map(\.id) == [blocker.id])
    }

    // MARK: - The refusals

    @Test("a ticket cannot block on itself")
    func selfIsRefused() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["alone"], at: dir)
        let ref = made["alone"]!.ref!

        #expect(message { _ = try store.block(ref, on: ref) }
            == "TAN-1 cannot block on itself. Name the ticket that has to finish first.")
        #expect(try store.ticket(id: ref).value.blockedBy.isEmpty, "nothing was written")
    }

    @Test("an edge that would close a cycle is refused and names the path")
    func cycleIsRefused() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["a", "b", "c"], at: dir)
        let a = made["a"]!.ref!, b = made["b"]!.ref!, c = made["c"]!.ref!

        _ = try store.block(a, on: b)
        _ = try store.block(b, on: c)

        #expect(message { _ = try store.block(c, on: a) }
            == "That edge would close a cycle: TAN-3 -> TAN-1 -> TAN-2 -> TAN-3. "
             + "Two tickets cannot wait on each other; unblock one of them first.")
        #expect(try store.ticket(id: c).value.blockedBy.isEmpty, "nothing was written")
    }

    @Test("a done blocker is refused")
    func doneBlockerIsRefused() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finished"], at: dir)
        _ = try store.complete(made["finished"]!.ref!)

        #expect(message { _ = try store.block(made["waits"]!.ref!, on: made["finished"]!.ref!) }
            == "TAN-2 is done, so the block would lift the moment it is written. "
             + "Block on the ticket that is still open.")
        #expect(try store.ticket(id: made["waits"]!.ref!).value.blockedBy.isEmpty)
    }

    @Test("an archived blocker is refused")
    func archivedBlockerIsRefused() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "filed away"], at: dir)
        _ = try store.archive(made["filed away"]!.ref!)

        #expect(message { _ = try store.block(made["waits"]!.ref!, on: made["filed away"]!.ref!) }
            == "TAN-2 is archived, so the block could never be seen to lift. "
             + "Bring it back to the board first.")
        #expect(try store.ticket(id: made["waits"]!.ref!).value.blockedBy.isEmpty)
    }

    @Test("an unknown handle is refused with the store's own ref message")
    func unknownHandleKeepsTheStoreRule() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)

        let text = try #require(message { _ = try store.block(made["waits"]!.ref!, on: "TAN-99") })
        #expect(text.hasPrefix("Unknown ticket ref \"TAN-99\"."))
        #expect(text.contains("has issued TAN-1 to TAN-2"), "the lane's real range, not a new one")

        // An empty handle is the unknown id the store already answers for.
        let empty = try #require(message { _ = try store.block(made["waits"]!.ref!, on: "") })
        #expect(empty.hasPrefix("Unknown ticket id \"\"."))
    }

    // MARK: - The derived state

    @Test("completing the blocker lifts the block with no write")
    func completingTheBlockerLiftsIt() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)
        let waits = made["waits"]!
        _ = try store.block(waits.ref!, on: made["finishes"]!.ref!)

        _ = try store.complete(made["finishes"]!.ref!)

        let dependent = try store.ticket(id: waits.ref!).value
        #expect(dependent.blocked == false, "the block lifts without a write to the dependent")
        #expect(dependent.blockedBy.map(\.status) == [.done], "the edge stays as history")
        // The dependent's own row is untouched by the blocker's completion.
        #expect(dependent.title == waits.title)
        #expect(dependent.status == waits.status)
        #expect(dependent.assignee == waits.assignee)
        #expect(dependent.lane == waits.lane)
        #expect(dependent.number == waits.number)
        // The store writes whole seconds, so the created time is compared as stored.
        #expect(Timestamp.encode(dependent.createdAt) == Timestamp.encode(waits.createdAt))
        #expect(dependent.completedAt == waits.completedAt)
        #expect(dependent.archived == waits.archived)
    }

    @Test("blocks names the dependents on the far side, in stored order")
    func blocksNamesTheFarSide() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["one", "two", "blocker"], at: dir)

        _ = try store.block(made["one"]!.ref!, on: made["blocker"]!.ref!)
        _ = try store.block(made["two"]!.ref!, on: made["blocker"]!.ref!)

        let far = try store.ticket(id: made["blocker"]!.ref!).value
        #expect(far.blocked == false, "holding others up is not being blocked")
        // The far side has no order of its own to store, so it reads in the board's order
        // (ADR 0006), the one order every read of the store already follows.
        let stored = try store.list(.all).value
            .filter { $0.blockedBy.contains { $0.id == made["blocker"]!.id } }
            .map(\.ref)
        #expect(far.blocks.map(\.ref) == stored)
        #expect(Set(far.blocks.compactMap(\.ref)) == ["TAN-1", "TAN-2"])
        #expect(Set(far.blocks.map(\.title)) == ["one", "two"])
        #expect(far.blocks.allSatisfy { $0.status == .backlog })
    }

    // MARK: - unblock

    @Test("unblock --on clears one edge and unblock alone clears every edge")
    func unblockOneThenAll() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "first", "second"], at: dir)
        let waits = made["waits"]!.ref!
        _ = try store.block(waits, on: made["first"]!.ref!)
        _ = try store.block(waits, on: made["second"]!.ref!)

        let one = try store.unblock(waits, on: made["first"]!.ref!)
        #expect(one.value.blockedBy.map(\.ref) == ["TAN-3"])

        let none = try store.unblock(waits)
        #expect(none.value.blockedBy.isEmpty)
        #expect(try store.ticket(id: waits).value.blockedBy.isEmpty)
        #expect(try store.ticket(id: made["first"]!.ref!).value.blocks.isEmpty,
                "the far side lost the edge with the near side")
    }

    @Test("unblock on an unblocked ticket is a no-op")
    func unblockUnblockedIsNoOp() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)

        let result = try store.unblock(made["waits"]!.ref!)
        #expect(result.value.blockedBy.isEmpty)
        #expect(result.value.blocked == false)

        // A named edge that is not there is a no-op too.
        let named = try store.unblock(made["waits"]!.ref!, on: made["finishes"]!.ref!)
        #expect(named.value.blockedBy.isEmpty)
    }

    // MARK: - Edges and the rest of the store

    @Test("deleting a ticket removes its edges on both ends")
    func deleteCascades() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["one", "two", "blocker"], at: dir)
        _ = try store.block(made["one"]!.ref!, on: made["blocker"]!.ref!)
        _ = try store.block(made["two"]!.ref!, on: made["blocker"]!.ref!)

        _ = try store.delete(made["blocker"]!.ref!)

        #expect(try store.ticket(id: made["one"]!.ref!).value.blockedBy.isEmpty,
                "the near side dropped the edge")
        #expect(try store.ticket(id: made["two"]!.ref!).value.blockedBy.isEmpty)
    }

    @Test("a ticket moved to another lane keeps its edge and the ref follows the lane key")
    func moveKeepsTheEdge() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let (store, made) = try store(["waits", "finishes"], at: dir)
        _ = try store.addLane("Fleet")
        _ = try store.block(made["waits"]!.ref!, on: made["finishes"]!.ref!)

        _ = try store.move(made["finishes"]!.ref!, toLane: "Fleet", at: .max)

        let dependent = try store.ticket(id: made["waits"]!.ref!).value
        #expect(dependent.blockedBy.count == 1, "the UUID is the key, so the edge survived the move")
        #expect(dependent.blockedBy.first?.id == made["finishes"]!.id)
        #expect(dependent.blockedBy.first?.ref == "FLE-1", "the ref is the display, and it moved")
        #expect(dependent.blocked)
    }

    // MARK: - The reason is the body's section

    @Test("blockedReason reads the ## Blocked on section and is nil without one")
    func blockedReasonReadsTheSection() throws {
        let body = """
        The real state of this ticket.

        ## Blocked on

        TAN-210 has to land first.

        ## Done when

        The block lifts.
        """
        #expect(DescriptionBody.blockedReason(body)
            == "TAN-210 has to land first.")
        #expect(DescriptionBody.blockedReason("No section here.") == nil)
        #expect(DescriptionBody.blockedReason(nil) == nil)
        #expect(DescriptionBody.blockedReason("## Blocked on\n\n\n## Next\n\ntext") == nil,
                "an empty section is no reason")
    }

    @Test("a body that mentions the heading inline is not a reason")
    func inlineMentionIsNotASection() {
        let body = "This ticket's `## Blocked on` line must become data, not stay prose."
        #expect(DescriptionBody.blockedReason(body) == nil)
    }

    @Test("the ticket's blockedReason is the body's section, read not copied")
    func ticketCarriesTheReason() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.addLane("Tandem")
        _ = try store.add("waits", lane: "Tandem",
                          description: "## Blocked on\n\nTAN-210 lands first.")
        let finished = try store.add("finishes", lane: "Tandem").value

        _ = try store.block("TAN-1", on: finished.ref!)
        #expect(try store.ticket(id: "TAN-1").value.blockedReason == "TAN-210 lands first.")
    }
}
