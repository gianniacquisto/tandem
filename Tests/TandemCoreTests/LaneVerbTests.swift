import Foundation
import Testing

/// The `lane` verb group's read: `lane list` — the lane discovery the board's
/// lanes answer without pulling ticket rows. The write verbs (`add`, `rename`,
/// `delete`) each answer the list their write produced; `list` answers the
/// store's declared lane list, empty lanes included, the axis the store holds
/// on its own.
///
/// The suite drives the binary the way an agent does, against a throwaway home:
/// the CLI is an adapter, and the adapter's contract is the JSON on stdout, the
/// text on stderr and the exit code.
@Suite("The lane verbs")
struct LaneVerbTests {

    private let cli = CLIHarness()

    // MARK: - list

    @Test("`lane list` on an empty store is `{\"lanes\":[]}`")
    func listEmptyStore() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let result = try run(home: home, ["lane", "list"])
        #expect(result.code == 0)
        let object = try cli.object(from: result.output)
        #expect(object["lanes"] as? [String] == [])
        #expect(Set(object.keys) == ["lanes"], "one key, the shape the write verbs print")
    }

    @Test("`lane list` prints the store's declared lanes — empty lanes included, no ticket rows")
    func listPrintsTheDeclaredLanes() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["lane", "add", "--name", "Tandem"])
        _ = try run(home: home, ["lane", "add", "--name", "Retro"])
        // A ticket in one lane must not change what `list` reports: the lanes
        // come from the store's lane list, not from ticket rows.
        _ = try cli.addTicket(home: home, title: "In a lane")
        let id = try #require((try cli.object(
            from: try run(home: home, ["ticket", "list", "--filter", "all"]).output
        )["tickets"] as? [[String: Any]])?.first?["id"] as? String)
        _ = try run(home: home, ["ticket", "move", "--id", id, "--lane", "Tandem"])

        let result = try run(home: home, ["lane", "list"])
        #expect(result.code == 0)
        let object = try cli.object(from: result.output)
        #expect(object["lanes"] as? [String] == ["Tandem", "Retro"], "render order, the empty lane included")
        #expect(object["tickets"] == nil, "no ticket rows")
        #expect(object["moved"] == nil, "no deletion count — that is `delete`'s shape")
    }

    @Test("`lane list` takes no flags — an unknown one is a usage failure")
    func listRefusesFlags() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let result = try run(home: home, ["lane", "list", "--filter", "all"])
        #expect(result.code == 2, "usage text on stderr, exit 2")
        #expect(result.error.contains("Usage:"))
        #expect(result.output == "", "nothing on stdout")
    }

    // MARK: - The usage text

    @Test("the usage text lists the four lane verbs")
    func helpListsTheLaneVerbs() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let help = try run(home: home, ["--help"])
        #expect(help.code == 0)
        for verb in ["add", "rename", "delete", "list"] {
            #expect(help.output.contains("tandem-cli lane \(verb)"), "the usage lists `lane \(verb)`")
        }
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-lane-verb-\(UUID().uuidString)")
    }

    /// Run the CLI against a throwaway home.
    private func run(home: URL, _ arguments: [String]) throws -> CLIHarness.Result {
        try cli.run(home: home, arguments)
    }
}