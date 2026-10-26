import Foundation
import Testing

/// The project-wiki scaffold (9D846650): the conventions state the fixed
/// scaffold (Overview, Decisions, Risks, Processes), the input rule (the
/// agents' `.raw/` captures + the human's scratch pad + the ended session
/// records' summaries), and the scratch read-only rule. The repo's rules
/// live in the skill files, not in the app — this suite pins the
/// load-bearing phrases so a refactor cannot drop the contract silently.
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

    @Test("CONVENTIONS states the input rule and the scratch read-only rule")
    func conventionsStateTheInputs() throws {
        let c = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(c.contains("three inputs"))
        #expect(c.contains("the ended session records' summaries"),
                "the session summaries are a consolidation input (ADR 0027)")
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

    @Test("The scaffold's own files agree — risk routing and the Overview wording (12D351E9)")
    func scaffoldProseAgrees() throws {
        let consolidation = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(consolidation.contains("| an open **risk** | the **Risks** page"),
                "the routing can only route what the Risks page holds")
        #expect(!consolidation.contains("open or retired **risk**"),
                "the Risks page holds one line per open risk")
        let conventions = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(conventions.contains("one line per open risk"))
        for path in ["AGENTS.md",
                     "skills/tandem-memory/SKILL.md",
                     "skills/tandem-sprint/SKILL.md",
                     "skills/tandem-tidy/SKILL.md"] {
            let orient = try Self.file(path)
            #expect(!orient.contains("the project's curated note"),
                    "\(path): the orient step names the Overview, the scaffold's anchor")
            #expect(orient.contains("**Overview**"), "\(path): the orient step names the Overview")
        }
    }

    @Test("CONSOLIDATION reads the inputs, creates the scaffold on demand, leaves the scratch untouched")
    func consolidationStatesThePass() throws {
        let c = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(c.contains("scratch"))
        #expect(c.contains("read-only to the consolidator"))
        #expect(c.contains("creates all four"))
        #expect(c.contains("the human prunes"))
    }

    @Test("The question's exit is one write — the handoff rule and the sprint's inbox debt (D2522EC3)")
    func questionExitIsStatedOnce() throws {
        let agents = try Self.file("AGENTS.md")
        #expect(agents.contains("`human` is an assignee only while a question is open"),
                "the handoff states when `human` is an assignee")
        #expect(agents.contains("`ticket answer` is the one write that records the answer"),
                "the answer write is the question's exit")

        let sprint = try Self.file("skills/tandem-sprint/SKILL.md")
        #expect(sprint.contains("tandem-cli ticket answer"),
                "sprint §5 records an answer through the verb, not a restated write")
        #expect(sprint.contains("that is the bookkeeping debt"),
                "the agents' read is named as the debt an answered ticket leaves")

        let interview = try Self.file("skills/tandem-interview/SKILL.md")
        #expect(interview.contains("tandem-cli ticket answer"),
                "interview §6 closes the hold through the same verb")
    }
}
