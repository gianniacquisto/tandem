import Foundation
import Testing
@testable import TandemCore

/// The CLI's result encoding (schema v1's wire shapes) and actionable error text.
///
/// The store's own corrupt-file recovery tests are gone with the file protocol: a write
/// is one transaction now (ADR 0031), so there is no half-written file to recover, and a
/// legacy JSON read that cannot be decoded fails loudly instead of restarting empty
/// (see `StoreMigrationTests`). What remains here is the wire, unchanged.
@Suite("Recovery and encoding")
struct RecoveryTests {

    private func json(_ text: String) throws -> [String: Any] {
        let obj = try JSONSerialization.jsonObject(with: Data(text.utf8))
        guard let dict = obj as? [String: Any] else { throw TestError() }
        return dict
    }

    private struct TestError: Error {}

    // MARK: - Encoding

    @Test("StoreEncoding.listResult shape, note present only when non-nil")
    func listResultEncoding() throws {
        let todo = Ticket(title: "one", description: "the body")
        // No note → no note key.
        let noNote = try json(StoreEncoding.listResult([todo], note: nil))
        #expect((noNote["tickets"] as? [Any])?.count == 1)
        #expect(noNote["todos"] == nil, "the old wire key is gone from the encoding")
        #expect(noNote["note"] == nil, "note key must be absent when note is nil")
        // Summary rows: no description key, its length instead.
        let row = try #require((noNote["tickets"] as? [[String: Any]])?.first)
        #expect(row["description"] == nil, "list rows are summaries, not whole tickets")
        #expect(row["descriptionLength"] as? Int == "the body".count)
        #expect(row["title"] as? String == "one")
        // No description → length 0, the key still there (the shape never varies by row).
        let bare = try json(StoreEncoding.listResult([Ticket(title: "bare")], note: nil))
        let bareRow = try #require((bare["tickets"] as? [[String: Any]])?.first)
        #expect(bareRow["descriptionLength"] as? Int == 0)
        // With note → top-level note.
        let withNote = try json(StoreEncoding.listResult([todo], note: "NOTE"))
        #expect(withNote["note"] as? String == "NOTE")
        // Empty → {"tickets":[]}.
        let empty = try json(StoreEncoding.listResult([], note: nil))
        #expect((empty["tickets"] as? [Any])?.isEmpty == true)
    }

    @Test("StoreEncoding.ticketResult is the ticket object plus optional note")
    func ticketResultEncoding() throws {
        let todo = Ticket(title: "thing")
        let obj = try json(StoreEncoding.ticketResult(todo, note: nil))
        #expect(obj["id"] as? String == todo.id.uuidString)
        #expect(obj["title"] as? String == "thing")
        // The wire keeps the derived mirror even though the store file no longer writes it.
        #expect(obj["done"] as? Bool == false)
        #expect(obj["createdAt"] is String)
        #expect(obj["completedAt"] is NSNull, "completedAt should be null while open")
        #expect(obj["note"] == nil)
        let withNote = try json(StoreEncoding.ticketResult(todo, note: "N"))
        #expect(withNote["note"] as? String == "N")
    }

    @Test("StoreEncoding.openList is the list shape")
    func openListEncoding() throws {
        let todo = Ticket(title: "open one", description: "a body")
        let obj = try json(StoreEncoding.openList([todo]))
        #expect((obj["tickets"] as? [Any])?.count == 1)
        #expect(obj["todos"] == nil, "the old wire key is gone from the encoding")
        #expect(obj["note"] == nil)
        // Summarized like the list: the recovery needs the id and the title.
        let row = try #require((obj["tickets"] as? [[String: Any]])?.first)
        #expect(row["description"] == nil)
        #expect(row["descriptionLength"] as? Int == "a body".count)
    }

    @Test("StoreEncoding lane results: the lanes after the write, plus moved on delete")
    func laneResultEncoding() throws {
        let added = try json(StoreEncoding.lanesResult(["Tandem", "fleet"], note: nil))
        #expect(added["lanes"] as? [String] == ["Tandem", "fleet"])
        #expect(added["note"] == nil)

        let withNote = try json(StoreEncoding.lanesResult([], note: "N"))
        #expect((withNote["lanes"] as? [Any])?.isEmpty == true)
        #expect(withNote["note"] as? String == "N")

        let deleted = try json(
            StoreEncoding.laneDeletionResult(
                LaneDeletion(lanes: ["fleet"], moved: 3), note: nil))
        #expect(deleted["lanes"] as? [String] == ["fleet"])
        #expect(deleted["moved"] as? Int == 3)
    }

    @Test("tandemErrorText renders each domain error actionably")
    func errorTextRendering() throws {
        #expect(tandemErrorText(DomainError.locked).contains("locked"))
        #expect(tandemErrorText(DomainError.emptyTitle).contains("Title is empty"))
        #expect(tandemErrorText(DomainError.invalidFilter("bogus")).contains("bogus"))

        let open = [Ticket(title: "the open one")]
        let unknown = tandemErrorText(DomainError.unknownID(id: "x", open: open))
        #expect(unknown.contains("x"))
        #expect(unknown.contains("the open one"), "unknownID text should carry the open list")

        let recovery = RecoveryFailure(underlying: DomainError.emptyTitle, note: "NOTE-XYZ")
        let recoveryText = tandemErrorText(recovery)
        #expect(recoveryText.contains("Title is empty"))
        #expect(recoveryText.contains("NOTE-XYZ"))
    }
}
