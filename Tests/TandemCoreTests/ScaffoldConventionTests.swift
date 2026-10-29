import Foundation
import Testing

/// The project-wiki scaffold (9D846650): the conventions state the fixed
/// scaffold (Overview, Decisions, Risks, Processes), the input rule (the
/// agents' `.raw/` captures + the human's daily notes + the ended session
/// records' summaries), and the daily notes' read-only rule. The repo's rules
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

    /// The retired orient wordings: the scaffold's anchor is the Overview, never
    /// the Hub or a "curated note". Both apostrophes fold to the ASCII one, so a
    /// file written with the typographic form cannot pass the pin silently
    /// (TAN-194).
    private static let staleOrientPhrases = ["the project's curated note", "orient from the Hub"]

    private static func namesAStaleOrientPhrase(_ text: String) -> Bool {
        let folded = text.replacingOccurrences(of: "\u{2018}", with: "'")
            .replacingOccurrences(of: "\u{2019}", with: "'")
        return staleOrientPhrases.contains { folded.contains($0) }
    }

    /// The review lens set (TAN-213): one table in `skills/review-lenses/SKILL.md`
    /// is the single source of truth, and a lens is a row plus a file. The
    /// parsers live here so the pins read the table the gate reads, never a copy
    /// of it.
    private static let lensDirectory = "skills/review-lenses"

    private static func section(_ text: String, _ heading: String) -> String {
        guard let start = text.range(of: heading) else { return "" }
        let rest = text[start.upperBound...]
        if let end = rest.range(of: "\n## ") { return String(rest[..<end.lowerBound]) }
        return String(rest)
    }

    /// The rows of `## The lenses`, in order: the lens's name and the file it
    /// reads. The header row and the separator are dropped.
    private static func lensRows() throws -> [(name: String, file: String)] {
        let skill = try Self.file("\(lensDirectory)/SKILL.md")
        var rows: [(String, String)] = []
        for line in Self.section(skill, "## The lenses").split(separator: "\n") where line.hasPrefix("|") {
            let cells = line.split(separator: "|", omittingEmptySubsequences: false)
                .map { $0.trimmingCharacters(in: .whitespaces) }
            guard cells.count >= 3, !cells[1].isEmpty, !cells[1].hasPrefix("-"),
                  cells[1] != "lens" else { continue }
            rows.append((cells[1].trimmingCharacters(in: CharacterSet(charactersIn: "`")),
                         cells[2].trimmingCharacters(in: CharacterSet(charactersIn: "`"))))
        }
        return rows
    }

    @Test("The lens table and the directory agree (TAN-213)")
    func lensTableAndDirectoryAgree() throws {
        let rows = try Self.lensRows()
        #expect(rows.count == 5, "the set is five lenses, the table has \(rows.count)")
        #expect(Set(rows.map(\.name)) == ["code", "security", "ui-design", "ux", "architecture"],
                "the set is code, security, ui-design, ux, architecture")
        for row in rows {
            let path = Self.root.appendingPathComponent(row.file).path
            #expect(FileManager.default.fileExists(atPath: path),
                    "lens `\(row.name)` names \(row.file), which does not exist")
        }
        let directory = Self.root.appendingPathComponent(Self.lensDirectory)
        let files = try FileManager.default.contentsOfDirectory(atPath: directory.path)
            .filter { $0.hasSuffix(".md") && $0 != "SKILL.md" }
        #expect(!files.isEmpty, "the lens directory holds no lens file")
        let rowFiles = Set(rows.map { ($0.file as NSString).lastPathComponent })
        for file in files {
            #expect(rowFiles.contains(file),
                    "\(Self.lensDirectory)/\(file) has no row in the lens table")
        }
    }

    @Test("Every lens states what it blocks on (TAN-213)")
    func everyLensStatesWhatItBlocksOn() throws {
        let rows = try Self.lensRows()
        let lensFiles = rows.map(\.file).filter { $0.hasPrefix(Self.lensDirectory) }
        #expect(lensFiles.count == 4, "four lens files carry their own checklist, found \(lensFiles.count)")
        for file in lensFiles {
            let text = try Self.file(file)
            #expect(text.contains("## Read"), "\(file) has no `## Read`")
            #expect(text.contains("## Blocks on"), "\(file) has no `## Blocks on`")
            #expect(text.contains("## Never blocks on"), "\(file) has no `## Never blocks on`")
            let blocksOn = Self.section(text, "## Blocks on")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            #expect(!blocksOn.isEmpty, "\(file)'s `## Blocks on` is empty")
        }
    }

    @Test("The gate reads the ticket's lenses (TAN-213)")
    func gateReadsTheTicketsLenses() throws {
        let reviewer = try Self.file("agents/reviewer.md")
        #expect(reviewer.contains("## Lenses"), "the gate reads the ticket's `## Lenses` section")
        #expect(reviewer.contains("`code` is the default"),
                "a brief with no `## Lenses` is reviewed with `code` alone")
        #expect(reviewer.contains("never skipped silently"),
                "a named lens that cannot be read is named, never a silent pass")
        #expect(reviewer.contains("skills/review-lenses/"),
                "the gate points at the lens set it applies")
    }

    @Test("The refiner writes the lenses line (TAN-213)")
    func refinerWritesTheLensesLine() throws {
        let refiner = try Self.file("agents/refiner.md")
        #expect(refiner.contains("## Lenses"),
                "the brief shape gains `## Lenses`, after `## What`")
        #expect(refiner.contains("skills/review-lenses/"), "the refiner points at the lens table")
    }

    @Test("The roster points at the lens set (TAN-213)")
    func rosterPointsAtTheLensSet() throws {
        let team = try Self.file("skills/tandem-team/SKILL.md")
        let row = team.split(separator: "\n").first { $0.hasPrefix("| `reviewer`") }
        #expect(row?.contains("skills/review-lenses/") == true,
                "the reviewer row names the lens set it reads")
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

    @Test("CONVENTIONS states the input rule and the daily notes' read-only rule (EE991EF2)")
    func conventionsStateTheInputs() throws {
        let c = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(c.contains("three inputs"))
        #expect(c.contains("the ended session records' summaries"),
                "the session summaries are a consolidation input (ADR 0027)")
        #expect(c.contains("the human's daily notes"),
                "the human's daily notes are a consolidation input")
        #expect(c.contains("read-only to the consolidator"))
        #expect(c.contains("never edits, moves or archives"),
                "the pass folds a daily note, never edits, moves or archives it")
        #expect(c.contains("the human keeps and edits"))
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

    @Test("The bikeshed holds three areas, and the session record is not one of them (7CA67B4F)")
    func bikeshedHoldsThreeAreas() throws {
        let memory = try Self.file("skills/tandem-memory/SKILL.md")
        #expect(!memory.lowercased().contains("two layers"), "the skill miscounts the bikeshed's areas")
        for area in ["saddlebag", "daily notes", "curated"] {
            #expect(memory.contains(area), "the skill names the \(area) area")
        }
        #expect(memory.contains("tandem.db"),
                "the session record is a row in the store, not a layer of the folder (ADR 0027)")
        let conventions = try Self.file("skills/tandem-memory/CONVENTIONS.md")
        #expect(!conventions.contains("Two layers"), "CONVENTIONS heads three areas as two layers")
        #expect(conventions.contains("Three areas"), "CONVENTIONS counts the areas it lists")
    }

    @Test("The per-event capture cadence is retired — the session record is the record (7CA67B4F)")
    func perEventCaptureCadenceIsRetired() throws {
        let retired = ["Record captures as you work", "at each moment", "at each qualifying moment",
                       "never batch to session end", "don't batch to session end",
                       "files its captures", "file note captures as you work",
                       "a moment that would be lost when the session ends"]
        for path in ["AGENTS.md",
                     "skills/tandem-memory/SKILL.md",
                     "skills/tandem-memory/WRITE-PROTOCOL.md",
                     "skills/tandem-build/SKILL.md",
                     "skills/tandem/SKILL.md",
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
        // The README's skills table carries the same orient wording. It is read
        // whole like the skills page, not fixtured: the pin is the phrase, not
        // the prose around it (TAN-194).
        for path in ["AGENTS.md",
                     "skills/tandem-memory/SKILL.md",
                     "skills/tandem-sprint/SKILL.md",
                     "skills/tandem-tidy/SKILL.md",
                     "README.md"] {
            let orient = try Self.file(path)
            #expect(!Self.namesAStaleOrientPhrase(orient),
                    "\(path): the orient step names the Overview, the scaffold's anchor")
            #expect(orient.contains("**Overview**"), "\(path): the orient step names the Overview")
        }
    }

    @Test("The stale orient phrases are apostrophe-agnostic (TAN-194)")
    func staleOrientPhraseIsApostropheAgnostic() throws {
        for apostrophe in ["'", "\u{2019}", "\u{2018}"] {
            let stale = "orient from the project\(apostrophe)s curated note"
            #expect(Self.namesAStaleOrientPhrase(stale),
                    "the pin must catch the stale wording written with \(apostrophe)")
        }
        #expect(Self.namesAStaleOrientPhrase("orient from the Hub"),
                "the Hub wording is retired too")
        #expect(!Self.namesAStaleOrientPhrase("orient from the project's **Overview**"),
                "the scaffold's wording is not the stale phrase")
    }

    @Test("CONSOLIDATION reads the inputs, creates the scaffold on demand, leaves the daily notes untouched")
    func consolidationStatesThePass() throws {
        let c = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(c.contains("daily"))
        #expect(c.contains("read-only to the consolidator"))
        #expect(c.contains("creates all four"))
        #expect(c.contains("never edits, moves or archives"))
    }

    @Test("The daily notes contract — the harvest list, the read-only rule and the fold marker (EE991EF2)")
    func dailyNotesContractIsStated() throws {
        for path in ["skills/tandem-consolidate/SKILL.md",
                     "skills/tandem-memory/CONSOLIDATION.md"] {
            let c = try Self.file(path)
            #expect(c.contains("[\"daily\"]"), "\(path): the harvest list is named, `[\"daily\"]`")
            #expect(c.contains("human-owned"), "\(path): the daily notes are the human's")
            #expect(c.contains("read-only"), "\(path): agents never write them")
            #expect(c.contains("Human notes folded through"),
                    "\(path): the fold marker's wording is fixed")
            #expect(c.contains("never edits, moves or archives"),
                    "\(path): the source files are untouched")
        }
        // Discovery is the list plus the marker date — an mtime never re-folds.
        let consolidation = try Self.file("skills/tandem-memory/CONSOLIDATION.md")
        #expect(consolidation.contains("not mtime"), "the pass does not scan by mtime")
        // The agent capture path is unchanged: daily notes added no agent write path.
        let consolidate = try Self.file("skills/tandem-consolidate/SKILL.md")
        #expect(consolidate.contains("tandem-cli note capture"),
                "agent captures still go through `note capture`")
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
        #expect(readme.contains("orient from the project's **Overview**"),
                "the skills table names the Overview as the orient target (TAN-194)")
    }

    @Test("The setup surfaces run the one install verb, never a hand-made link (TAN-216)")
    func setupSurfacesRunTheInstallVerb() throws {
        let command = "tandem-cli setup install"
        for path in ["README.md", "Sources/Tandem/SetupWizard.swift"] {
            let text = try Self.file(path)
            #expect(text.contains(command), "\(path) names `\(command)`")
        }
        let setup = try Self.file("skills/tandem-setup/SKILL.md")
        #expect(setup.contains(command), "the setup skill's door is `\(command)`")
        for path in ["README.md", "Sources/Tandem/SetupWizard.swift", "skills/tandem-setup/SKILL.md"] {
            let text = try Self.file(path)
            #expect(!text.contains(#"ln -s "$PWD/skills"#),
                    "\(path) still tells a human to link a checkout by hand")
        }
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
