import Foundation
import Testing
@testable import TandemCore
@testable import tandem_cli

/// `ticket answer` (D2522EC3): the human's answer recorded and the hold closed in
/// **one write** — the `## Answer — <date>` block appended to the description, the
/// status the answer opens, and the next assignee (omitted clears). The suite drives
/// the binary the way an agent does, against a throwaway home: the contract is the
/// JSON on stdout, the text on stderr and the exit code.
@Suite("The answer verb")
struct AnswerVerbTests {

    private let cli = CLIHarness()

    // MARK: - One write, both halves

    @Test("`answer` appends the dated block and moves the status and assignee together")
    func answerAppendsAndMovesThePair() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "Which option?")
        let brief = "## Why\n\nWhich option?\n\n## What\n\nPick one."
        let set = try run(home: home, [
            "ticket", "set", "--id", id, "--status", "refined", "--assignee", "human",
            "--description", brief,
        ])
        #expect(set.code == 0)

        let answered = try run(home: home, [
            "ticket", "answer", "--id", id, "--status", "ready",
            "--body", "A — the first option.",
        ])
        #expect(answered.code == 0)
        #expect(answered.error == "")

        // The ticket object comes back, whole: the pair has moved.
        let object = try cli.object(from: answered.output)
        #expect(object["id"] as? String == id)
        #expect(object["status"] as? String == "ready")
        #expect(object["assignee"] is NSNull, "the question is settled: the hold is closed")

        // The block is the answer, the brief survives above it, and the heading is dated.
        let description = try #require(object["description"] as? String)
        #expect(description.hasPrefix(brief + "\n\n## Answer — "),
                "the answer is appended below the brief")
        #expect(description.contains("\n\nA — the first option."))
        let heading = try #require(description.split(separator: "\n")
            .first { $0.hasPrefix("## Answer — ") })
        let date = heading.dropFirst("## Answer — ".count)
        #expect(date.range(of: #"^\d{4}-\d{2}-\d{2}$"#, options: .regularExpression) != nil,
                "a dated heading (got \"\(date)\")")
    }

    @Test("an omitted `--assignee` clears the owner; `--assignee \"\"` is the same")
    func omittedAssigneeClears() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        for (index, extra) in [[], ["--assignee", ""]].enumerated() {
            let id = try cli.addTicket(home: home, title: "Question \(index)")
            _ = try run(home: home, ["ticket", "set", "--id", id, "--assignee", "human"])

            let args = ["ticket", "answer", "--id", id, "--status", "ready",
                        "--body", "B — settled."] + extra
            let answered = try run(home: home, args)
            #expect(answered.code == 0)
            #expect(try cli.object(from: answered.output)["assignee"] is NSNull,
                    "omitted `--assignee` clears the hold")
        }
    }

    @Test("`--assignee self` hands the ticket to this agent")
    func selfAssignsTheAgent() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "Question")
        _ = try run(home: home, ["ticket", "set", "--id", id, "--assignee", "human"])

        let answered = try run(home: home, [
            "ticket", "answer", "--id", id, "--status", "ready",
            "--assignee", "self", "--body", "A.",
        ])
        #expect(answered.code == 0)
        #expect(try cli.object(from: answered.output)["assignee"] as? String == "sprint",
                "the next owner is named in the same write")
    }

    // MARK: - The refusals mirror `set`'s

    @Test("an empty body is refused and the ticket is left alone")
    func emptyBodyIsRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "Question")
        _ = try run(home: home, ["ticket", "set", "--id", id, "--status", "refined",
                                 "--assignee", "human", "--description", "## Why\n\nWhich?"])

        for (args, stdin) in [
            (["--body", ""], ""),
            (["--body", "   "], ""),
            (["--body", "-"], ""),
            (["--body", "-"], " \n\t "),
        ] {
            let refused = try run(home: home, [
                "ticket", "answer", "--id", id, "--status", "ready",
            ] + args, stdin: stdin)
            #expect(refused.code == 1, "args \(args), stdin \(stdin)")
            #expect(refused.error.contains("empty"))
            #expect(try show(id: id, home: home)["description"] as? String == "## Why\n\nWhich?",
                    "a refused answer writes nothing")
        }
    }

    @Test("an unknown status and an unknown id are domain failures")
    func unknownStatusAndID() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "Question")

        let badStatus = try run(home: home, [
            "ticket", "answer", "--id", id, "--status", "bogus", "--body", "A",
        ])
        #expect(badStatus.code == 1)
        #expect(badStatus.error.contains("bogus"))

        let missingStatus = try run(home: home, [
            "ticket", "answer", "--id", id, "--body", "A",
        ])
        #expect(missingStatus.code == 1, "the answer names the status it opens")

        let badID = try run(home: home, [
            "ticket", "answer", "--id", "not-an-id", "--status", "ready", "--body", "A",
        ])
        #expect(badID.code == 1)
        #expect(badID.error.contains("Unknown ticket id"))
        #expect(badID.error.contains("Question"), "the error carries the open board")
    }

    // MARK: - The stored form

    @Test("the body lands with the block and the status the answer opens")
    func storedBlockIsTheAnswer() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let id = try cli.addTicket(home: home, title: "Question")
        let answered = try run(home: home, [
            "ticket", "answer", "--id", id, "--status", "backlog",
            "--body", "B — needs re-refining.",
        ])
        #expect(answered.code == 0)

        let shown = try show(id: id, home: home)
        #expect(shown["status"] as? String == "backlog")
        #expect(shown["assignee"] is NSNull)
        let description = try #require(shown["description"] as? String)
        #expect(description.hasPrefix("## Answer — "), "a ticket with no brief still gets the block")
        #expect(description.contains("B — needs re-refining."))
        #expect(DescriptionBody.answered(description), "the store now reads it as answered")
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-answer-verb-\(UUID().uuidString)")
    }

    /// Run the CLI the way a host runs it: `TANDEM_AGENT` is exported, so
    /// `--assignee self` resolves.
    private func run(home: URL, _ arguments: [String], stdin: String = "") throws -> CLIHarness.Result {
        try cli.run(home: home, arguments, stdin: stdin, environment: ["TANDEM_AGENT": "sprint"])
    }

    /// One ticket, whole, through the CLI's own `show`.
    private func show(id: String, home: URL) throws -> [String: Any] {
        let result = try run(home: home, ["ticket", "show", "--id", id])
        #expect(result.code == 0)
        return try cli.object(from: result.output)
    }
}
