import Foundation
import Testing

/// The `session` verb group (spec `FC770B8A`): the CLI's face of the session
/// record — `start`, `end`, `link`, `list`, `show`.
///
/// The suite drives the binary the way an agent does, against a throwaway home:
/// the CLI is an adapter, and the adapter's contract is the JSON on stdout, the
/// text on stderr and the exit code.
@Suite("The session verbs")
struct SessionVerbTests {

    private let cli = CLIHarness()

    /// The session id the host exports — the record's handle, and the default
    /// every verb falls back to.
    private let sessionID = "ses_test"

    // MARK: - start

    @Test("`session start` records a running session: no outcome, no tickets")
    func startRecordsARunningSession() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let started = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        #expect(started.code == 0)

        let record = try cli.object(from: started.output)
        #expect(record["id"] as? String == sessionID)
        #expect(record["project"] as? String == "tandem")
        #expect(record["brief"] as? String == "spec the sessions work")
        #expect(record["status"] as? String == "running")
        #expect(record["outcome"] is NSNull, "a running session carries no outcome")
        #expect(record["endedAt"] is NSNull, "a running session has not ended")
        #expect(record["tickets"] as? [String] == [], "a new session carries no ticket")
    }

    @Test("`session start` again is the same record, a note, and no write")
    func startAgainIsTheSameRecord() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let first = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        let firstRecord = try cli.object(from: first.output)

        let again = try run(home: home, ["session", "start", "--project", "tandem", "--session", sessionID, "--brief", "spec the sessions work"])
        #expect(again.code == 0)

        let record = try cli.object(from: again.output)
        #expect(record["id"] as? String == sessionID)
        #expect(record["status"] as? String == "running")
        #expect(record["note"] as? String == "session \(sessionID) already recorded")
        #expect(record["startedAt"] as? String == firstRecord["startedAt"] as? String,
                "the second start writes nothing")
    }

    @Test("`session start` reads its brief from stdin, and refuses an empty one")
    func startReadsTheBriefFromStdin() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let started = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "-"], stdin: "a brief from stdin\n")
        #expect(started.code == 0)
        let record = try cli.object(from: started.output)
        #expect(record["brief"] as? String == "a brief from stdin")

        let empty = try run(home: home, ["session", "start", "--project", "tandem", "--session", "ses_two", "--brief", "-"], stdin: "")
        #expect(empty.code == 1)
        #expect(empty.error.contains("Session brief is empty"))
    }

    // MARK: - link

    @Test("`session link` links a ticket once, and refuses an id the store does not carry")
    func linkLinksATicketOnce() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Sessions CLI")
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let linked = try run(home: home, ["session", "link", "--ticket", ticket])
        #expect(linked.code == 0)
        #expect(try tickets(of: linked.output) == [ticket])

        let again = try run(home: home, ["session", "link", "--ticket", ticket])
        #expect(again.code == 0)
        #expect(try tickets(of: again.output) == [ticket], "a ticket is linked once")

        let unknown = try run(home: home, ["session", "link", "--ticket", "00000000-0000-0000-0000-000000000000"])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown ticket id"))

        // A malformed id is the same failure, not a crash.
        let malformed = try run(home: home, ["session", "link", "--ticket", "not-a-uuid"])
        #expect(malformed.code == 1)
        #expect(malformed.error.contains("Unknown ticket id"))
    }

    // MARK: - end

    @Test("`session end` stamps the outcome once, and refuses to restate it")
    func endStampsTheOutcomeOnce() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"])
        #expect(ended.code == 0)
        let record = try cli.object(from: ended.output)
        #expect(record["status"] as? String == "ended")
        #expect(record["outcome"] as? String == "succeeded")
        #expect(record["summary"] as? String == "spec filed")
        #expect(record["endedAt"] as? String != nil, "the end stamps the time")

        let again = try run(home: home, ["session", "end", "--outcome", "failed", "--summary", "second try"])
        #expect(again.code == 0)
        let kept = try cli.object(from: again.output)
        #expect(kept["outcome"] as? String == "succeeded", "the first outcome is the fact")
        #expect(kept["summary"] as? String == "spec filed")
        #expect(kept["note"] as? String == "session \(sessionID) already ended")
    }

    @Test("`session end` without an outcome ends the session as `unknown`")
    func endWithoutAnOutcomeIsUnknown() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let ended = try run(home: home, ["session", "end"])
        #expect(ended.code == 0)
        #expect(try cli.object(from: ended.output)["outcome"] as? String == "unknown")
    }

    @Test("`session end` refuses an outcome that is not one of the four")
    func endRefusesAnUnknownOutcome() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let bad = try run(home: home, ["session", "end", "--outcome", "bogus"])
        #expect(bad.code == 1)
        #expect(bad.error.contains("succeeded"))
        #expect(bad.error.contains("failed"))
        #expect(bad.error.contains("interrupted"))
        #expect(bad.error.contains("unknown"))
    }

    @Test("`session end` reads its summary from stdin, and refuses an empty one")
    func endReadsTheSummaryFromStdin() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "-"], stdin: "spec filed\n")
        #expect(ended.code == 0)
        #expect(try cli.object(from: ended.output)["summary"] as? String == "spec filed")

        let empty = try run(home: home, ["session", "end", "--session", "ses_two", "--outcome", "succeeded", "--summary", "-"], stdin: "")
        #expect(empty.code == 1)
        #expect(empty.error.contains("Session summary is empty"))
    }

    // MARK: - usage

    /// The usage flags, as the opencode adapter passes them.
    private let usageFlags = [
        "--tokens-input", "12345",
        "--tokens-output", "4567",
        "--tokens-reasoning", "567",
        "--tokens-cache-read", "89012",
        "--tokens-cache-write", "1234",
        "--cost", "0.42",
        "--model", "z-ai/glm-5.3-flash",
    ]

    @Test("`session end` with the usage flags stores the usage on the record")
    func endStoresTheUsage() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"] + usageFlags)
        #expect(ended.code == 0)

        let shown = try run(home: home, ["session", "show"])
        #expect(shown.code == 0)
        let record = try cli.object(from: shown.output)
        let tokens = try #require(record["tokens"] as? [String: Any])
        #expect(tokens["input"] as? Int == 12345)
        #expect(tokens["output"] as? Int == 4567)
        #expect(tokens["reasoning"] as? Int == 567)
        #expect(tokens["cacheRead"] as? Int == 89012)
        #expect(tokens["cacheWrite"] as? Int == 1234)
        #expect(record["cost"] as? Double == 0.42)
        #expect(record["model"] as? String == "z-ai/glm-5.3-flash")
    }

    @Test("absent usage flags leave the record without usage — today's behaviour")
    func endWithoutUsageStoresNone() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"])
        #expect(ended.code == 0)

        let shown = try run(home: home, ["session", "show"])
        let record = try cli.object(from: shown.output)
        #expect(record["tokens"] == nil, "no usage flags, no tokens key")
        #expect(record["cost"] == nil)
        #expect(record["model"] == nil)
    }

    @Test("`session end` with usage posts one comment, under the record's agent, on every ticket the record links")
    func endPostsUsageCommentsOnLinkedTickets() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let first = try cli.addTicket(home: home, title: "First carried")
        let second = try cli.addTicket(home: home, title: "Second carried")
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--agent", "builder", "--brief", "spec the sessions work"])
        _ = try run(home: home, ["session", "link", "--ticket", first])
        _ = try run(home: home, ["session", "link", "--ticket", second])

        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"] + usageFlags)
        #expect(ended.code == 0)

        for ticket in [first, second] {
            let listed = try run(home: home, ["ticket", "comment", "list", "--id", ticket])
            #expect(listed.code == 0)
            let comments = try #require(try cli.object(from: listed.output)["comments"] as? [[String: Any]])
            #expect(comments.count == 1, "exactly one comment on \(ticket)")
            #expect(comments.first?["author"] as? String == "builder", "the comment is signed by the record's agent")
            let body = try #require(comments.first?["body"] as? String)
            #expect(body.split(separator: "\n").map(String.init) == [
                "Usage — \(sessionID) · z-ai/glm-5.3-flash",
                "input 12,345",
                "output 4,567",
                "cache read 89,012",
                "cache write 1,234",
                "reasoning 567",
                "cost $0.42",
            ], "the body is one metric per line, with thousands separators")
        }
    }

    @Test("a record ended without `--agent` posts the usage comment under author `usage`")
    func endWithoutAgentFallsBackToTheUsageAuthor() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        _ = try run(home: home, ["session", "link", "--ticket", ticket])

        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"] + usageFlags)
        #expect(ended.code == 0)

        let listed = try run(home: home, ["ticket", "comment", "list", "--id", ticket])
        let comments = try #require(try cli.object(from: listed.output)["comments"] as? [[String: Any]])
        #expect(comments.count == 1)
        #expect(comments.first?["author"] as? String == "usage", "no agent on the record, the fallback names the line")
    }

    @Test("the usage body leaves pieces the harness did not report out, and groups the counts it did")
    func usageBodyOmitsUnreportedPiecesAndGroupsCounts() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--agent", "reviewer", "--brief", "spec the sessions work"])
        _ = try run(home: home, ["session", "link", "--ticket", ticket])

        let ended = try run(home: home, [
            "session", "end", "--outcome", "succeeded", "--summary", "spec filed",
            "--tokens-input", "5346816",
            "--tokens-cache-write", "0",
            "--cost", "0.22",
        ])
        #expect(ended.code == 0)

        let listed = try run(home: home, ["ticket", "comment", "list", "--id", ticket])
        let comments = try #require(try cli.object(from: listed.output)["comments"] as? [[String: Any]])
        #expect(comments.first?["author"] as? String == "reviewer")
        let body = try #require(comments.first?["body"] as? String)
        #expect(body.split(separator: "\n").map(String.init) == [
            "Usage — \(sessionID)",
            "input 5,346,816",
            "cache write 0",
            "cost $0.22",
        ], "unreported pieces are absent, never a zero dressed up as a count; a reported zero stays")
    }

    @Test("`session end` with usage and no linked tickets stores the usage and posts nothing")
    func endWithUsageAndNoTicketsPostsNothing() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        let ended = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"] + usageFlags)
        #expect(ended.code == 0)

        let shown = try run(home: home, ["session", "show"])
        #expect(try cli.object(from: shown.output)["model"] as? String == "z-ai/glm-5.3-flash",
                "the usage is stored even with nothing to comment on")
    }

    @Test("`session end` stamps once, so the usage comment cannot double-post")
    func endStampsOnceSoTheCommentCannotDoublePost() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        _ = try run(home: home, ["session", "link", "--ticket", ticket])

        _ = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "spec filed"] + usageFlags)
        let again = try run(home: home, ["session", "end", "--outcome", "failed", "--summary", "second try"] + usageFlags)
        #expect(again.code == 0)
        #expect(try cli.object(from: again.output)["note"] as? String == "session \(sessionID) already ended")

        let listed = try run(home: home, ["ticket", "comment", "list", "--id", ticket])
        let comments = try #require(try cli.object(from: listed.output)["comments"] as? [[String: Any]])
        #expect(comments.count == 1, "the second end posted nothing")
    }

    @Test("a usage flag that is not a number is a domain failure naming the flag")
    func aBadUsageValueIsADomainFailure() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let bad = try run(home: home, ["session", "end", "--outcome", "succeeded", "--tokens-input", "abc"])
        #expect(bad.code == 1)
        #expect(bad.error.contains("--tokens-input"))
        #expect(bad.error.contains("abc"))

        let negative = try run(home: home, ["session", "end", "--outcome", "succeeded", "--cost", "-1"])
        #expect(negative.code == 1)
        #expect(negative.error.contains("--cost"))
    }

    // MARK: - list and show

    @Test("`session list` lists the records newest first, and filters by status")
    func listIsNewestFirstAndFiltersByStatus() async throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--session", "ses_old", "--brief", "older"])
        // The stamps are whole seconds, and records that share one keep the order
        // they were recorded in — so the two need a second between them for
        // "newest first" to mean anything.
        try await Task.sleep(for: .seconds(1.1))
        _ = try run(home: home, ["session", "start", "--project", "tandem", "--session", "ses_new", "--brief", "newer"])

        let listed = try run(home: home, ["session", "list"])
        #expect(listed.code == 0)
        let object = try cli.object(from: listed.output)
        let sessions = try #require(object["sessions"] as? [[String: Any]])
        #expect(sessions.map { $0["id"] as? String } == ["ses_new", "ses_old"], "newest first")

        _ = try run(home: home, ["session", "end", "--session", "ses_new", "--outcome", "succeeded"])
        let running = try run(home: home, ["session", "list", "--status", "running"])
        #expect(running.code == 0)
        let runningObject = try cli.object(from: running.output)
        #expect(try #require(runningObject["sessions"] as? [[String: Any]]).map { $0["id"] as? String } == ["ses_old"])
    }

    @Test("`session list --status` refuses a status that is not one of the two")
    func listRefusesAnUnknownStatus() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let bad = try run(home: home, ["session", "list", "--status", "bogus"])
        #expect(bad.code == 1)
        #expect(bad.error.contains("running"))
        #expect(bad.error.contains("ended"))
    }

    @Test("`session show` prints the record, and refuses an id the file does not carry")
    func showPrintsTheRecord() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])

        let shown = try run(home: home, ["session", "show", "--session", sessionID])
        #expect(shown.code == 0)
        #expect(try cli.object(from: shown.output)["id"] as? String == sessionID)

        let unknown = try run(home: home, ["session", "show", "--session", "ses_nope"])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown session id"))
    }

    @Test("`session show` names the notes the session wrote, and only those")
    func showNamesTheNotes() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--brief", "spec the sessions work"])
        let captured = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem", "--session", sessionID], stdin: "# A note\n")
        #expect(captured.code == 0)
        let path = try #require(try cli.object(from: captured.output)["path"] as? String)

        let shown = try run(home: home, ["session", "show", "--session", sessionID])
        #expect(shown.code == 0)
        let notes = try #require(try cli.object(from: shown.output)["notes"] as? [String])
        #expect(notes == [path], "the capture the session wrote is named")

        // Another session's capture is not this session's note.
        _ = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem", "--session", "ses_other"], stdin: "# Another\n")
        let again = try run(home: home, ["session", "show", "--session", sessionID])
        #expect(try #require(try cli.object(from: again.output)["notes"] as? [String]) == [path])
    }

    // MARK: - The defaults

    @Test("a session id is the host's, and an unset one is a failure that names it")
    func theSessionIdIsTheHosts() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let unset = try cli.run(home: home, ["session", "start", "--project", "tandem", "--brief", "x"], environment: ["OPENCODE_SESSION_ID": nil])
        #expect(unset.code == 1)
        #expect(unset.error.contains("OPENCODE_SESSION_ID"), "the failure names the variable")
    }

    @Test("the agent is the host's, and the project is the working directory's")
    func theAgentAndProjectAreTheHosts() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let started = try cli.run(
            home: home,
            ["session", "start", "--brief", "defaults"],
            environment: ["OPENCODE_SESSION_ID": sessionID, "TANDEM_AGENT": "refiner"])
        #expect(started.code == 0)
        let record = try cli.object(from: started.output)
        #expect(record["agent"] as? String == "refiner")
        #expect(record["project"] as? String == currentProject(), "the cwd's last path component")
    }

    // MARK: - The usage text

    @Test("the usage text lists the five session verbs")
    func helpListsTheSessionVerbs() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let help = try run(home: home, ["--help"])
        #expect(help.code == 0)
        for verb in ["start", "end", "link", "list", "show"] {
            #expect(help.output.contains("tandem-cli session \(verb)"), "the usage lists `session \(verb)`")
        }
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-session-verb-\(UUID().uuidString)")
    }

    /// Run the CLI the way a host runs it: the session id is exported.
    private func run(home: URL, _ arguments: [String], stdin: String = "") throws -> CLIHarness.Result {
        try cli.run(home: home, arguments, stdin: stdin, environment: ["OPENCODE_SESSION_ID": sessionID])
    }

    /// The ticket ids a session record carries.
    private func tickets(of json: String) throws -> [String] {
        try cli.object(from: json)["tickets"] as? [String] ?? []
    }

    /// The working directory's last path component — the `note capture`
    /// project default, and the session group's.
    private func currentProject() -> String {
        URL(fileURLWithPath: FileManager.default.currentDirectoryPath).lastPathComponent
    }
}
