import Foundation
import Testing

/// `ticket block` / `ticket unblock` / `ticket list --blocked` (TAN-232): the CLI's
/// half of the dependency edge. The verb writes, the read carries the four fields,
/// and a refusal is the store's own message on stderr with nothing on stdout.
@Suite("Ticket dependency verbs")
struct DependencyVerbTests {

    private let cli = CLIHarness()

    private func home() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-depverbs-\(UUID().uuidString)")
    }

    /// A lane and three tickets: `waits` is `TAN-1`, `blocker` is `TAN-2`, `other`
    /// is `TAN-3`.
    private func seed(home: URL) throws {
        #expect(try cli.run(home: home, ["lane", "add", "--name", "Tandem"]).code == 0)
        for title in ["waits", "blocker", "other"] {
            let added = try cli.run(home: home,
                                    ["ticket", "add", "--title", title, "--lane", "Tandem"])
            #expect(added.code == 0, "add \(title): \(added.error)")
        }
    }

    @Test("ticket block prints the ticket with its blockedBy and exits 0")
    func blockPrintsTheEdge() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)

        let result = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-2"])
        #expect(result.code == 0)
        #expect(result.error == "")
        let ticket = try cli.object(from: result.output)
        #expect(ticket["blocked"] as? Bool == true)
        let blockedBy = try #require(ticket["blockedBy"] as? [[String: Any]])
        #expect(blockedBy.count == 1)
        #expect(blockedBy.first?["ref"] as? String == "TAN-2")
        #expect(blockedBy.first?["title"] as? String == "blocker")
        #expect(blockedBy.first?["status"] as? String == "backlog")
    }

    @Test("ticket unblock --on prints the ticket with one edge left")
    func unblockLeavesOneEdge() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)

        _ = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-2"])
        _ = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-3"])

        let result = try cli.run(home: home, ["ticket", "unblock", "--id", "TAN-1", "--on", "TAN-2"])
        #expect(result.code == 0)
        #expect(result.error == "")
        let ticket = try cli.object(from: result.output)
        let blockedBy = try #require(ticket["blockedBy"] as? [[String: Any]])
        #expect(blockedBy.map { $0["ref"] as? String } == ["TAN-3"])
        #expect(ticket["blocked"] as? Bool == true)
    }

    @Test("ticket list --blocked prints only the blocked rows")
    func listBlockedFilters() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)
        _ = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-2"])

        let result = try cli.run(home: home, ["ticket", "list", "--blocked"])
        #expect(result.code == 0)
        let tickets = try #require(try cli.object(from: result.output)["tickets"] as? [[String: Any]])
        #expect(tickets.map { $0["ref"] as? String } == ["TAN-1"])

        // The summary keeps the edge but not the reason: the reason is body text.
        let row = try #require(tickets.first)
        #expect(row["blocked"] as? Bool == true)
        #expect((row["blockedBy"] as? [[String: Any]])?.count == 1)
        #expect(row["blocks"] as? [Any] != nil, "the far side rides the summary too")
        #expect(row["blockedReason"] == nil, "the body's reason stays out of the summary")
    }

    @Test("ticket show carries blocked, blockedBy, blockedReason and blocks")
    func showCarriesEveryField() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)
        let body = "The real state.\n\n## Blocked on\n\nTAN-2 lands first.\n"
        _ = try cli.run(home: home, ["ticket", "set", "--id", "TAN-1", "--description", body])
        _ = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-2"])

        let dependent = try cli.object(from: try cli.run(
            home: home, ["ticket", "show", "--id", "TAN-1"]).output)
        #expect(dependent["blocked"] as? Bool == true)
        #expect(dependent["blockedReason"] as? String == "TAN-2 lands first.")
        let blockedBy = try #require(dependent["blockedBy"] as? [[String: Any]])
        #expect(blockedBy.first?["ref"] as? String == "TAN-2")
        #expect(blockedBy.first?["title"] as? String == "blocker")
        #expect(blockedBy.first?["status"] as? String == "backlog")

        let blocker = try cli.object(from: try cli.run(
            home: home, ["ticket", "show", "--id", "TAN-2"]).output)
        #expect(blocker["blocked"] as? Bool == false)
        let blocks = try #require(blocker["blocks"] as? [[String: Any]])
        #expect(blocks.map { $0["ref"] as? String } == ["TAN-1"])
        #expect(blocks.first?["title"] as? String == "waits")
        #expect(blocks.first?["status"] as? String == "backlog")
    }

    @Test("a refused block exits 1 with the message on stderr and no JSON on stdout")
    func refusedBlockWritesNothing() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)

        let result = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1", "--on", "TAN-1"])
        #expect(result.code == 1)
        #expect(result.output == "", "a refusal writes no JSON")
        #expect(result.error.contains(
            "TAN-1 cannot block on itself. Name the ticket that has to finish first."))

        let ticket = try cli.object(from: try cli.run(
            home: home, ["ticket", "show", "--id", "TAN-1"]).output)
        #expect(ticket["blocked"] as? Bool == false, "the refusal wrote nothing")
    }

    @Test("ticket block with no --on fails as an unknown handle")
    func missingOnIsAnUnknownHandle() throws {
        let home = home()
        defer { try? FileManager.default.removeItem(at: home) }
        try seed(home: home)

        let result = try cli.run(home: home, ["ticket", "block", "--id", "TAN-1"])
        #expect(result.code == 1)
        #expect(result.output == "")
        #expect(result.error.hasPrefix("Unknown ticket id \"\"."))
    }
}
