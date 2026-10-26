import Foundation
import Testing

/// The SQLite store's document set (ADR 0031, ticket FD0DE150): the record the code
/// has been citing since P1, and the live prose that has to match an installed build.
/// The ADR is history and keeps the retired names; the live docs name the database,
/// never the JSON store it replaced. Pinned so the switch cannot silently revert.
@Suite("Store documents")
struct StoreDocumentsTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    /// ADR 0031 exists, and it states the decision as it shipped rather than a sketch.
    @Test("ADR 0031 exists and names the shipped store protocol")
    func adr0031ExistsAndStatesTheProtocol() throws {
        let adr = try FileManager.default
            .contentsOfDirectory(atPath: Self.root.appendingPathComponent("docs/adr").path)
            .first { $0.hasPrefix("0031-") }
        let name = try #require(adr, "ADR 0031 is cited by twelve files and must exist")
        let text = try Self.file("docs/adr/\(name)")
        for phrase in [
            "tandem.db",                      // one file
            "WAL",
            "schema_migrations",              // the version seam
            "store_imports",                  // the import marker
            "tandem.db.bak",                  // the rolling generation
            "tandem.lock",                    // the retired sidecar lock
            "read-fallback",
            "one generation",
        ] {
            #expect(text.contains(phrase), "ADR 0031 must state \(phrase)")
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

    /// The glossary's Store entry defines the database-backed store.
    @Test("the glossary's Store entry defines the database")
    func glossaryDefinesTheDatabase() throws {
        let glossary = try Self.file("GLOSSARY.md")
        let start = try #require(glossary.range(of: "**Store**:"))
        let tail = glossary[start.lowerBound...]
        let entry = tail.range(of: "\n### ").map { tail[..<$0.lowerBound] } ?? tail
        #expect(entry.contains("tandem.db"), "the Store entry must name the database")
        #expect(entry.contains("tandem.db.bak"), "the Store entry must name the rolling backup")
        #expect(!entry.contains("todos.json"), "the Store entry must not name the retired file")
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
