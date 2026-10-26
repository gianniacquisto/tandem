import Foundation
import Testing

/// The SQLite store's documents (ADR 0031, ticket FD0DE150): the record the code has
/// been citing since P1, and the live prose that has to match an installed build.
/// What it pins is narrow on purpose — the sentences a switch would silently break:
/// that the marker, not table emptiness, decides an import and how a fresh home records
/// it, that the recovery recipe cannot be blocked by a journal file a clean close left
/// absent, and that an older build's write beside a migrated home is ignored. It does
/// not check the ADR against the store's code; the receipts for that are the suites
/// beside this one and the review of PR #189. A sentence here that passes against any
/// text is a bug in the test, not coverage.
@Suite("Store documents")
struct StoreDocumentsTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    private static func adr0031() throws -> String {
        let adr = try FileManager.default
            .contentsOfDirectory(atPath: Self.root.appendingPathComponent("docs/adr").path)
            .first { $0.hasPrefix("0031-") }
        let name = try #require(adr, "ADR 0031 is cited by twelve files and must exist")
        return try Self.file("docs/adr/\(name)")
    }

    /// The recovery recipe, as all three documents that carry it must write it.
    private static let removeStore = "rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db"
    private static let restoreBackup = "mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db"

    /// ADR 0031 states the import decision as it shipped: the marker decides, the marker
    /// is written with the rows, and a fresh home records one with no source file.
    @Test("ADR 0031 states the marker's decision and its fresh-home shape")
    func adr0031StatesTheMarker() throws {
        let adr = try Self.adr0031()
        #expect(
            adr.contains("Table emptiness cannot be the signal"),
            "the ADR must reject table emptiness by name, not only name its replacement")
        #expect(
            adr.contains("or for which there was nothing to import"),
            "a marker row is written for an imported file *and* for a fresh home")
        #expect(
            adr.contains("`source` is NULL"),
            "a fresh home's marker has no file name, so the definition must say so")
        #expect(
            adr.contains("in the same transaction as the imported rows"),
            "the marker is durable state written with the rows")
    }

    /// The old copy-back recipe chained the removals to the restore with `&&`. After a
    /// clean close there are no `-wal`/`-shm` files, `rm` exits non-zero, and the restore
    /// never runs — so the documents must remove what exists (`-f`) and put the journal
    /// files before the database.
    @Test("the recovery recipe cannot be blocked by an absent journal file")
    func recoveryRecipeCannotBeBlocked() throws {
        for path in [
            "README.md",
            "skills/tandem/SKILL.md",
            "docs/adr/0031-one-database-the-store.md",
        ] {
            let text = try Self.file(path)
            let removed = try #require(
                text.range(of: Self.removeStore),
                "\(path) must remove the journal files and the database with `rm -f`")
            let restored = try #require(
                text.range(of: Self.restoreBackup),
                "\(path) must restore the backup over the database")
            #expect(
                removed.lowerBound < restored.lowerBound,
                "\(path) must remove the journal files before the restore, or the restored file is read under a dead WAL")
        }
    }

    /// An older installed build is the hand-restored file's twin: the marker ignores its
    /// writes too. The README states it beside the warning it belongs to.
    @Test("the README says an older build's write beside a migrated home is ignored")
    func readmeStatesTheDowngrade() throws {
        let readme = try Self.file("README.md")
        #expect(
            readme.contains("a pre-v0.27.0 build writing `tickets.json` beside the database is ignored too"),
            "the marker silences an older build's writes; the README must say so beside the fork warning")
    }

    /// The amendments have one home: the text is in ADR 0031, and every amended record
    /// carries a dated pointer to it in its own file — so 0018's title cannot read as
    /// current protocol.
    @Test("every amended ADR points at ADR 0031 from its own file")
    func amendedADRsPointAt0031() throws {
        for path in [
            "docs/adr/0001-store-protocol.md",
            "docs/adr/0006-stored-order-and-sections.md",
            "docs/adr/0014-cli-doctor.md",
            "docs/adr/0017-the-ticket-model.md",
            "docs/adr/0018-rolling-store-backup.md",
            "docs/adr/0027-session-records.md",
        ] {
            let text = try Self.file(path)
            #expect(
                text.contains("[ADR 0031](0031-one-database-the-store.md)"),
                "\(path) is amended by ADR 0031 and must say where the amendment's text is")
            #expect(
                text.contains("2026-10-26"),
                "\(path)'s pointer must be dated, like every other amendment in docs/adr")
        }
    }

    /// The README's store section shows the database, not a hand-editable JSON file.
    @Test("the README's store section names the database")
    func readmeNamesTheDatabase() throws {
        let readme = try Self.file("README.md")
        #expect(readme.contains("tandem.db"), "the README must name the database")
        #expect(readme.contains("tandem.db.bak"), "the rolling backup is a whole-database copy")
        #expect(!readme.contains("todos.json"), "the retired store file must leave the README")
        #expect(!readme.contains("edit by hand"), "the store is no longer offered as hand-editable")
    }

    /// The glossary's Store entry defines the database-backed store, and its Import
    /// marker entry allows a fresh home's marker (no source file).
    @Test("the glossary's Store entry defines the database")
    func glossaryDefinesTheDatabase() throws {
        let glossary = try Self.file("GLOSSARY.md")
        let start = try #require(glossary.range(of: "**Store**:"))
        let tail = glossary[start.lowerBound...]
        let entry = tail.range(of: "\n### ").map { tail[..<$0.lowerBound] } ?? tail
        #expect(entry.contains("tandem.db"), "the Store entry must name the database")
        #expect(entry.contains("tandem.db.bak"), "the Store entry must name the rolling backup")
        #expect(!entry.contains("todos.json"), "the Store entry must not name the retired file")

        let markerStart = try #require(glossary.range(of: "**Import marker**:"))
        let markerTail = glossary[markerStart.lowerBound...]
        let marker = markerTail.range(of: "\n### ").map { markerTail[..<$0.lowerBound] } ?? markerTail
        #expect(
            marker.contains("`source` is NULL"),
            "an import marker with no source is a fresh home, and the entry must say so")
    }

    /// AGENTS.md and the skills name the database; the retired session file is gone.
    @Test("the agent-facing prose names no retired store file")
    func agentProseNamesTheDatabase() throws {
        let agents = try Self.file("AGENTS.md")
        #expect(!agents.contains("sessions.json"), "sessions live in the database (ADR 0031)")
        let compaction = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(!compaction.contains("sessions.json"), "the session records are rows now")
        let skill = try Self.file("skills/tandem/SKILL.md")
        #expect(skill.contains("tandem.db"), "the tandem skill opens on the store and must name it")
        #expect(!skill.contains("todos.json"), "the retired store file must leave the skill")
        #expect(!skill.contains("comments/<id>.jsonl"), "a comment is a row, not a sidecar file")
    }
}
