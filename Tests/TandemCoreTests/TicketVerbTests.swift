import Foundation
import Testing
@testable import TandemCore
@testable import tandem_cli

/// The CLI's verb is `ticket` — the `todo` alias ended when its one-release
/// window closed (D1 answer A): an unknown group token, usage on stderr, exit 2,
/// like any other unknown word. The wire carries one word too: `tickets`, and
/// `lane` — the v1 `todos`/`section` spellings are gone from the *encoding*,
/// while the store's decoder keeps reading them forever (C4's rule). `done`
/// stays on the wire: it predates the ticket model (ADR 0017).
@Suite("The ticket verb and the retired todo alias")
struct TicketVerbTests {

    /// The anchor the test bundle is found by (`Bundle(for:)` needs a class).
    private final class Anchor {}

    // MARK: - The wire: one word

    @Test("the list result carries `tickets` alone — the old `todos` key is gone")
    func listResultCarriesOneWord() throws {
        let one = Ticket(title: "one")
        let two = Ticket(title: "two")
        let object = try json(StoreEncoding.listResult([one, two], note: nil))

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.count == 2)
        #expect(tickets.map { $0["id"] as? String } == [one.id.uuidString, two.id.uuidString])
        #expect(object["todos"] == nil, "the old wire key is gone")
    }

    @Test("the unknown-id error's open list carries `tickets` alone too")
    func openListCarriesOneWord() throws {
        let ticket = Ticket(title: "open one")
        let object = try json(StoreEncoding.openList([ticket]))

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        #expect(tickets.map { $0["id"] as? String } == [ticket.id.uuidString])
        #expect(object["todos"] == nil, "the old wire key is gone")
    }

    // MARK: - The wire: `lane`, not `section`

    @Test("a ticket carries `lane`, never `section`; `done` stays and is derived")
    func laneOnlyNoSection() throws {
        let inLane = Ticket(title: "in a lane", lane: "Tandem")
        let noLane = Ticket(title: "in the default area")

        let withLane = try json(StoreEncoding.ticketResult(inLane, note: nil))
        #expect(withLane["lane"] as? String == "Tandem")
        #expect(withLane["section"] == nil, "the v1 wire key is gone")
        #expect(withLane["done"] as? Bool == false, "`done` stays on the wire (ADR 0017)")

        let withoutLane = try json(StoreEncoding.ticketResult(noLane, note: nil))
        #expect(withoutLane["lane"] is NSNull)
        #expect(withoutLane["section"] == nil)

        let listed = try json(StoreEncoding.listResult([inLane], note: nil))
        let first = try #require((listed["tickets"] as? [[String: Any]])?.first)
        #expect(first["lane"] as? String == "Tandem")
        #expect(first["section"] == nil)
    }

    // MARK: - `show`: one ticket by id, whole

    @Test("`show --id` returns the whole ticket — description included, where the list is summaries")
    func showReturnsTheWholeTicket() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let added = try run(home: home, ["ticket", "add", "--title", "A ticket"])
        let id = try #require((try json(added.output)["id"] as? String))
        let body = "## The brief\n\nmulti-line markdown"
        let set = try run(home: home, ["ticket", "set", "--id", id, "--description", body])
        #expect(set.code == 0)

        // The list row is a summary: no description, its length instead.
        let listed = try run(home: home, ["ticket", "list", "--filter", "all"])
        let row = try #require((try json(listed.output)["tickets"] as? [[String: Any]])?.first)
        #expect(row["description"] == nil)
        #expect(row["descriptionLength"] as? Int == body.count)

        // The show answer is the whole ticket, schema-verbatim like `add`'s.
        let shown = try run(home: home, ["ticket", "show", "--id", id])
        #expect(shown.code == 0)
        let ticket = try json(shown.output)
        #expect(ticket["id"] as? String == id)
        #expect(ticket["description"] as? String == body)
        #expect(ticket["descriptionLength"] == nil, "the whole ticket has no summary key")
    }

    @Test("`show` with an unknown or missing id is the domain failure, open list attached")
    func showUnknownIDIsADomainFailure() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        try run(home: home, ["ticket", "add", "--title", "On the board"])

        let unknown = try run(home: home, ["ticket", "show", "--id", "00000000-0000-0000-0000-000000000000"])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown ticket id"))
        // The open list rides on stderr, summarized like the list.
        let list = try #require(unknown.error.split(separator: "\n").last)
        let object = try json(String(list))
        let row = try #require((object["tickets"] as? [[String: Any]])?.first)
        #expect(row["title"] as? String == "On the board")
        #expect(row["description"] == nil)

        let missing = try run(home: home, ["ticket", "show"])
        #expect(missing.code == 1, "a missing --id is the empty id, a domain failure, not usage")
    }

    // MARK: - The alias, retired

    @Test("`todo` is an unknown group: usage on stderr, exit 2, nothing on stdout")
    func aliasIsAnUnknownGroup() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let todoList = try run(home: home, ["todo", "list", "--filter", "all"])
        #expect(todoList.code == 2, "the alias reads like `ticketx` now")
        #expect(todoList.output.isEmpty, "stdout stays empty — one JSON object or none")
        #expect(todoList.error == Usage.text + "\n", "the usage text is the whole answer")

        let bare = try run(home: home, ["todo"])
        #expect(bare.code == 2, "the bare token is the same unknown group")
        #expect(bare.error == Usage.text + "\n")
    }

    @Test("the usage text lists `ticket` and no alias line")
    func helpListsTheVerbOnly() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let help = try run(home: home, ["--help"])
        #expect(help.code == 0)
        #expect(help.output.contains("tandem-cli ticket list"))
        #expect(help.output.contains("tandem-cli ticket show --id"))
        #expect(help.output.contains("tandem-cli ticket comment add"))
        #expect(!help.output.contains("todo"), "the alias line is gone from the grammar")
    }

    @Test("the error prose says ticket")
    func errorProseSaysTicket() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let empty = try run(home: home, ["ticket", "add", "--title", ""])
        #expect(empty.code == 1)
        #expect(empty.error.contains("non-empty ticket title"))
        #expect(!empty.error.contains("todo"))

        let unknown = try run(home: home, ["ticket", "complete", "--id", "00000000-0000-0000-0000-000000000000"])
        #expect(unknown.code == 1)
        // The prose is the first line; the open list under it carries `tickets` alone now.
        let prose = try #require(unknown.error.split(separator: "\n").first)
        #expect(prose.contains("Unknown ticket id"))
        #expect(!prose.contains("todo"))
    }

    // MARK: - `archive --id`: one ticket, set aside

    @Test("`ticket archive --id` sets one ticket aside; an unknown id is the domain failure")
    func archiveWithID() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let added = try run(home: home, ["ticket", "add", "--title", "An open ticket"])
        let id = try #require((try json(added.output)["id"] as? String))

        // One ticket, by id: it comes back archived...
        let archived = try run(home: home, ["ticket", "archive", "--id", id])
        #expect(archived.code == 0)
        let ticket = try json(archived.output)
        #expect(ticket["id"] as? String == id)
        #expect(ticket["archived"] as? Bool == true)

        // ...and it has left the default views.
        let open = try run(home: home, ["ticket", "list"])
        #expect((try json(open.output)["tickets"] as? [[String: Any]])?.isEmpty == true)

        // The sweep without the flag still answers with a count — and counts
        // nothing, because the archived ticket was open, not done.
        let sweep = try run(home: home, ["ticket", "archive"])
        #expect(sweep.code == 0)
        #expect(try json(sweep.output)["archived"] as? Int == 0)

        // An unknown id is the domain failure, exit 1, the open list attached.
        let unknown = try run(
            home: home, ["ticket", "archive", "--id", "00000000-0000-0000-0000-000000000000"])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown ticket id"))
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-ticket-verb-\(UUID().uuidString)")
    }

    /// The `tandem-cli` binary this suite exercises: the product SwiftPM built
    /// beside the test bundle, in the configuration the tests run in.
    private var cliURL: URL {
        Bundle(for: Anchor.self).bundleURL
            .deletingLastPathComponent()
            .appendingPathComponent("tandem-cli")
    }

    private struct Result {
        var code: Int32
        var output: String
        var error: String
    }

    /// Run the CLI against a throwaway home.
    private func run(home: URL, _ arguments: [String]) throws -> Result {
        let process = Process()
        process.executableURL = cliURL
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        environment["TANDEM_HOME"] = home.path
        process.environment = environment
        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error
        try process.run()
        let out = output.fileHandleForReading.readDataToEndOfFile()
        let err = error.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return Result(code: process.terminationStatus,
                      output: String(decoding: out, as: UTF8.self),
                      error: String(decoding: err, as: UTF8.self))
    }

    private func json(_ text: String) throws -> [String: Any] {
        let object = try JSONSerialization.jsonObject(with: Data(text.utf8))
        guard let dictionary = object as? [String: Any] else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return dictionary
    }
}
