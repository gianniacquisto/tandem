import Foundation
import Testing
@testable import TandemCore

/// The board: the core's half of it. Two seams — `TicketBoard.rows` (pure grouping, so the
/// board and the list cannot disagree about what a lane holds) and `Store.place` (the
/// drop, one write for lane and Status together, proved against the real store).
@Suite("Board")
struct BoardTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-board-\(UUID().uuidString)")
    }

    private func makeStore() -> (Store, URL) {
        let dir = tempDir()
        return (Store(directory: dir), dir)
    }

    // MARK: - rows

    @Test("every declared lane draws a row, even with no tickets in it")
    func emptyRows() {
        let rows = TicketBoard.rows(from: [], lanes: ["Board", "Memory"])
        #expect(rows.map(\.key) == [nil, "Board", "Memory"])
        #expect(rows.allSatisfy { $0.count == 0 })
    }

    @Test("a row is its lane split by Status, in stored order inside each column")
    func cellsKeepStoredOrder() {
        let a = Ticket(title: "a", status: .backlog, lane: nil)
        let b = Ticket(title: "b", status: .backlog, lane: nil)
        let c = Ticket(title: "c", status: .ready, lane: nil)
        let rows = TicketBoard.rows(from: [a, b, c], lanes: [])
        #expect(rows.count == 1)
        #expect(rows[0].tickets(.backlog).map { $0.title } == ["a", "b"])
        #expect(rows[0].tickets(.ready).map { $0.title } == ["c"])
        #expect(rows[0].tickets(.done).isEmpty)
    }

    /// TAN-196: the retired `refined` row is still a ticket, so it draws in the column
    /// the word folds into — On hold — and the lane header's count is the cells' sum.
    @Test("a refined row draws beside the on-hold rows, and the header count matches the cells")
    func refinedFoldsIntoOnHold() {
        let parked = Ticket(title: "parked the old way", status: .refined, lane: "Tandem")
        let held = Ticket(title: "parked the new way", status: .onHold, lane: "Tandem")
        let rows = TicketBoard.rows(from: [parked, held], lanes: ["Tandem"])

        let row = rows[1]
        #expect(row.tickets(.onHold).map { $0.title } == ["parked the old way", "parked the new way"])
        #expect(row.tickets(.refined).isEmpty, "refined is a readable status, not a drawn column")
        let drawn = Status.workflow.reduce(0) { $0 + row.tickets($1).count }
        #expect(drawn == row.count, "the lane header counts exactly what the cells draw")
    }

    @Test("the default area is the first row, then declared lanes, then a lane only a ticket names")
    func laneOrder() {
        let handEdited = Ticket(title: "hand edited", lane: "Stray")
        let filed = Ticket(title: "filed", lane: "Board")
        let loose = Ticket(title: "loose")
        let rows = TicketBoard.rows(from: [loose, filed, handEdited], lanes: ["Board", "Memory"])
        #expect(rows.map(\.key) == [nil, "Board", "Memory", "Stray"])
        #expect(rows[3].tickets(.backlog).map { $0.title } == ["hand edited"])
    }

    @Test("archived tickets are the caller's decision, not the board's")
    func archivedIsTheCallersDecision() {
        let open = Ticket(title: "open")
        var archived = Ticket(title: "archived", status: .done)
        archived.archived = true
        let all = TicketBoard.rows(from: [open, archived], lanes: [])
        #expect(all[0].count == 2)
        let visible = TicketBoard.rows(from: TicketQuery().apply(to: [open, archived]), lanes: [])
        #expect(visible[0].tickets(.done).isEmpty)
        #expect(visible[0].count == 1)
    }

    // MARK: - place

    @Test("a drop sets the lane and the Status in one write")
    func placeSetsBothAxes() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try store.addLane("Board")
        let ticket = try store.add("carry it").value

        let placed = try store.place(ticket.id.uuidString, lane: "Board", status: .inProgress).value
        #expect(placed.lane == "Board")
        #expect(placed.status == .inProgress)
        #expect(!placed.done)
        // The file agrees, after one write (read it back through a fresh store).
        let reread = try Store(directory: dir).list(.all).value
        #expect(reread.map { $0.status } == [.inProgress])
        #expect(reread.map { $0.lane } == ["Board"])
    }

    @Test("a drop into in-review persists and draws in the in-review column")
    func placeInReview() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try store.add("waiting for a reader").value

        let placed = try store.place(ticket.id.uuidString, lane: nil, status: .inReview).value
        #expect(placed.status == .inReview)
        #expect(!placed.done)

        // The file agrees, and the board draws the ticket in the in-review cell.
        let reread = try #require(try Store(directory: dir).list(.all).value.first)
        let rows = TicketBoard.rows(from: [reread], lanes: [])
        #expect(rows[0].tickets(.inReview).map { $0.title } == ["waiting for a reader"])
        #expect(rows[0].tickets(.inProgress).isEmpty)
        #expect(rows[0].tickets(.done).isEmpty)
    }

    @Test("a drop appends to its lane by default and takes an index when given one")
    func placeIndex() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try store.addLane("Board")
        _ = try store.add("one", lane: "Board")
        _ = try store.add("two", lane: "Board")
        let moving = try store.add("three", lane: "Board").value

        // No index: the drop appends to the end of the lane.
        _ = try store.place(moving.id.uuidString, lane: "Board", status: .ready)
        #expect(try store.list(.all).value.map { $0.title } == ["two", "one", "three"])

        // An index: the drop lands at that place in the lane.
        _ = try store.place(moving.id.uuidString, lane: "Board", status: .backlog, at: 0)
        #expect(try store.list(.all).value.map { $0.title } == ["three", "two", "one"])
    }

    @Test("dropping into done stamps completedAt; dropping out of it clears it")
    func placeDoneMirror() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try store.add("finish it").value

        let done = try store.place(ticket.id.uuidString, lane: nil, status: .done).value
        #expect(done.done)
        #expect(done.completedAt != nil)

        let back = try store.place(ticket.id.uuidString, lane: nil, status: .ready).value
        #expect(!back.done)
        #expect(back.completedAt == nil)
    }

    @Test("a drop that lands the ticket where it already was changes nothing")
    func placeNoOp() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try store.add("a")
        let moving = try store.add("b").value

        let same = try store.place(moving.id.uuidString, lane: nil, status: .backlog, at: 0).value
        #expect(same.id == moving.id)
        #expect(try store.list(.all).value.map { $0.title } == ["b", "a"])
    }

    @Test("an unknown lane or id is rejected and the store is left alone")
    func placeRejects() throws {
        let (store, dir) = makeStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try store.add("a").value
        let before = try store.list(.all).value

        #expect(throws: DomainError.self) {
            try store.place(ticket.id.uuidString, lane: "Nope", status: .ready)
        }
        #expect(throws: DomainError.self) {
            try store.place(UUID().uuidString, lane: nil, status: .ready)
        }
        #expect(try store.list(.all).value == before)
    }
}
