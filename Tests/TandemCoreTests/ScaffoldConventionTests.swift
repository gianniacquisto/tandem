import Foundation
import Testing

/// The project-wiki scaffold (9D846650): the conventions state the fixed
/// scaffold (Overview, Decisions, Risks, Processes), the two-input rule
/// (the agents' `.raw/` captures + the human's scratch pad), and the scratch
/// read-only rule. The repo's rules live in the skill files, not in the app —
/// this suite pins the load-bearing phrases so a refactor cannot drop the
/// contract silently.
@Suite("Scaffold conventions")
struct ScaffoldConventionTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    @Test("CONVENTIONS states the fixed scaffold — the four pages, on demand, no persona partition")
    func conventionsStateTheScaffold() throws {
        let c = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(c.contains("fixed scaffold"), "the scaffold must be named as fixed")
        for page in ["**Overview**", "**Decisions**", "**Risks**", "**Processes**"] {
            #expect(c.contains(page), "missing scaffold page \(page)")
        }
        #expect(c.contains("created on demand"))
        #expect(c.contains("no per-persona folders"))
    }

    @Test("CONVENTIONS states the two-input rule and the scratch read-only rule")
    func conventionsStateTheInputs() throws {
        let c = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(c.contains("two inputs"))
        #expect(c.contains("read-only to the consolidator"))
        #expect(c.contains("never edits or archives the scratch"))
        #expect(c.contains("the human prunes"))
    }

    @Test("CONVENTIONS gives the Decisions page its index duty — ADRs stay in the repo")
    func conventionsStateTheDecisionsDuty() throws {
        let c = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(c.contains("every ADR"))
        #expect(c.contains("stays in the repo"))
    }

    @Test("The capture cadence retires the floor work note — the session summary is the work record (0AC26CC5)")
    func captureCadenceRetiresTheWorkNote() throws {
        let write = try Self.file("skills/tandem-memory/WRITE-PROTOCOL.md")
        #expect(!write.contains("Floor: one `work` note"), "the floor work-note rule must go")
        #expect(!write.contains("significant unit of work finished"), "`work` must leave the cadence's kind list")
        #expect(write.contains("session end --summary"), "the session summary is the work record")
        let consolidation = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(!consolidation.contains("| `work` |"), "the routing table must not route `work`")
        #expect(consolidation.contains("--status ended"), "the pass reads ended session records")
        #expect(consolidation.contains("session show"), "the pass reads the record's summary")
        for path in ["AGENTS.md", "skills/tandem-memory/SKILL.md", "skills/tandem/SKILL.md"] {
            let c = try Self.file(path)
            #expect(!c.contains("|work|"), "\(path) still lists the `work` kind")
            #expect(!c.contains("one `work` note"), "\(path) still names the floor work note")
        }
    }

    @Test("CONSOLIDATION reads both inputs, creates the scaffold on demand, leaves the scratch untouched")
    func consolidationStatesThePass() throws {
        let c = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(c.contains("scratch"))
        #expect(c.contains("read-only to the consolidator"))
        #expect(c.contains("creates all four"))
        #expect(c.contains("the human prunes"))
    }
}
