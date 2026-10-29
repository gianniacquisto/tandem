import Foundation
import Testing
@testable import TandemCore

/// Ticket references (ticket 6598E091): a lane key plus a per-lane number, rendered
/// `KEY-N` (`TAN-42`) and accepted wherever an id is. The suite drives the CLI for the
/// lane/number/ref contract — the adapter's contract is the JSON on stdout and the text
/// on stderr — and the store's own API for the schema migration, whose seam only the
/// database can show.
@Suite("Ticket references")
struct TicketRefTests {

    private let cli = CLIHarness()

    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-ticket-ref-\(UUID().uuidString)")
    }

    private func run(home: URL, _ arguments: [String]) throws -> CLIHarness.Result {
        try cli.run(home: home, arguments)
    }

    /// A ticket's whole object from `ticket add`'s output.
    private func add(home: URL, title: String, lane: String? = nil) throws -> [String: Any] {
        var arguments = ["ticket", "add", "--title", title]
        if let lane { arguments += ["--lane", lane] }
        let result = try run(home: home, arguments)
        #expect(result.code == 0, "add failed: \(result.error)")
        return try cli.object(from: result.output)
    }

    // MARK: - The number and the rendered ref

    @Test("a lane's tickets are numbered PRB-1, PRB-2 and the ref is on add and list")
    func numbersAndRefs() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        let first = try add(home: home, title: "first", lane: "Probe")
        let second = try add(home: home, title: "second", lane: "Probe")

        #expect(first["number"] as? Int == 1)
        #expect(first["ref"] as? String == "PRB-1")
        #expect(second["number"] as? Int == 2)
        #expect(second["ref"] as? String == "PRB-2")

        let list = try run(home: home, ["ticket", "list", "--filter", "all"])
        #expect(list.code == 0)
        let rows = try #require(cli.object(from: list.output)["tickets"] as? [[String: Any]])
        #expect(rows.compactMap { $0["ref"] as? String }.sorted() == ["PRB-1", "PRB-2"],
                "the list rows carry the ref too")

        // The default area carries no number and no ref.
        let loose = try add(home: home, title: "loose")
        #expect(loose["number"] == nil || loose["number"] is NSNull)
        #expect(loose["ref"] == nil || loose["ref"] is NSNull)
    }

    @Test("an absent --abbr is the first three of the name, separators stripped")
    func defaultKeys() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Tandem"]).code == 0)
        #expect(try run(home: home, ["lane", "add", "--name", "Retro"]).code == 0)
        #expect(try run(home: home, ["lane", "add", "--name", "code-rank"]).code == 0)

        #expect(try add(home: home, title: "t", lane: "Tandem")["ref"] as? String == "TAN-1")
        #expect(try add(home: home, title: "r", lane: "Retro")["ref"] as? String == "RET-1")
        #expect(try add(home: home, title: "c", lane: "code-rank")["ref"] as? String == "COD-1")
    }

    @Test("a key already in use is refused, case-insensitively")
    func duplicateKeyRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        let clash = try run(home: home, ["lane", "add", "--name", "Pilot", "--abbr", "prb"])
        #expect(clash.code == 1)
        #expect(clash.error.contains("PRB"), "the failure names the key it cannot use")

        let bad = try run(home: home, ["lane", "add", "--name", "Bad", "--abbr", "P-R"])
        #expect(bad.code == 1, "a key is alphanumerics only")
    }

    // MARK: - Resolution

    @Test("show resolves a ref in either case; an unissued number names the lane's range")
    func resolution() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        let first = try add(home: home, title: "first", lane: "Probe")
        let second = try add(home: home, title: "second", lane: "Probe")
        let id = try #require(second["id"] as? String)

        for ref in ["PRB-2", "prb-2"] {
            let shown = try run(home: home, ["ticket", "show", "--id", ref])
            #expect(shown.code == 0, "\(ref) must resolve")
            #expect(try cli.object(from: shown.output)["id"] as? String == id)
        }

        // The full UUID still resolves.
        #expect(try run(home: home, ["ticket", "show", "--id", id]).code == 0)

        let missing = try run(home: home, ["ticket", "show", "--id", "PRB-99"])
        #expect(missing.code == 1)
        #expect(missing.error.contains("PRB-1"))
        #expect(missing.error.contains("PRB-2"), "the failure names the lane's real range")

        let unknownKey = try run(home: home, ["ticket", "show", "--id", "ZZZ-1"])
        #expect(unknownKey.code == 1)
        #expect(unknownKey.error.contains("ZZZ"))

        // The first ticket is what its ref names — numbering follows creation.
        #expect(try cli.object(
            from: try run(home: home, ["ticket", "show", "--id", "PRB-1"]).output
        )["id"] as? String == first["id"] as? String)
    }

    // MARK: - The key's lifecycle

    @Test("rename keeps the key and every ref still resolves")
    func renameKeepsKey() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        _ = try add(home: home, title: "first", lane: "Probe")

        let renamed = try run(home: home, ["lane", "rename", "--from", "Probe", "--to", "Probe Two"])
        #expect(renamed.code == 0, "\(renamed.error)")
        #expect(try cli.object(from: renamed.output)["lanes"] as? [String] == ["Probe Two"])

        let shown = try run(home: home, ["ticket", "show", "--id", "PRB-1"])
        #expect(shown.code == 0, "the key survives the rename")
        #expect(try cli.object(from: shown.output)["lane"] as? String == "Probe Two")
    }

    @Test("--abbr on a lane with a numbered ticket is refused, naming its refs")
    func frozenKey() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        _ = try add(home: home, title: "first", lane: "Probe")

        let refused = try run(home: home, [
            "lane", "rename", "--from", "Probe", "--to", "Probe Two", "--abbr", "PBX",
        ])
        #expect(refused.code == 1, "a frozen key cannot move")
        #expect(refused.error.contains("PRB-1"), "the failure names the refs that would move")
        #expect(try run(home: home, ["ticket", "show", "--id", "PRB-1"]).code == 0,
                "nothing changed")

        // An empty lane's key is not frozen: it may move.
        #expect(try run(home: home, ["lane", "add", "--name", "Empty"]).code == 0)
        let moved = try run(home: home, [
            "lane", "rename", "--from", "Empty", "--to", "Empty Two", "--abbr", "EMP",
        ])
        #expect(moved.code == 0, "\(moved.error)")
    }

    @Test("a move between lanes assigns the target lane's next number and drops the old one")
    func moveAssignsTheTargetNumber() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        #expect(try run(home: home, ["lane", "add", "--name", "Probe", "--abbr", "PRB"]).code == 0)
        #expect(try run(home: home, ["lane", "add", "--name", "Retro", "--abbr", "RET"]).code == 0)
        let ticket = try add(home: home, title: "moving", lane: "Probe")
        let id = try #require(ticket["id"] as? String)
        #expect(ticket["ref"] as? String == "PRB-1")

        let moved = try run(home: home, ["ticket", "move", "--id", id, "--lane", "Retro"])
        #expect(moved.code == 0, "\(moved.error)")
        let movedObject = try cli.object(from: moved.output)
        #expect(movedObject["ref"] as? String == "RET-1")
        #expect(movedObject["number"] as? Int == 1)

        // A move to the default area carries neither number nor ref.
        // No `--lane` at all is the default area (an absent flag is `nil`).
        let loose = try run(home: home, ["ticket", "move", "--id", id])
        #expect(loose.code == 0, "\(loose.error)")
        let looseObject = try cli.object(from: loose.output)
        #expect(looseObject["ref"] == nil || looseObject["ref"] is NSNull)

        // And back in a lane it gets that lane's next number, not the one it had.
        _ = try add(home: home, title: "another", lane: "Retro")
        let back = try run(home: home, ["ticket", "move", "--id", id, "--lane", "Retro"])
        #expect(try cli.object(from: back.output)["ref"] as? String == "RET-3",
                "RET-1 was issued, RET-2 is the other ticket, so the next is RET-3")
    }

    // MARK: - The migration

    private func tempDir() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-ticket-ref-migration-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// A version-2 database (the shipped schema): lanes without keys, tickets without
    /// numbers, the import marker in place — the shape an installed 0.27.0 build left.
    private func version2Store(at dir: URL, rows: [(id: String, lane: String?, createdAt: String)])
        throws {
        let db = try SQLiteDatabase(url: dir.appendingPathComponent("tandem.db"), mode: .readWrite)
        defer { db.close() }
        try StoreSchema.migrate(db, migrations: Array(StoreSchema.migrations.prefix(2)))
        try db.run("INSERT INTO lanes (name, position) VALUES ('Alpha', 0), ('Beta', 1)")
        for (position, row) in rows.enumerated() {
            try db.run("""
                INSERT INTO tickets
                    (id, title, status, archived, lane, origin, position, createdAt)
                VALUES (?, ?, 'backlog', 0, ?, 'human', ?, ?)
                """, [
                    .text(row.id), .text("ticket \(position)"), .textOrNull(row.lane),
                    .int(position), .text(row.createdAt),
                ])
        }
        try StoreSchema.markImported(StoreSchema.ticketsDomain, from: nil, at: Date(), in: db)
    }

    /// The migration fixtures' ids, spelled in full. `add` gives the ticket it makes a
    /// random UUID, so a selector that matched the last two characters let that ticket
    /// stand in for a fixture row about once in 256 runs (TAN-223): match the whole id.
    private enum Fixture {
        static let a1 = UUID(uuidString: "00000000-0000-0000-0000-0000000000A1")!
        static let a2 = UUID(uuidString: "00000000-0000-0000-0000-0000000000A2")!
        static let b1 = UUID(uuidString: "00000000-0000-0000-0000-0000000000B1")!
        static let b2 = UUID(uuidString: "00000000-0000-0000-0000-0000000000B2")!
        static let loose = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        static let decoy = UUID(uuidString: "00000000-0000-0000-0000-00000000DEA2")!
    }

    /// The row carrying exactly this id, never the first row that resembles it.
    private func row(_ tickets: [Ticket], _ id: UUID) -> Ticket? {
        tickets.first { $0.id == id }
    }

    @Test("the migration numbers each lane by createdAt, leaves the default area null")
    func migrationBackfills() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        // Alpha: two tickets and a tie broken by id. Beta: one. Plus a loose one, and a
        // row whose id ends in `A2` but is not the fixture: it stands in for the random
        // UUID `add` gives the new ticket, which landed on `A2` once in 256 runs while
        // the assertions selected rows by their last two characters (TAN-223).
        try version2Store(at: dir, rows: [
            (Fixture.b2.uuidString, "Alpha", "2026-02-01T00:00:00Z"),
            (Fixture.a2.uuidString, "Alpha", "2026-01-01T00:00:00Z"),
            (Fixture.decoy.uuidString, nil, "2026-01-20T00:00:00Z"),
            (Fixture.loose.uuidString, nil, "2026-01-15T00:00:00Z"),
            (Fixture.a1.uuidString, "Alpha", "2026-01-01T00:00:00Z"),
            (Fixture.b1.uuidString, "Beta", "2026-03-01T00:00:00Z"),
        ])

        // A read before any write must not fail: the store is still v2, and the read
        // shows the refs the migration will make true.
        let store = Store(directory: dir)
        let before = try store.list(.all).value
        #expect(store.schemaVersion() == 2, "a read never migrates")
        #expect(row(before, Fixture.a1)?.ref == "ALP-1",
                "the createdAt tie is broken by UUID: A1 sorts first")
        #expect(row(before, Fixture.a2)?.ref == "ALP-2")
        #expect(row(before, Fixture.b2)?.ref == "ALP-3")
        #expect(row(before, Fixture.b1)?.ref == "BET-1")
        #expect(row(before, Fixture.loose)?.number == nil,
                "the default area carries no number")
        #expect(row(before, Fixture.loose)?.ref == nil)
        #expect(row(before, Fixture.decoy)?.ref == nil, "the decoy is not the ALP-2 fixture")

        // The first write migrates; the numbers it writes are the ones the read showed.
        _ = try store.add("new")
        #expect(store.schemaVersion() == StoreSchema.currentVersion)
        let after = try store.list(.all).value
        #expect(row(after, Fixture.a1)?.ref == "ALP-1")
        #expect(row(after, Fixture.a2)?.ref == "ALP-2")
        #expect(row(after, Fixture.b2)?.ref == "ALP-3")
        #expect(row(after, Fixture.b1)?.ref == "BET-1")
        #expect(row(after, Fixture.decoy)?.ref == nil, "the decoy is not the ALP-2 fixture")
    }

    @Test("a second open does not renumber — the migration is idempotent")
    func migrationIsIdempotent() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try version2Store(at: dir, rows: [
            ("00000000-0000-0000-0000-0000000000A2", "Alpha", "2026-02-01T00:00:00Z"),
            ("00000000-0000-0000-0000-0000000000A1", "Alpha", "2026-01-01T00:00:00Z"),
        ])

        let first = try Store(directory: dir).list(.all).value
        _ = try Store(directory: dir).add("new")
        let second = try Store(directory: dir).list(.all).value
        _ = try Store(directory: dir).add("another")
        let third = try Store(directory: dir).list(.all).value

        for (id, ref) in first.compactMap({ t in t.ref.map { (t.id, $0) } }) {
            #expect(second.first { $0.id == id }?.ref == ref)
            #expect(third.first { $0.id == id }?.ref == ref)
        }

        // The migration itself, run again, changes nothing.
        let db = try SQLiteDatabase(url: dir.appendingPathComponent("tandem.db"), mode: .readWrite)
        defer { db.close() }
        let numbered = try db.rows("SELECT id, number FROM tickets ORDER BY id") {
            ($0.text(0), $0.intOrNil(1))
        }
        try StoreSchema.migrate(db)
        let again = try db.rows("SELECT id, number FROM tickets ORDER BY id") {
            ($0.text(0), $0.intOrNil(1))
        }
        #expect(again.map { $0.0 } == numbered.map { $0.0 })
        #expect(again.map { $0.1 } == numbered.map { $0.1 })
    }

    // MARK: - The palette

    @Test("a typed ref matches its ticket in the palette")
    func paletteMatchesRef() {
        var ticket = Ticket(title: "Something else")
        ticket.ref = "TAN-42"
        let ranked = QuickOpen.rank([.ticket(ticket)], query: "tan-42")
        #expect(ranked.count == 1)
    }
}
