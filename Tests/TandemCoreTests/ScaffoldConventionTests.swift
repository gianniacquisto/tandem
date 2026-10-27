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

    @Test("The per-event capture cadence is retired — the session record is the record (7CA67B4F)")
    func perEventCaptureCadenceIsRetired() throws {
        let retired = ["Record captures as you work", "at each moment", "at each qualifying moment",
                       "never batch to session end", "don't batch to session end",
                       "files its captures", "file note captures as you work"]
        for path in ["AGENTS.md",
                     "skills/tandem-memory/SKILL.md",
                     "skills/tandem-memory/WRITE-PROTOCOL.md",
                     "skills/tandem-build/SKILL.md",
                     "agents/builder.md"] {
            let c = try Self.file(path)
            for phrase in retired {
                #expect(!c.contains(phrase), "\(path) still carries the retired cadence: \(phrase)")
            }
        }
        for path in ["AGENTS.md", "skills/tandem-memory/SKILL.md", "skills/tandem-memory/WRITE-PROTOCOL.md"] {
            let c = try Self.file(path)
            #expect(c.contains("cost of re-derivation"),
                    "\(path) states the one capture rule: a real cost of re-derivation")
            #expect(c.contains("session record") || c.contains("session end"),
                    "\(path) points the record at the session record")
        }
        let write = try Self.file("skills/tandem-memory/WRITE-PROTOCOL.md")
        #expect(write.contains("session end --summary"), "the session summary is the work record")
        let build = try Self.file("skills/tandem-build/SKILL.md")
        #expect(build.contains("session record"), "the build pass's record step is the session record")
        let retro = try Self.file("skills/retro/SKILL.md")
        #expect(retro.contains("session record"), "the retro records through the session record, not a capture")
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

    @Test("The wiki is documentation — both readers, the page roles, the journal out (8C8DBD13)")
    func wikiIsDocumentation() throws {
        let adr = try Self.file("docs/adr/0033-wiki-is-documentation.md")
        #expect(adr.contains("## Decision"))
        #expect(adr.contains("journal"), "the ADR refuses the journal by name")
        #expect(adr.contains("human"), "the ADR names the human reader")
        #expect(adr.contains("agent"), "the ADR names the agent reader")

        let conventions = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(conventions.contains("## One wiki, two readers"),
                "the page roles get a section of their own")
        #expect(conventions.contains("ADR 0033"), "the section cites the decision")
        #expect(conventions.contains("journal is out"), "the page set refuses the journal")
        #expect(conventions.contains("what this is, how to run it, where things live, and what is open"),
                "the Overview's job is stated where the pages are described")
    }

    @Test("The freshness rule — a page dates itself, and a pass that changes a fact amends it (8C8DBD13)")
    func freshnessRule() throws {
        let conventions = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(conventions.contains("Reviewed <date>"), "the review stamp is the page's own date")
        #expect(conventions.contains("mtime"), "the Notes tab's mtime is the reader's signal")
        #expect(conventions.contains("amends the page in the same write"),
                "a fact change is not deferred to a later pass")

        let agents = try Self.file("AGENTS.md")
        #expect(agents.contains("CONVENTIONS.md"), "AGENTS.md points at the conventions")
        #expect(!agents.contains("Reviewed <date>"), "AGENTS.md does not restate the freshness rule")
    }

    @Test("The README leads a human to a project's Overview (8C8DBD13)")
    func readmeLeadsToTheOverview() throws {
        let readme = try Self.file("README.md")
        #expect(readme.contains("~/.tandem/bikeshed/projects/<project>/<project>.md"),
                "the README names the Overview's path")
        #expect(readme.contains("`## Threads`"),
                "the Overview's Threads section is where what is open lives")
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
