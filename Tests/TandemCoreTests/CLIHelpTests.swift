import Foundation
import Testing
@testable import tandem_cli

/// The `--help` wiring below the root: every subcommand and verb prints its own
/// usage to stdout and exits 0 (ticket `BEE9B50F`) — the root usage stays as it
/// is, and a help probe under `set -euo pipefail` no longer aborts its block.
/// ADR 0009: help carries syntax only; the skill carries the meaning.
@Suite("Subcommand help")
struct CLIHelpTests {

    /// A throwaway home: help touches no store, but the harness points every
    /// run at one anyway.
    private let home = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("tandem-help-tests-\(UUID().uuidString)")

    private let cli = CLIHarness()

    // MARK: - The done-when verbs

    @Test("note capture --help prints note capture's usage and exits 0")
    func noteCaptureHelp() throws {
        let result = try cli.run(home: home, ["note", "capture", "--help"])
        #expect(result.code == 0)
        #expect(result.error == "", "a help probe is not an error")
        #expect(result.output.contains("tandem-cli note capture"))
        #expect(result.output.contains("--kind"))
        #expect(result.output.contains("<decision|gotcha|fact|thread>"),
            "the four live kinds (the retired `work` kind is gone)")
        #expect(!result.output.contains("fact|work|thread"), "the retired kind leaves the help")
    }

    @Test("ticket add --help and ticket set --help print their own usage, exit 0")
    func ticketVerbHelp() throws {
        for verb in ["add", "set"] {
            let result = try cli.run(home: home, ["ticket", verb, "--help"])
            #expect(result.code == 0, "verb \(verb)")
            #expect(result.output.contains("tandem-cli ticket \(verb)"), "verb \(verb)")
        }
    }

    @Test("session start --help and lane add --help print their own usage, exit 0")
    func otherVerbHelp() throws {
        let session = try cli.run(home: home, ["session", "start", "--help"])
        #expect(session.code == 0)
        #expect(session.output.contains("tandem-cli session start"))

        let lane = try cli.run(home: home, ["lane", "add", "--help"])
        #expect(lane.code == 0)
        #expect(lane.output.contains("tandem-cli lane add"))
    }

    @Test("lane list --help prints lane list's own usage and exits 0")
    func laneListHelp() throws {
        let result = try cli.run(home: home, ["lane", "list", "--help"])
        #expect(result.code == 0)
        #expect(result.error == "", "a help probe is not an error")
        #expect(result.output.hasPrefix("tandem-cli lane list"),
            "the verb's own line, not the group or root text")
    }

    // MARK: - Every verb and group answers

    @Test("every verb's --help prints that verb's usage and exits 0")
    func everyVerbAnswers() throws {
        for name in Usage.verbs.keys {
            let args = name.split(separator: " ").map(String.init) + ["--help"]
            let result = try cli.run(home: home, args)
            #expect(result.code == 0, "verb \(name)")
            #expect(result.error == "", "verb \(name)")
            #expect(result.output.contains("tandem-cli \(name)"), "verb \(name)")
        }
    }

    @Test("every group's --help prints that group's verbs and exits 0")
    func everyGroupAnswers() throws {
        for group in Usage.groups.keys {
            let result = try cli.run(home: home, [group, "--help"])
            #expect(result.code == 0, "verb \(group)")
            #expect(result.error == "", "verb \(group)")
            #expect(result.output.contains("tandem-cli \(group) "), "verb \(group)")
        }
    }

    /// A group's text is the root's lines for its verbs, so a verb the CLI
    /// dispatches under a group must not be missing from that group's `--help`
    /// (ticket `94F94B1A`: `lane list` had its own help but no group line).
    @Test("every group's --help lists every verb that group dispatches")
    func everyGroupListsItsVerbs() throws {
        let dispatched = Dictionary(grouping: Usage.verbs.keys) {
            $0.split(separator: " ").first.map(String.init) ?? $0
        }
        for group in Usage.groups.keys {
            let result = try cli.run(home: home, [group, "--help"])
            let lines = result.output.split(separator: "\n")
                .map { $0.trimmingCharacters(in: .whitespaces) }
            for verb in dispatched[group] ?? [] {
                #expect(lines.contains { $0.hasPrefix("tandem-cli \(verb)") },
                    "\(group) --help omits `\(verb)`")
            }
        }
    }

    @Test("`-h` answers below the root too")
    func shortSpellingAnswers() throws {
        let result = try cli.run(home: home, ["ticket", "list", "-h"])
        #expect(result.code == 0)
        #expect(result.output.contains("tandem-cli ticket list"))
    }

    // MARK: - The edges

    @Test("the root --help is unchanged and still exits 0")
    func rootHelpUnchanged() throws {
        let result = try cli.run(home: home, ["--help"])
        #expect(result.code == 0)
        #expect(result.output == Usage.text + "\n")
    }

    @Test("an unknown verb with --help is still a usage failure, exit 2")
    func unknownVerbStillFails() throws {
        let result = try cli.run(home: home, ["ticket", "bogus", "--help"])
        #expect(result.code == 2)
        #expect(result.output == "", "usage goes to stderr")
        #expect(result.error.contains("tandem-cli ticket list"))
    }

    @Test("a verb help is narrower than the root usage")
    func verbHelpIsNarrow() throws {
        let result = try cli.run(home: home, ["note", "capture", "--help"])
        #expect(result.output.contains("tandem-cli note capture"))
        #expect(!result.output.contains("tandem-cli sprint open"),
                "one verb's help does not carry the whole grammar")
    }
}