import Foundation
import Testing

/// The usage wrapper (RET-2, spec F68948C0): `Scripts/usage-report.sh` reads one
/// session row from opencode's database and ends that record with the usage
/// flags. `--session <id>` names the record — a mid-session verify ends a record
/// it minted instead of hand-rolling the sqlite read. No flag and the record is
/// `$OPENCODE_SESSION_ID`'s, byte-for-byte what the wrapper always did.
///
/// The suite runs the real script against a throwaway sqlite database and a stub
/// `tandem-cli` that records the arguments it was handed, so the assertions are
/// on the call the wrapper makes, not on a copy of its reasoning.
@Suite("The usage report wrapper")
struct UsageReportTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // tests/TandemCoreTests
        .deletingLastPathComponent() // tests
        .deletingLastPathComponent() // repo root

    private struct Run {
        let arguments: [String]
        let body: String
        let errors: String
        let status: Int32
    }

    /// One `session_v2` row: the columns the wrapper's query reads.
    private struct Row {
        let id: String
        var input = "1200"
        var output = "340"
        var reasoning = "5"
        var cacheRead = "900"
        var cacheWrite = "60"
        var cost = "0.0123"
        var model = "deepseek-v4.1-flash"
    }

    // MARK: fixture

    /// A throwaway opencode database carrying `rows` in `session_v2`, shaped like
    /// the real one (the model is a JSON object, so `json_extract` finds its id).
    private static func makeDatabase(_ rows: [Row]) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("usage-report-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let db = dir.appendingPathComponent("opencode.db")

        var sql = """
        CREATE TABLE session_v2 (
            id TEXT PRIMARY KEY,
            tokens_input INTEGER,
            tokens_output INTEGER,
            tokens_reasoning INTEGER,
            tokens_cache_read INTEGER,
            tokens_cache_write INTEGER,
            cost REAL,
            model TEXT
        );
        """
        for row in rows {
            sql += """
            INSERT INTO session_v2 VALUES (
                '\(row.id)', \(row.input), \(row.output), \(row.reasoning),
                \(row.cacheRead), \(row.cacheWrite), \(row.cost),
                '{"id":"\(row.model)","providerID":"opencode-go"}'
            );

            """
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
        process.arguments = [db.path]
        let input = Pipe()
        process.standardInput = input
        try process.run()
        input.fileHandleForWriting.write(Data(sql.utf8))
        try input.fileHandleForWriting.close()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw FixtureError(message: "sqlite3 could not build the fixture database")
        }
        return db
    }

    /// A stub `tandem-cli`: writes each argument on its own line and the summary
    /// it was handed, then answers like `session end`.
    private static func stubCLI() throws -> (path: String, arguments: URL, body: URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("usage-cli-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("tandem-cli")
        let arguments = dir.appendingPathComponent("arguments")
        let body = dir.appendingPathComponent("body")
        try Data("""
        #!/bin/sh
        printf '%s\\n' "$@" > '\(arguments.path)'
        cat > '\(body.path)'
        printf '{"status":"ended"}\\n'
        """.utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        return (path.path, arguments, body)
    }

    private struct FixtureError: Error { let message: String }

    /// Runs the wrapper with the stub CLI, the fixture database and `environment`
    /// laid over the caller's, the summary on stdin.
    private static func run(
        database: URL,
        environment: [String: String?],
        arguments: [String] = ["--outcome", "succeeded"]
    ) throws -> Run {
        let stub = try Self.stubCLI()
        let process = Process()
        process.executableURL = Self.root.appendingPathComponent("Scripts/usage-report.sh")
        process.arguments = arguments
        var env = ProcessInfo.processInfo.environment
        env["OPENCODE_DB"] = database.path
        env["TANDEM_CLI"] = stub.path
        for (key, value) in environment {
            env[key] = value
        }
        process.environment = env

        let input = Pipe()
        process.standardInput = input
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        input.fileHandleForWriting.write(Data("what landed\n".utf8))
        try input.fileHandleForWriting.close()
        process.waitUntilExit()

        _ = output.fileHandleForReading.readDataToEndOfFile()
        let recorded = (try? String(contentsOf: stub.arguments, encoding: .utf8)) ?? ""
        return Run(
            arguments: recorded.split(separator: "\n", omittingEmptySubsequences: false)
                .dropLast()
                .map(String.init),
            body: (try? String(contentsOf: stub.body, encoding: .utf8)) ?? "",
            errors: String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
            status: process.terminationStatus
        )
    }

    /// The `session end` call the wrapper makes for `row`: the usage flags in the
    /// wrapper's order, then the caller's own arguments.
    private static func expectedEnd(for row: Row, plus tail: [String] = ["--outcome", "succeeded"]) -> [String] {
        [
            "session", "end", "--summary", "-",
            "--tokens-input", row.input,
            "--tokens-output", row.output,
            "--tokens-reasoning", row.reasoning,
            "--tokens-cache-read", row.cacheRead,
            "--tokens-cache-write", row.cacheWrite,
            "--cost", row.cost,
            "--model", row.model,
        ] + tail
    }

    // MARK: the tests

    @Test("`--session <id>` ends that record, with its usage read from its row")
    func flagNamesTheRecord() throws {
        let named = Row(id: "ses_named", input: "4200", output: "810", cost: "0.0431", model: "deepseek-v4.1-flash")
        let database = try Self.makeDatabase([Row(id: "ses_env"), named])

        // The environment names a different record, so a passing test proves the
        // flag wins rather than the two agreeing.
        let run = try Self.run(
            database: database,
            environment: ["OPENCODE_SESSION_ID": "ses_env"],
            arguments: ["--session", "ses_named", "--outcome", "succeeded"]
        )

        #expect(run.status == 0, "stderr: \(run.errors)")
        #expect(run.arguments == Self.expectedEnd(for: named),
                "the wrapper ends the named record and never forwards --session")
        #expect(run.body == "what landed\n", "the summary travels on stdin untouched")
    }

    @Test("No flag: the environment names the record, exactly as before")
    func noFlagUsesTheEnvironment() throws {
        let environment = Row(id: "ses_env", input: "77", output: "88", cost: "0.5")
        let database = try Self.makeDatabase([environment, Row(id: "ses_other", input: "1", output: "2")])

        let run = try Self.run(
            database: database,
            environment: ["OPENCODE_SESSION_ID": "ses_env"]
        )

        #expect(run.status == 0, "stderr: \(run.errors)")
        #expect(run.arguments == Self.expectedEnd(for: environment))
        #expect(run.body == "what landed\n")
    }

    @Test("`--session` with no id is refused before the database is touched")
    func flagWithoutAValueFails() throws {
        let database = try Self.makeDatabase([Row(id: "ses_env")])

        let run = try Self.run(
            database: database,
            environment: ["OPENCODE_SESSION_ID": "ses_env"],
            arguments: ["--session"]
        )

        #expect(run.status == 1)
        #expect(run.errors.contains("--session"), "the message names the flag: \(run.errors)")
        #expect(run.arguments.isEmpty, "nothing was ended")
    }
}
