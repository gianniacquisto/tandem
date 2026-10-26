import Foundation
import Testing

/// The one-shot sprint migration (ADR 0032, ticket 02CA3EC2): five archived
/// `Sprint <n> — <lane>` tickets become five records. The script is not a shipped
/// verb — it runs once, reports its mapping, and is repeatable enough that a second
/// run writes nothing.
///
/// The suite drives `Scripts/migrate-sprints.py` against a throwaway home: the five
/// tickets are a legacy `tickets.json` (the store's own read-fallback), the CLI is
/// the binary beside the test bundle, and every record is read back through
/// `tandem-cli sprint list` — the record's real reader.
@Suite("Sprint migration")
struct SprintMigrationTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static let script = "Scripts/migrate-sprints.py"

    private struct Run {
        var code: Int32
        var output: String
        var error: String
    }

    private struct Fixture {
        let home: URL
        let cli: CLIHarness
    }

    /// The five tickets, shaped like the real archive: three of them titled
    /// `Sprint 1 — Tandem` (the titles collide, the `createdAt` does not), one lane
    /// with two `## Close` sections, one with no close at all, one lane in another
    /// project.
    private static let ticketA = "11111111-1111-1111-1111-111111111111"
    private static let ticketB = "22222222-2222-2222-2222-222222222222"
    private static let ticketC = "33333333-3333-3333-3333-333333333333"
    private static let ticketD = "44444444-4444-4444-4444-444444444444"
    private static let ticketE = "55555555-5555-5555-5555-555555555555"

    private static let batchA1 = "AAAA1111-0000-4000-8000-000000000001"
    private static let batchA2 = "AAAA1111-0000-4000-8000-000000000002"
    private static let batchC1 = "CCCC3333-0000-4000-8000-000000000001"
    private static let batchC2 = "CCCC3333-0000-4000-8000-000000000002"
    private static let batchD1 = "DDDD4444-0000-4000-8000-000000000001"
    private static let batchE1 = "EEEE5555-0000-4000-8000-000000000001"

    private static func ticket(
        _ id: String,
        title: String,
        lane: String,
        created: String,
        completed: String,
        description: String
    ) -> [String: Any] {
        [
            "id": id,
            "title": title,
            "status": "done",
            "archived": true,
            "lane": lane,
            "origin": "sprint",
            "description": description,
            "createdAt": created,
            "completedAt": completed,
        ]
    }

    private static func fixture() throws -> Fixture {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("sprint-migration-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)

        let a = """
        ## The batch

        - `\(batchA1)` — one
        - `\(batchA2)` — two

        Session: ses_aaaaaaaaaaaaaaaaaaaaaaaa

        ## Run — 2026-10-22

        - `\(batchA1)` — landed, PR #1.

        ## Close

        One landed, one slipped.
        """
        let b = """
        ## The batch

        Session: ses_bbbbbbbbbbbbbbbbbbbbbbbb
        """
        let c = """
        ## The batch

        - `\(batchC1)` — three
        - `\(batchC2)` — four

        ## Run — 2026-10-23

        - `\(batchC1)` — landed.

        ## Run — 2026-10-23 (second)

        - `\(batchC2)` — escalated.

        ## Close — 2026-10-23

        First close.

        ## Close — addendum (same day)

        Second close.
        """
        let d = """
        ## The batch

        - `\(batchD1)` — five

        _(batch opened by session ses_dddddddddddddddddddddd, the sprint pass, ticket 77777777-7777-4777-8777-777777777777)_
        """
        let e = """
        ## The batch

        - `\(batchE1)` — six

        Session: ses_eeeeeeeeeeeeeeeeeeeeeeee

        ## Run — 2026-10-22

        - `\(batchE1)` — nothing merged.

        ## Close

        Nothing merged.
        """

        let tickets: [String: Any] = [
            "version": 1,
            "lanes": ["Tandem", "code-rank"],
            "tickets": [
                ticket(ticketA, title: "Sprint 1 — Tandem", lane: "Tandem",
                       created: "2026-10-22T17:00:00Z", completed: "2026-10-22T18:00:00Z",
                       description: a),
                ticket(ticketB, title: "Sprint 1 — Tandem", lane: "Tandem",
                       created: "2026-10-22T21:00:00Z", completed: "2026-10-22T22:00:00Z",
                       description: b),
                ticket(ticketC, title: "Sprint 2 — Tandem", lane: "Tandem",
                       created: "2026-10-22T23:00:00Z", completed: "2026-10-22T23:30:00Z",
                       description: c),
                ticket(ticketD, title: "Sprint 1 — Tandem", lane: "Tandem",
                       created: "2026-10-23T07:00:00Z", completed: "2026-10-23T08:00:00Z",
                       description: d),
                ticket(ticketE, title: "Sprint 1 — code-rank", lane: "code-rank",
                       created: "2026-10-22T12:00:00Z", completed: "2026-10-22T13:00:00Z",
                       description: e),
            ],
        ]
        let data = try JSONSerialization.data(withJSONObject: tickets, options: [.prettyPrinted])
        try data.write(to: home.appendingPathComponent("tickets.json"))
        return Fixture(home: home, cli: CLIHarness())
    }

    /// Run the migration script against a home, with the product CLI on `TANDEM_CLI`.
    private static func migrate(_ harness: CLIHarness, home: URL) throws -> Run {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = [
            Self.root.appendingPathComponent(Self.script).path,
            Self.ticketA, Self.ticketB, Self.ticketC, Self.ticketD, Self.ticketE,
        ]
        var environment = ProcessInfo.processInfo.environment
        environment["TANDEM_HOME"] = home.path
        environment["TANDEM_CLI"] = harness.url.path
        process.environment = environment
        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err
        try process.run()
        process.waitUntilExit()
        return Run(
            code: process.terminationStatus,
            output: String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self),
            error: String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self))
    }

    /// The records the CLI reads back, keyed by "lane#number".
    private static func records(_ harness: CLIHarness, home: URL) throws -> [String: [String: Any]] {
        let run = try harness.run(home: home, ["sprint", "list"])
        #expect(run.code == 0, "sprint list failed: \(run.error)")
        let object = try harness.object(from: run.output)
        let sprints = object["sprints"] as? [[String: Any]] ?? []
        var byHandle: [String: [String: Any]] = [:]
        for sprint in sprints {
            let lane = sprint["lane"] as? String ?? ""
            let number = sprint["number"] as? Int ?? 0
            byHandle["\(lane)#\(number)"] = sprint
        }
        return byHandle
    }

    private static func runs(_ record: [String: Any]?) throws -> [String] {
        try #require(record, "the record must exist")["runs"] as? [String] ?? []
    }

    private static func batch(_ record: [String: Any]?) throws -> [String] {
        try #require(record, "the record must exist")["batch"] as? [String] ?? []
    }

    /// The five tickets become five records, numbered per lane by `createdAt` — the
    /// titles lie (three read `Sprint 1 — Tandem`), the timestamps do not — and each
    /// record's first run entry carries its provenance.
    @Test("the five tickets migrate to five renumbered records")
    func migratesFiveRecords() throws {
        let fixture = try Self.fixture()
        defer { try? FileManager.default.removeItem(at: fixture.home) }

        let run = try Self.migrate(fixture.cli, home: fixture.home)
        #expect(run.code == 0, "the migration failed: \(run.error)")
        #expect(run.output.contains("wrote 5, skipped 0"), "the report names both counts:\n\(run.output)")

        let records = try Self.records(fixture.cli, home: fixture.home)
        #expect(records.count == 5, "five records, one per ticket")

        // Tandem by createdAt: A (17:00), B (21:00), C (23:00), D (next day).
        for (number, ticket) in [(1, Self.ticketA), (2, Self.ticketB),
                                 (3, Self.ticketC), (4, Self.ticketD)] {
            let record = try #require(records["Tandem#\(number)"],
                                      "Tandem's newest title says Sprint 2; the records renumber by createdAt")
            let provenance = try #require(try Self.runs(record).first)
            #expect(
                provenance.hasSuffix("migrated from ticket \(ticket) — 'Sprint 1 — Tandem'")
                    || provenance.hasSuffix("migrated from ticket \(ticket) — 'Sprint 2 — Tandem'"),
                "the first run entry carries the ticket it came from, title and all: \(provenance)")
            #expect(record["status"] as? String == "closed")
            #expect(record["mode"] as? String == "serial")
        }
        #expect(records["code-rank#1"] != nil, "numbering is per lane")

        let a = records["Tandem#1"]
        #expect(try Self.batch(a) == [Self.batchA1, Self.batchA2], "the batch keeps stored order")
        #expect(a?["openedAt"] as? String == "2026-10-22T17:00:00Z", "openedAt is the ticket's createdAt")
        #expect(a?["closedAt"] as? String == "2026-10-22T18:00:00Z", "closedAt is the ticket's completedAt")
        #expect(a?["session"] as? String == "ses_aaaaaaaaaaaaaaaaaaaaaaaa")
        #expect(try Self.runs(a).count == 2, "the provenance plus the one `## Run` section")
        #expect(try Self.runs(a).last?.contains("landed, PR #1") == true)

        let c = records["Tandem#3"]
        #expect(try Self.runs(c).count == 3, "the provenance plus both `## Run` sections")
        let close = try #require(c?["close"] as? String)
        #expect(close.contains("First close") && close.contains("Second close"),
                "both `## Close` sections are the close text")

        let d = records["Tandem#4"]
        #expect(try Self.batch(d) == [Self.batchD1], "a UUID in the prose is not a batch ticket")
        #expect(
            d?["close"] is NSNull || d?["close"] == nil,
            "a ticket with no `## Close` closes with no text")
        #expect(d?["session"] as? String == "ses_dddddddddddddddddddddd",
                "the opening session is read where the ticket names it, prose included")

        let e = records["code-rank#1"]
        #expect(try Self.batch(e) == [Self.batchE1])
        #expect(try Self.runs(e).count == 2, "the provenance plus one `## Run` section")
        #expect(e?["close"] as? String == "Nothing merged.")
    }

    /// A second run writes nothing: the provenance marker is the record's own receipt.
    @Test("a second run skips every ticket")
    func secondRunSkips() throws {
        let fixture = try Self.fixture()
        defer { try? FileManager.default.removeItem(at: fixture.home) }

        let first = try Self.migrate(fixture.cli, home: fixture.home)
        #expect(first.code == 0, "the first run failed: \(first.error)")

        let second = try Self.migrate(fixture.cli, home: fixture.home)
        #expect(second.code == 0, "the second run failed: \(second.error)")
        #expect(second.output.contains("wrote 0, skipped 5"), "the second run is a no-op:\n\(second.output)")
        #expect(try Self.records(fixture.cli, home: fixture.home).count == 5)
    }

    /// The script is a script, not a verb: no `sprint` subcommand grows for it.
    @Test("the migration is not a shipped verb")
    func migrationIsNotAVerb() throws {
        let usage = try String(
            contentsOf: Self.root.appendingPathComponent("Sources/tandem-cli/Usage.swift"),
            encoding: .utf8)
        #expect(!usage.contains("migrate"), "the migration runs once and is not a surface to maintain")
    }
}
