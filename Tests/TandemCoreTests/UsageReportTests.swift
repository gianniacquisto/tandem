import Foundation
import Testing

/// `Scripts/usage-report.sh` — the harness adapter that ends this session's
/// record with the usage opencode measured (spec `F68948C0`).
///
/// The case that matters here is the re-dispatched pass (`RET-43`): the harness
/// resumes an opencode session whose tandem record the pass before it already
/// ended. A record stamps once, so that record cannot take this pass's usage —
/// and exiting 0 without writing it loses the pass. The wrapper therefore mints
/// the pass its own record, keyed to the same opencode id with a `-<n>` suffix,
/// carries the ended record's tickets onto it, and writes the usage there.
@Suite("The usage wrapper")
struct UsageReportTests {

    /// The repo root, from this file's own path: three levels up.
    private static let script = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root
        .appendingPathComponent("Scripts/usage-report.sh")

    private let cli = CLIHarness()

    /// The id opencode exports — the first record's handle, and the key every
    /// continuation record extends.
    private let sessionID = "ses_test"

    // MARK: - the terminal record

    @Test("a pass whose record already ended gets its own record — `<id>-2` — and the usage lands on it")
    func theTerminalRecordGetsThePassItsOwnRecord() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        #expect(try run(home: home, [
            "session", "start", "--project", "tandem", "--agent", "builder", "--brief", "the gate pass",
        ]).code == 0)
        #expect(try run(home: home, ["session", "link", "--ticket", ticket]).code == 0)
        // The gate pass ends, stamped without usage: the record a resumed pass
        // finds terminal.
        #expect(try run(home: home, [
            "session", "end", "--outcome", "succeeded", "--summary", "gate pass merged",
        ]).code == 0)

        let db = try opencodeDatabase(for: sessionID)
        defer { try? FileManager.default.removeItem(at: db) }

        let ended = try wrapper(home: home, db: db, ["--outcome", "succeeded"], stdin: "the merge pass\n")
        #expect(ended.code == 0, "the wrapper ends the pass: \(ended.error)")

        // The pass's record: the same opencode id with the `-2` suffix, carrying
        // the ended record's agent, project and ticket.
        let cont = try cli.object(from: try run(home: home, ["session", "show", "--session", "\(sessionID)-2"]).output)
        #expect(cont["status"] as? String == "ended")
        #expect(cont["outcome"] as? String == "succeeded")
        #expect(cont["summary"] as? String == "the merge pass", "the summary is the pass's own")
        #expect(cont["agent"] as? String == "builder", "the ended record's agent carries over — the usage comment's signature")
        #expect(cont["project"] as? String == "tandem")
        #expect(cont["tickets"] as? [String] == [ticket], "the ended record's ticket carries over, so the usage lands on the board")
        #expect(cont["model"] as? String == "opencode-go/deepseek-v4.1-flash", "the usage is written")
        #expect(cont["cost"] as? Double == 0.42)
        let tokens = try #require(cont["tokens"] as? [String: Any])
        #expect(tokens["input"] as? Int == 12345)
        #expect(tokens["cacheRead"] as? Int == 89012)

        // The first record is the fact it was: the continuation does not restate
        // it (a record stamps once).
        let first = try cli.object(from: try run(home: home, ["session", "show", "--session", sessionID]).output)
        #expect(first["summary"] as? String == "gate pass merged")
        #expect(first["model"] == nil, "the terminal record gains no usage")

        // The usage comment is on the carried ticket, signed by the agent.
        let comments = try cli.comments(home: home, id: ticket)
        #expect(comments.count == 1)
        #expect(comments.first?["author"] as? String == "builder")
        #expect((comments.first?["body"] as? String)?.split(separator: "\n").first.map(String.init)
            == "Usage — \(sessionID)-2 · opencode-go/deepseek-v4.1-flash",
                "the comment names the record that carries the usage")
    }

    @Test("a second re-dispatch in the same opencode session gets the next suffix, not the first pass's record")
    func theSecondRedispatchGetsTheNextSuffix() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--agent", "builder", "--brief", "the gate pass"])
        _ = try run(home: home, ["session", "end", "--outcome", "succeeded", "--summary", "gate pass merged"])

        let db = try opencodeDatabase(for: sessionID)
        defer { try? FileManager.default.removeItem(at: db) }

        #expect(try wrapper(home: home, db: db, ["--outcome", "succeeded"], stdin: "second pass\n").code == 0)
        #expect(try wrapper(home: home, db: db, ["--outcome", "succeeded"], stdin: "third pass\n").code == 0)

        let listed = try cli.object(from: try run(home: home, ["session", "list"]).output)
        let ids = try #require(listed["sessions"] as? [[String: Any]]).compactMap { $0["id"] as? String }
        #expect(Set(ids) == [sessionID, "\(sessionID)-2", "\(sessionID)-3"], "each pass has its own record")
        let third = try cli.object(from: try run(home: home, ["session", "show", "--session", "\(sessionID)-3"]).output)
        #expect(third["summary"] as? String == "third pass")
    }

    @Test("a pass that loses the suffix race takes the next suffix instead of writing into the winner's record")
    func theRacingPassTakesTheNextSuffix() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        #expect(try run(home: home, [
            "session", "start", "--project", "my project", "--agent", "builder", "--brief", "the gate pass",
        ]).code == 0)
        #expect(try run(home: home, ["session", "link", "--ticket", ticket]).code == 0)
        #expect(try run(home: home, [
            "session", "end", "--outcome", "succeeded", "--summary", "gate pass merged",
        ]).code == 0)

        let db = try opencodeDatabase(for: sessionID)
        defer { try? FileManager.default.removeItem(at: db) }

        // The other end of the race wins `-2` between this pass reading the
        // record and its mint: the shim creates that record, then hands the
        // wrapper's own `start` through — which answers with the existing
        // record and a note, at exit 0.
        let racing = try shim("""
        if [ "$1" = "session" ] && [ "$2" = "start" ] && [ "$4" = "\(sessionID)-2" ]; then
            "$REAL" session start --session "\(sessionID)-2" --project tandem --brief "the other pass" >/dev/null
        fi
        """)

        let ended = try wrapper(home: home, db: db, cli: racing, ["--outcome", "succeeded"], stdin: "the merge pass\n")
        #expect(ended.code == 0, "the wrapper ends the pass: \(ended.error)")

        // The winner's record is untouched: no usage, its own brief, still
        // running — the other pass has not ended yet.
        let winner = try cli.object(from: try run(home: home, ["session", "show", "--session", "\(sessionID)-2"]).output)
        #expect(winner["brief"] as? String == "the other pass", "the loser must not mint over the winner's record")
        #expect(winner["status"] as? String == "running", "the loser must not stamp the winner's record")
        #expect(winner["model"] == nil, "the loser's usage must not land on the winner's record")

        // The loser's own record carries the usage, and the project's space
        // survived the mint as one argument.
        let cont = try cli.object(from: try run(home: home, ["session", "show", "--session", "\(sessionID)-3"]).output)
        #expect(cont["summary"] as? String == "the merge pass")
        #expect(cont["project"] as? String == "my project", "a project name with a space stays one argument")
        #expect(cont["model"] as? String == "opencode-go/deepseek-v4.1-flash", "the usage lands on the pass's own record")
        #expect(cont["tickets"] as? [String] == [ticket])

        let comments = try cli.comments(home: home, id: ticket)
        #expect(comments.count == 1)
        #expect((comments.first?["body"] as? String)?.split(separator: "\n").first.map(String.init)
            == "Usage — \(sessionID)-3 · opencode-go/deepseek-v4.1-flash")
    }

    @Test("a link that fails is named on stderr and does not cost the usage")
    func aFailedLinkIsSurfaced() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let ticket = try cli.addTicket(home: home, title: "Carried")
        #expect(try run(home: home, [
            "session", "start", "--project", "tandem", "--agent", "builder", "--brief", "the gate pass",
        ]).code == 0)
        #expect(try run(home: home, ["session", "link", "--ticket", ticket]).code == 0)
        #expect(try run(home: home, [
            "session", "end", "--outcome", "succeeded", "--summary", "gate pass merged",
        ]).code == 0)

        let db = try opencodeDatabase(for: sessionID)
        defer { try? FileManager.default.removeItem(at: db) }

        // The link is the one call that can keep the usage comment off the
        // ticket; a wrapper that swallows it reports a success nobody can see.
        let failing = try shim("""
        if [ "$1" = "session" ] && [ "$2" = "link" ]; then
            echo "tandem-cli: unknown ticket" >&2
            exit 1
        fi
        """)

        let ended = try wrapper(home: home, db: db, cli: failing, ["--outcome", "succeeded"], stdin: "the merge pass\n")
        #expect(ended.code == 0, "the usage is still written: \(ended.error)")
        #expect(ended.error.contains("warning"), "the failed link is named on stderr: \(ended.error)")
        #expect(ended.error.contains(ticket), "the warning names the ticket whose comment is missing")

        let cont = try cli.object(from: try run(home: home, ["session", "show", "--session", "\(sessionID)-2"]).output)
        #expect(cont["model"] as? String == "opencode-go/deepseek-v4.1-flash", "the usage lands despite the failed link")
        #expect(try cli.comments(home: home, id: ticket).isEmpty, "no usage comment — and the wrapper said so")
    }

    @Test("a running record takes the usage itself — the wrapper mints nothing")
    func theRunningRecordTakesTheUsage() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        _ = try run(home: home, ["session", "start", "--project", "tandem", "--agent", "builder", "--brief", "the pass"])

        let db = try opencodeDatabase(for: sessionID)
        defer { try? FileManager.default.removeItem(at: db) }

        let ended = try wrapper(home: home, db: db, ["--outcome", "succeeded"], stdin: "the pass landed\n")
        #expect(ended.code == 0, "\(ended.error)")

        let record = try cli.object(from: try run(home: home, ["session", "show", "--session", sessionID]).output)
        #expect(record["model"] as? String == "opencode-go/deepseek-v4.1-flash")
        #expect(record["summary"] as? String == "the pass landed")

        let listed = try cli.object(from: try run(home: home, ["session", "list"]).output)
        let ids = try #require(listed["sessions"] as? [[String: Any]]).compactMap { $0["id"] as? String }
        #expect(ids == [sessionID], "no continuation record where the first one is still live")
    }

    // MARK: - helpers

    /// Run the CLI as the harness would: against the throwaway home, with the
    /// opencode session id the wrapper resolves — never the test process's own.
    private func run(home: URL, _ arguments: [String]) throws -> CLIHarness.Result {
        try cli.run(home: home, arguments, environment: ["OPENCODE_SESSION_ID": sessionID])
    }

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-usage-wrapper-\(UUID().uuidString)")
    }

    /// Run the wrapper against the throwaway home, with `stdin` piped in — the
    /// summary. The opencode database is the fixture; the CLI is the binary the
    /// tests run, or the `cli` the test shims a call through.
    private func wrapper(
        home: URL, db: URL, cli given: URL? = nil, _ arguments: [String], stdin text: String
    ) throws -> CLIHarness.Result {
        let process = Process()
        process.executableURL = Self.script
        process.arguments = arguments
        var environment = ProcessInfo.processInfo.environment
        environment["TANDEM_HOME"] = home.path
        environment["TANDEM_CLI"] = (given ?? cli.url).path
        environment["OPENCODE_SESSION_ID"] = sessionID
        environment["OPENCODE_DB"] = db.path
        process.environment = environment
        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error
        try process.run()
        input.fileHandleForWriting.write(Data(text.utf8))
        input.fileHandleForWriting.closeFile()
        let out = output.fileHandleForReading.readDataToEndOfFile()
        let err = error.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return CLIHarness.Result(
            code: process.terminationStatus,
            output: String(decoding: out, as: UTF8.self),
            error: String(decoding: err, as: UTF8.self))
    }

    /// A CLI shim: `body` runs first, the real binary (`$REAL`) takes every call
    /// it does not intercept. The wrapper is a shell script driven by
    /// `$TANDEM_CLI`, so a race or a failing call is a shim here, not a sleep.
    private func shim(_ body: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-shim-\(UUID().uuidString)")
        let script = """
        #!/bin/sh
        REAL='\(cli.url.path)'
        \(body)
        exec "$REAL" "$@"
        """
        try script.write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: url.path)
        return url
    }

    /// An opencode database carrying one session row for `session` — the shape
    /// the wrapper reads: the five token counts, a cost and a JSON model.
    private func opencodeDatabase(for session: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("opencode-\(UUID().uuidString).db")
        let sql = """
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            tokens_input INTEGER,
            tokens_output INTEGER,
            tokens_reasoning INTEGER,
            tokens_cache_read INTEGER,
            tokens_cache_write INTEGER,
            cost REAL,
            model TEXT);
        INSERT INTO session_v2 VALUES ('\(session)', 12345, 4567, 567, 89012, 1234, 0.42,
            '{"id":"opencode-go/deepseek-v4.1-flash"}');
        """
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
        process.arguments = [url.path]
        let input = Pipe()
        process.standardInput = input
        try process.run()
        input.fileHandleForWriting.write(Data(sql.utf8))
        input.fileHandleForWriting.closeFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw CocoaError(.fileWriteUnknown)
        }
        return url
    }
}
