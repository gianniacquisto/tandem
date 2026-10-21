import Foundation
import Testing

/// The `tandem-cli` binary the tests exercise: the product SwiftPM built beside
/// the test bundle, in the configuration the tests run in.
///
/// Every run is pointed at a throwaway home (`TANDEM_HOME`), so a test that
/// drives the binary never touches the human's store.
struct CLIHarness {

    /// The anchor the test bundle is found by (`Bundle(for:)` needs a class).
    private final class Anchor {}

    /// What a CLI run ended as.
    struct Result {
        var code: Int32
        var output: String
        var error: String
    }

    /// The binary's path.
    var url: URL {
        Bundle(for: Anchor.self).bundleURL
            .deletingLastPathComponent()
            .appendingPathComponent("tandem-cli")
    }

    /// Run the CLI against a throwaway home, with `stdin` piped in.
    ///
    /// `overrides` replaces the inherited environment, one variable at a time; a
    /// `nil` value **removes** a variable, so a test can drive the "the host
    /// exports no session id" case.
    func run(
        home: URL,
        _ arguments: [String],
        stdin: String = "",
        environment overrides: [String: String?] = [:]
    ) throws -> Result {
        let process = Process()
        process.executableURL = url
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        for (key, value) in overrides {
            if let value {
                environment[key] = value
            } else {
                environment.removeValue(forKey: key)
            }
        }
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

    /// The JSON object the CLI printed.
    func object(from json: String) throws -> [String: Any] {
        let data = Data(json.utf8)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return object
    }

    /// A new ticket's id, from the JSON the CLI prints.
    func addTicket(home: URL, title: String) throws -> String {
        let result = try run(home: home, ["todo", "add", "--title", title])
        #expect(result.code == 0)
        return try object(from: result.output)["id"] as? String ?? ""
    }

    /// The comments `todo comment list` prints, in append order.
    func comments(home: URL, id: String) throws -> [[String: Any]] {
        let result = try run(home: home, ["todo", "comment", "list", "--id", id])
        #expect(result.code == 0)
        return try object(from: result.output)["comments"] as? [[String: Any]] ?? []
    }
}
