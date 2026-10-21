import Foundation
import Testing
@testable import TandemCore
@testable import tandem_cli

/// The CLI's own `--description` path: the flag's value, and the stdin body
/// behind `-`. An empty or whitespace-only body is refused — the pipe that
/// produced it is a failed write, not a deliberate clear — and the refusal
/// leaves the description alone.
@Suite("CLI description body")
struct CLIDescriptionTests {

    // MARK: - The stdin path

    @Test("a stdin body that carries text is the body")
    func stdinBodyIsUsed() throws {
        #expect(try descriptionBody("-", stdin: { "the brief" }) == "the brief")
    }

    @Test("an empty or whitespace-only stdin body is refused")
    func emptyStdinBodyIsRefused() {
        for body in ["", "   ", "\n\t "] {
            #expect(throws: DomainError.emptyDescriptionBody) {
                _ = try descriptionBody("-", stdin: { body })
            }
        }
    }

    @Test("a deliberate empty flag clears without reading stdin")
    func emptyFlagIsNotRefused() throws {
        #expect(try descriptionBody("", stdin: { "never read" }) == "")
    }

    // MARK: - The binary, end to end

    @Test("an empty stdin is refused, and the description survives it")
    func emptyStdinIsRefusedEndToEnd() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-description-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "A ticket")
        let set = try cli.run(home: home, ["todo", "set", "--id", id, "--description", "the brief"])
        #expect(set.code == 0)
        #expect(try description(of: id, home: home) == "the brief")

        for body in ["", "   ", "\n\t "] {
            let refused = try cli.run(home: home,
                                      ["todo", "set", "--id", id, "--description", "-"],
                                      stdin: body)
            #expect(refused.code == 1)
            #expect(refused.error.contains("empty"))
            #expect(try description(of: id, home: home) == "the brief")
        }

        // `--description ""` is the deliberate clear: it still works.
        let cleared = try cli.run(home: home, ["todo", "set", "--id", id, "--description", ""])
        #expect(cleared.code == 0)
        #expect(try description(of: id, home: home) == nil)
    }

    // MARK: - Helpers

    /// The CLI this suite exercises.
    private let cli = CLIHarness()

    /// A ticket's description, read back through the CLI's own list.
    private func description(of id: String, home: URL) throws -> String? {
        let result = try cli.run(home: home, ["todo", "list", "--filter", "all"])
        #expect(result.code == 0)
        guard let todos = try cli.object(from: result.output)["todos"] as? [[String: Any]],
              let todo = todos.first(where: { $0["id"] as? String == id })
        else { return nil }
        return todo["description"] as? String
    }
}
