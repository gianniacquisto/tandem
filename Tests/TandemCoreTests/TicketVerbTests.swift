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

    // MARK: - `add --description`: the body in the same write

    @Test("`ticket add --description -` lands the body in the same write — no add-then-set round trip")
    func addCarriesTheBodyFromStdin() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-ticket-verb-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let body = "## Why\n\nmulti-line markdown, quoting-free"

        let added = try cli.run(home: home,
                                ["ticket", "add", "--title", "A body'd ticket", "--description", "-"],
                                stdin: body)
        #expect(added.code == 0)
        let ticket = try json(added.output)
        #expect(ticket["description"] as? String == body)
        #expect(ticket["descriptionLength"] == nil, "add answers the whole ticket, not a summary")

        // The one write landed: the stored row reads back whole...
        let id = try #require(ticket["id"] as? String)
        let shown = try cli.run(home: home, ["ticket", "show", "--id", id])
        #expect(shown.code == 0)
        #expect(try json(shown.output)["description"] as? String == body)

        // ...and the list summary carries its length, so the row is the same ticket.
        let listed = try cli.run(home: home, ["ticket", "list", "--filter", "all"])
        let row = try #require((try json(listed.output)["tickets"] as? [[String: Any]])?.first)
        #expect(row["descriptionLength"] as? Int == body.count)
    }

    @Test("`ticket add` refuses an empty description — the `\"\"` flag and an empty pipe alike")
    func addRefusesAnEmptyDescription() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-ticket-verb-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }

        // A new ticket has no earlier body, so an empty value is not a clear:
        // naming `--description` means the body must carry text — the same
        // refusal an empty `--description -` pipe meets.
        let emptyFlag = try cli.run(home: home,
                                    ["ticket", "add", "--title", "No body", "--description", ""])
        #expect(emptyFlag.code == 1)
        #expect(emptyFlag.error.contains("empty"))
        #expect(emptyFlag.output.isEmpty, "stdout stays empty — one JSON object or none")

        let emptyPipe = try cli.run(home: home,
                                    ["ticket", "add", "--title", "No body", "--description", "-"],
                                    stdin: "  \n\t ")
        #expect(emptyPipe.code == 1)
        #expect(emptyPipe.error.contains("empty"))

        // Nothing was filed: the refusal happens before the write.
        let listed = try cli.run(home: home, ["ticket", "list", "--filter", "all"])
        #expect((try json(listed.output)["tickets"] as? [[String: Any]])?.isEmpty == true)
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

    // MARK: - the on-hold status (TAN-196: the reader ships first)

    @Test("`ticket set --status on-hold` writes the new value and `list --status` finds it")
    func onHoldWritesAndLists() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let added = try run(home: home, ["ticket", "add", "--title", "Parked on purpose"])
        let id = try #require((try json(added.output)["id"] as? String))

        let set = try run(home: home, ["ticket", "set", "--id", id, "--status", "on-hold"])
        #expect(set.code == 0)
        #expect(try json(set.output)["status"] as? String == "on-hold")

        let listed = try run(home: home, ["ticket", "list", "--status", "on-hold"])
        #expect(listed.code == 0)
        let rows = try #require(try json(listed.output)["tickets"] as? [[String: Any]])
        #expect(rows.count == 1)
        #expect(rows.first?["status"] as? String == "on-hold")
    }

    /// TAN-195 retires the word: the reader era accepted `refined` beside `on-hold`, and
    /// the writer build refuses it by name instead of storing a status nothing draws.
    @Test("`ticket set --status refined` is refused and the ticket keeps its stage")
    func retiredStatusIsRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let added = try run(home: home, ["ticket", "add", "--title", "Parked the old way"])
        let id = try #require((try json(added.output)["id"] as? String))

        let set = try run(home: home, ["ticket", "set", "--id", id, "--status", "refined"])
        #expect(set.code == 1)
        #expect(set.error.contains("status must be one of"))
        #expect(set.error.contains("on-hold"), "the refusal lists the live values")

        let shown = try run(home: home, ["ticket", "show", "--id", id])
        #expect(try json(shown.output)["status"] as? String == "backlog",
                "a refused status writes nothing")
    }

    // MARK: - Helpers

    /// The shared harness, for the runs that feed stdin (`add --description -`).
    private let cli = CLIHarness()

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
