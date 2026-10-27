import Foundation
import Testing

/// The `sprint` verb group (spec `4F746EFE`, R2): the CLI's face of the sprint
/// record — `open`, `ticket`, `run`, `close`, `list`, `show`.
///
/// The suite drives the binary the way an agent does, against a throwaway home:
/// the CLI is an adapter, and the adapter's contract is the JSON on stdout, the
/// text on stderr and the exit code. The conventions are the `session` group's,
/// copied not invented — the defaults that belong to a process (`--session` →
/// `$OPENCODE_SESSION_ID`) are resolved in the adapter, an unset one is a
/// domain failure, and an empty stdin body is one too.
@Suite("The sprint verbs")
struct SprintVerbTests {

    private let cli = CLIHarness()

    /// The session id the host exports — the opening session's default.
    private let sessionID = "ses_test"

    // MARK: - open

    @Test("`sprint open` records a sprint: per-lane number, serial default, the host's session")
    func openRecordsASprint() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let opened = try run(home: home, ["sprint", "open", "--lane", "Tandem"])
        #expect(opened.code == 0)

        let record = try cli.object(from: opened.output)
        #expect(record["lane"] as? String == "Tandem")
        #expect(record["number"] as? Int == 1)
        #expect(record["session"] as? String == sessionID, "the opening session defaults to the host's id")
        #expect(record["mode"] as? String == "serial", "serial is the skill's default, stored either way")
        #expect(record["batch"] as? [String] == [])
        #expect(record["runs"] as? [String] == [])
        #expect(record["status"] as? String == "open")
        #expect(record["close"] is NSNull, "an open sprint carries no close")
        #expect(record["closedAt"] is NSNull, "an open sprint has not closed")
        #expect(record["openedAt"] as? String != nil)
    }

    @Test("`sprint open` again is the same record, a note, and no write")
    func openAgainIsTheSameRecord() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let first = try run(home: home, ["sprint", "open", "--lane", "Tandem"])
        let firstRecord = try cli.object(from: first.output)

        let again = try run(home: home, [
            "sprint", "open", "--lane", "Tandem", "--mode", "parallel", "--session", "ses_other",
        ])
        #expect(again.code == 0)

        let record = try cli.object(from: again.output)
        #expect(record["lane"] as? String == "Tandem")
        #expect(record["number"] as? Int == 1)
        #expect(record["session"] as? String == sessionID, "the first open's session is the fact")
        #expect(record["mode"] as? String == "serial")
        #expect(record["note"] as? String == "Sprint 1 — Tandem is already open")

        #expect(record["openedAt"] as? String == firstRecord["openedAt"] as? String,
                "the second open writes nothing")
    }

    @Test("`sprint open` numbers per lane, and takes `--session` from the flag")
    func openNumbersPerLane() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])
        let other = try run(home: home, [
            "sprint", "open", "--lane", "code-rank", "--mode", "parallel", "--session", "ses_explicit",
        ])
        #expect(other.code == 0)
        let record = try cli.object(from: other.output)
        #expect(record["lane"] as? String == "code-rank")
        #expect(record["number"] as? Int == 1, "numbering is per-lane")
        #expect(record["mode"] as? String == "parallel")
        #expect(record["session"] as? String == "ses_explicit")
    }

    @Test("an unset OPENCODE_SESSION_ID on `sprint open` is a failure that names it")
    func unsetSessionIDOnOpenFails() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let unset = try cli.run(
            home: home,
            ["sprint", "open", "--lane", "Tandem"],
            environment: ["OPENCODE_SESSION_ID": nil])
        #expect(unset.code == 1)
        #expect(unset.error.contains("OPENCODE_SESSION_ID"), "the failure names the variable")
    }

    @Test("`sprint open` refuses a mode that is not serial or parallel")
    func openRefusesAnUnknownMode() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let bad = try run(home: home, ["sprint", "open", "--lane", "Tandem", "--mode", "bogus"])
        #expect(bad.code == 1)
        #expect(bad.error.contains("serial"))
        #expect(bad.error.contains("parallel"))
    }

    // MARK: - ticket

    @Test("`sprint ticket` adds to the open sprint's batch, and dedupes")
    func ticketAddsToTheBatchOnce() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "The CLI")
        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let added = try run(home: home, ["sprint", "ticket", "--lane", "Tandem", "--ticket", ticket])
        #expect(added.code == 0)
        #expect(try cli.object(from: added.output)["batch"] as? [String] == [ticket])

        let again = try run(home: home, ["sprint", "ticket", "--lane", "Tandem", "--ticket", ticket])
        #expect(again.code == 0)
        #expect(try cli.object(from: again.output)["batch"] as? [String] == [ticket], "a ticket is batched once")
    }

    @Test("`sprint ticket` refuses an id the store does not carry, and a malformed one")
    func ticketRefusesAnUnknownID() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let unknown = try run(home: home, [
            "sprint", "ticket", "--lane", "Tandem",
            "--ticket", "00000000-0000-0000-0000-000000000000",
        ])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown ticket id"))

        let malformed = try run(home: home, ["sprint", "ticket", "--lane", "Tandem", "--ticket", "not-a-uuid"])
        #expect(malformed.code == 1)
        #expect(malformed.error.contains("Unknown ticket id"), "a malformed id is the same failure, not a crash")
    }

    // MARK: - run

    @Test("`sprint run` appends a dated entry from stdin, and refuses an empty one")
    func runAppendsADatedEntry() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let ran = try run(home: home, ["sprint", "run", "--lane", "Tandem", "--note", "-"], stdin: "shipped the batch\n")
        #expect(ran.code == 0)
        let runs = try #require(try cli.object(from: ran.output)["runs"] as? [String])
        #expect(runs.count == 1)
        #expect(runs.first?.hasSuffix(" — shipped the batch") == true, "the entry is dated by the core, the note is the caller's")

        let inline = try run(home: home, ["sprint", "run", "--lane", "Tandem", "--note", "reviewer took it"])
        #expect(inline.code == 0)
        #expect(try #require(try cli.object(from: inline.output)["runs"] as? [String]).count == 2)

        let empty = try run(home: home, ["sprint", "run", "--lane", "Tandem", "--note", "-"], stdin: "")
        #expect(empty.code == 1)
        #expect(empty.error.contains("Sprint note is empty"))

        let missing = try run(home: home, ["sprint", "run", "--lane", "Tandem"])
        #expect(missing.code == 1)
        #expect(missing.error.contains("Sprint note is empty"), "a missing --note is the empty note the core rejects")
    }

    // MARK: - close

    @Test("`sprint close` stamps once, and a second close keeps the first text")
    func closeStampsOnce() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let closed = try run(home: home, ["sprint", "close", "--lane", "Tandem", "--note", "two landed, one slipped"])
        #expect(closed.code == 0)
        let record = try cli.object(from: closed.output)
        #expect(record["status"] as? String == "closed")
        #expect(record["close"] as? String == "two landed, one slipped")
        #expect(record["closedAt"] as? String != nil)

        let again = try run(home: home, ["sprint", "close", "--lane", "Tandem", "--note", "second try"])
        #expect(again.code == 0)
        let kept = try cli.object(from: again.output)
        #expect(kept["close"] as? String == "two landed, one slipped", "the first close is the fact")
        #expect(kept["note"] as? String == "Sprint 1 — Tandem is already closed")
    }

    @Test("`sprint close` without a note closes with the stamp alone")
    func closeWithoutANoteStampsAlone() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let closed = try run(home: home, ["sprint", "close", "--lane", "Tandem"])
        #expect(closed.code == 0)
        let record = try cli.object(from: closed.output)
        #expect(record["status"] as? String == "closed")
        #expect(record["close"] is NSNull, "no note, no close text — the stamp alone")
    }

    @Test("`sprint close --note -` refuses an empty body")
    func closeRefusesAnEmptyStdinBody() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let empty = try run(home: home, ["sprint", "close", "--lane", "Tandem", "--note", "-"], stdin: "")
        #expect(empty.code == 1)
        #expect(empty.error.contains("Sprint note is empty"))
    }

    @Test("`sprint close --number` names the record; a closed one answers unchanged")
    func closeByNameLeavesTheOpenRecordAlone() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])
        _ = try run(home: home, ["sprint", "close", "--lane", "Tandem", "--note", "first"])
        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let named = try run(home: home, ["sprint", "close", "--lane", "Tandem", "--number", "1"])
        #expect(named.code == 0)
        let record = try cli.object(from: named.output)
        #expect(record["number"] as? Int == 1, "the number names the record, not the lane's open one")
        #expect(record["close"] as? String == "first", "the closed record keeps its close")
        #expect(record["note"] as? String == "Sprint 1 — Tandem is already closed")

        let untouched = try cli.object(
            from: run(home: home, ["sprint", "show", "--lane", "Tandem", "--number", "2"]).output)
        #expect(untouched["status"] as? String == "open", "the open record is not the one the flag named")
    }

    // MARK: - The open sprint is the handle

    @Test("`sprint ticket`/`run` on a lane with no open sprint is a failure that names it")
    func noOpenSprintIsAFailure() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "The CLI")

        let ran = try run(home: home, ["sprint", "run", "--lane", "Tandem", "--note", "x"])
        #expect(ran.code == 1)
        #expect(ran.error.contains("No sprint is open in lane \"Tandem\""))

        let batched = try run(home: home, ["sprint", "ticket", "--lane", "Tandem", "--ticket", ticket])
        #expect(batched.code == 1)
        #expect(batched.error.contains("No sprint is open in lane \"Tandem\""))
    }

    // MARK: - list and show

    @Test("`sprint list` lists newest first, and filters by lane and status")
    func listFiltersByLaneAndStatus() async throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])
        // The stamps are whole seconds, and records that share one keep the order
        // they were recorded in — so the two need a second between them for
        // "newest first" to mean anything.
        try await Task.sleep(for: .seconds(1.1))
        _ = try run(home: home, ["sprint", "open", "--lane", "code-rank"])
        _ = try run(home: home, ["sprint", "close", "--lane", "code-rank"])

        let listed = try run(home: home, ["sprint", "list"])
        #expect(listed.code == 0)
        let sprints = try #require(try cli.object(from: listed.output)["sprints"] as? [[String: Any]])
        #expect(sprints.map { $0["lane"] as? String } == ["code-rank", "Tandem"], "newest first")

        let lane = try run(home: home, ["sprint", "list", "--lane", "Tandem"])
        #expect(try #require(try cli.object(from: lane.output)["sprints"] as? [[String: Any]]).count == 1)

        let open = try run(home: home, ["sprint", "list", "--status", "open"])
        #expect(try #require(try cli.object(from: open.output)["sprints"] as? [[String: Any]]).map { $0["lane"] as? String } == ["Tandem"])

        let closed = try run(home: home, ["sprint", "list", "--status", "closed"])
        #expect(try #require(try cli.object(from: closed.output)["sprints"] as? [[String: Any]]).map { $0["lane"] as? String } == ["code-rank"])
    }

    @Test("`sprint list --status` refuses a status that is not one of the two")
    func listRefusesAnUnknownStatus() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let bad = try run(home: home, ["sprint", "list", "--status", "bogus"])
        #expect(bad.code == 1)
        #expect(bad.error.contains("open"))
        #expect(bad.error.contains("closed"))
    }

    @Test("`sprint show --lane --number` finds the record `list` returned, and refuses an unknown handle")
    func showFindsTheRecordByThePair() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["sprint", "open", "--lane", "Tandem"])

        let shown = try run(home: home, ["sprint", "show", "--lane", "Tandem", "--number", "1"])
        #expect(shown.code == 0)
        let record = try cli.object(from: shown.output)
        #expect(record["lane"] as? String == "Tandem")
        #expect(record["number"] as? Int == 1)

        let unknown = try run(home: home, ["sprint", "show", "--lane", "Tandem", "--number", "5"])
        #expect(unknown.code == 1)
        #expect(unknown.error.contains("Unknown sprint"))

        let notANumber = try run(home: home, ["sprint", "show", "--lane", "Tandem", "--number", "abc"])
        #expect(notANumber.code == 1)
        #expect(notANumber.error.contains("whole number"))
    }

    // MARK: - The usage text

    @Test("the usage text lists the six sprint verbs")
    func helpListsTheSprintVerbs() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let help = try run(home: home, ["--help"])
        #expect(help.code == 0)
        for verb in ["open", "ticket", "run", "close", "list", "show"] {
            #expect(help.output.contains("tandem-cli sprint \(verb)"), "the usage lists `sprint \(verb)`")
        }
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-sprint-verb-\(UUID().uuidString)")
    }

    /// Run the CLI the way a host runs it: the session id is exported.
    private func run(home: URL, _ arguments: [String], stdin: String = "") throws -> CLIHarness.Result {
        try cli.run(home: home, arguments, stdin: stdin, environment: ["OPENCODE_SESSION_ID": sessionID])
    }
}