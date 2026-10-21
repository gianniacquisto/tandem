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

    /// The anchor the test bundle is found by (`Bundle(for:)` needs a class).
    private final class Anchor {}

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

        let id = try addTicket(home: home, title: "A ticket")
        let set = try run(home: home, ["todo", "set", "--id", id, "--description", "the brief"])
        #expect(set.code == 0)
        #expect(try description(of: id, home: home) == "the brief")

        for body in ["", "   ", "\n\t "] {
            let refused = try run(home: home,
                                  ["todo", "set", "--id", id, "--description", "-"],
                                  stdin: body)
            #expect(refused.code == 1)
            #expect(refused.error.contains("empty"))
            #expect(try description(of: id, home: home) == "the brief")
        }

        // `--description ""` is the deliberate clear: it still works.
        let cleared = try run(home: home, ["todo", "set", "--id", id, "--description", ""])
        #expect(cleared.code == 0)
        #expect(try description(of: id, home: home) == nil)
    }

    // MARK: - Helpers

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

    /// Run the CLI against a throwaway home, with `stdin` piped in.
    private func run(home: URL, _ arguments: [String], stdin: String = "") throws -> Result {
        let process = Process()
        process.executableURL = cliURL
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        environment["TANDEM_HOME"] = home.path
        process.environment = environment
        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error
        try process.run()
        input.fileHandleForWriting.write(Data(stdin.utf8))
        input.fileHandleForWriting.closeFile()
        let out = output.fileHandleForReading.readDataToEndOfFile()
        let err = error.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return Result(code: process.terminationStatus,
                      output: String(decoding: out, as: UTF8.self),
                      error: String(decoding: err, as: UTF8.self))
    }

    /// A new ticket's id, from the JSON the CLI prints.
    private func addTicket(home: URL, title: String) throws -> String {
        let result = try run(home: home, ["todo", "add", "--title", title])
        #expect(result.code == 0)
        return try object(from: result.output)["id"] as? String ?? ""
    }

    /// A ticket's description, read back through the CLI's own list.
    private func description(of id: String, home: URL) throws -> String? {
        let result = try run(home: home, ["todo", "list", "--filter", "all"])
        #expect(result.code == 0)
        guard let todos = try object(from: result.output)["todos"] as? [[String: Any]],
              let todo = todos.first(where: { $0["id"] as? String == id })
        else { return nil }
        return todo["description"] as? String
    }

    private func object(from json: String) throws -> [String: Any] {
        let data = Data(json.utf8)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return object
    }
}
