import Foundation
import Testing
@testable import TandemCore
@testable import tandem_cli

/// The CLI's verb is `ticket`; `todo` is the deprecated alias that answers for
/// one release (D1). The alias parses the same, answers the same, and says so
/// once on stderr — where no JSON parser is reading. The wire carries both
/// words for the same release: `tickets` beside `todos`, `lane` beside
/// `section`. C8 drops the old spellings.
@Suite("The ticket verb and the todo alias")
struct TicketVerbTests {

    /// The anchor the test bundle is found by (`Bundle(for:)` needs a class).
    private final class Anchor {}

    // MARK: - The wire: both words, one array

    @Test("the list result carries the same array under `tickets` and `todos`")
    func listResultCarriesBothWords() throws {
        let one = Ticket(title: "one")
        let two = Ticket(title: "two")
        let object = try json(StoreEncoding.listResult([one, two], note: nil))

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        let todos = try #require(object["todos"] as? [[String: Any]])
        #expect(tickets.count == 2)
        #expect(todos.count == 2)
        // The same array, not a copy that drifted: the same ids in the same order.
        #expect(tickets.map { $0["id"] as? String } == [one.id.uuidString, two.id.uuidString])
        #expect(todos.map { $0["id"] as? String } == tickets.map { $0["id"] as? String })
    }

    @Test("the unknown-id error's open list carries both words too")
    func openListCarriesBothWords() throws {
        let ticket = Ticket(title: "open one")
        let object = try json(StoreEncoding.openList([ticket]))

        let tickets = try #require(object["tickets"] as? [[String: Any]])
        let todos = try #require(object["todos"] as? [[String: Any]])
        #expect(tickets.map { $0["id"] as? String } == [ticket.id.uuidString])
        #expect(todos.map { $0["id"] as? String } == tickets.map { $0["id"] as? String })
    }

    // MARK: - The wire: `lane` beside `section`

    @Test("a ticket carries `lane` beside `section`, with the same value")
    func laneRidesBesideSection() throws {
        let inLane = Ticket(title: "in a lane", lane: "Tandem")
        let noLane = Ticket(title: "in the default area")

        let withLane = try json(StoreEncoding.ticketResult(inLane, note: nil))
        #expect(withLane["lane"] as? String == "Tandem")
        #expect(withLane["lane"] as? String == withLane["section"] as? String)

        let withoutLane = try json(StoreEncoding.ticketResult(noLane, note: nil))
        #expect(withoutLane["lane"] is NSNull, "no lane is null under both keys")
        #expect(withoutLane["lane"] is NSNull, "no lane is null under both keys")

        let listed = try json(StoreEncoding.listResult([inLane], note: nil))
        let first = try #require((listed["tickets"] as? [[String: Any]])?.first)
        #expect(first["lane"] as? String == "Tandem")
        #expect(first["lane"] as? String == first["section"] as? String)
    }

    // MARK: - The alias, end to end

    @Test("`todo` answers like `ticket`, says so once on stderr, and exits 0")
    func aliasAnswersAndWarns() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let added = try run(home: home, ["ticket", "add", "--title", "A ticket"])
        #expect(added.code == 0)
        #expect(added.error.isEmpty, "the new verb says nothing")

        let ticket = try run(home: home, ["ticket", "list", "--filter", "all"])
        let todo = try run(home: home, ["todo", "list", "--filter", "all"])
        #expect(todo.code == 0)
        #expect(todo.output == ticket.output, "the alias answers the same")
        #expect(todo.error == Usage.deprecatedAliasNotice + "\n", "one line on stderr")
    }

    @Test("the usage text lists `ticket` and the alias line")
    func helpListsTheVerbAndTheAlias() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let help = try run(home: home, ["--help"])
        #expect(help.code == 0)
        #expect(help.output.contains("tandem-cli ticket list"))
        #expect(help.output.contains("tandem-cli ticket comment add"))
        #expect(help.output.contains("todo is a deprecated alias, removed in the release after this one"))
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
        // The prose is the first line; the open list under it still carries the
        // old `todos` key for this release.
        let prose = try #require(unknown.error.split(separator: "\n").first)
        #expect(prose.contains("Unknown ticket id"))
        #expect(!prose.contains("todo"))
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
