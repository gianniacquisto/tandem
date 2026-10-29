import Foundation
import Testing

/// The sprint record's documents (ADR 0032, ticket 02CA3EC2): the switch that makes
/// `skills/tandem-sprint/SKILL.md`, `skills/tandem-build/SKILL.md`, the glossary and
/// ADR 0029 name the record instead of a board ticket.
///
/// What it pins is narrow on purpose — the sentences a switch would silently break:
/// the number is the record's to derive, the batch lives on the record (not in a
/// comment or a description), the close is a verb and stamps once, and the
/// dead-sprint rule reads the record's own `session` rather than a ticket title.
/// A sentence here that passes against any text is a bug in the test, not coverage.
@Suite("Sprint documents")
struct SprintDocumentsTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    private static func adr0032() throws -> String {
        let adr = try FileManager.default
            .contentsOfDirectory(atPath: Self.root.appendingPathComponent("docs/adr").path)
            .first { $0.hasPrefix("0032-") }
        let name = try #require(adr, "ADR 0032 is the sprint records decision and must exist")
        return try Self.file("docs/adr/\(name)")
    }

    /// ADR 0032 is the decision in ADR 0027's shape: the record, its store, the pair
    /// handle, the stored batch, the run log and the close, and the tab.
    @Test("ADR 0032 records the sprint as a record, in 0027's shape")
    func adr0032RecordsTheSprint() throws {
        let adr = try Self.adr0032()
        for heading in ["## Decision", "## Why", "## Decided against", "## Consequences"] {
            #expect(adr.contains(heading), "ADR 0032 must carry \(heading), as ADR 0027 does")
        }
        #expect(
            adr.contains("The handle is the pair (lane, number)"),
            "a sprint's identity is `Sprint <n> — <lane>`, not a UUID")
        #expect(
            adr.contains("`sprints` table"),
            "the record is stored in the one database (ADR 0031), and the ADR must say where")
        #expect(
            adr.contains("ADR 0029 is amended"),
            "the ADR must name the line of ADR 0029 it replaces")
        #expect(
            adr.contains("`session`"),
            "the dead-sprint check reads the record's opening session")
    }

    /// ADR 0029's batch decision is amended, not rewritten: the line keeps its history
    /// and carries the dated pointer to ADR 0032; the roster and the minutes stand.
    @Test("ADR 0029's batch line carries a dated pointer to ADR 0032")
    func adr0029PointsAt0032() throws {
        let adr = try Self.file("docs/adr/0029-the-ceremony-set.md")
        #expect(
            adr.contains("**The sprint's batch is a ticket, not a field.**"),
            "the superseded line keeps its history; the amendment does not rewrite it")
        #expect(
            adr.contains("**Superseded by ADR 0032** (sprint records, 2026-10-23)"),
            "the line names the record that replaces it, and the date the decision was made")
        #expect(
            adr.contains("## Amendment, 2026-10-26:"),
            "the amendment is dated, like 0029's two earlier amendments")
        #expect(
            adr.contains("roster and the minutes stand"),
            "only the batch decision flips; the rest of 0029 is untouched")
    }

    /// The sprint skill drives the record's verbs: `open` derives the number, `ticket`
    /// fills the stored batch, `run` is the run log, `close` is the close.
    @Test("the sprint skill names the record's verbs")
    func sprintSkillNamesTheRecordVerbs() throws {
        let skill = try Self.file("skills/tandem-sprint/SKILL.md")
        for verb in [
            "tandem-cli sprint open --lane",
            "tandem-cli sprint ticket --lane",
            "tandem-cli sprint run --lane",
            "tandem-cli sprint close --lane",
        ] {
            #expect(skill.contains(verb), "the sprint skill must show \(verb)")
        }
        #expect(
            skill.contains("the record derives the number")
                || skill.contains("The record derives the number")
                || skill.contains("`sprint open` answers"),
            "the number is derived by the record, never by the skill")
        #expect(
            !skill.contains("ticket add --title \"Sprint"),
            "a sprint is no longer opened as a board ticket")
        #expect(
            !skill.contains("One comment per batch ticket naming the sprint"),
            "the per-batch comment dies at the switch — the stored batch is the machine truth")
        #expect(
            !skill.contains("description is **written once and appended to**"),
            "the run log and the close are fields on the record, not a description")
    }

    /// A dead sprint is found from the record: `sprint list --status open`, and the
    /// record's `session` field. The stale-session signals stay, read against that id.
    @Test("the dead-sprint rule reads the record, not a ticket title")
    func deadSprintRuleReadsTheRecord() throws {
        let skill = try Self.file("skills/tandem-sprint/SKILL.md")
        #expect(
            skill.contains("tandem-cli sprint list --status open"),
            "open records are the dead-sprint candidates")
        #expect(
            skill.contains("`sprint close --lane"),
            "triage closes a dead sprint with the record's own verb")
        #expect(
            !skill.contains("in-progress` ticket titled `Sprint"),
            "the rule no longer reads a ticket title")
        #expect(
            skill.contains("stale `time.updated`"),
            "the stale-session signals stay — only the record they are read against changes")
    }

    /// The build skill records a landing on the record and takes the mode from the
    /// open, not from a line under a description heading.
    @Test("the build skill records runs on the record and reads the stored mode")
    func buildSkillRecordsOnTheRecord() throws {
        let skill = try Self.file("skills/tandem-build/SKILL.md")
        #expect(
            skill.contains("tandem-cli sprint run --lane"),
            "the Record step appends a run entry to the sprint record")
        #expect(
            skill.contains("--mode"),
            "the mode is chosen at `sprint open`, and the build skill must say so")
        #expect(
            !skill.contains("sprint ticket is open"),
            "the sprint is a record, and the build skill must not call it a ticket")
        #expect(
            !skill.contains("one line under `## The batch`"),
            "the mode is stored by the record, not written under a description heading")
    }

    /// Tidy's liveness line leaves the sprint to its own skill; the sprint is not a
    /// ticket for tidy to demote.
    @Test("tidy leaves the sprint record to the sprint skill")
    func tidyLeavesTheRecordAlone() throws {
        let skill = try Self.file("skills/tandem-tidy/SKILL.md")
        #expect(
            !skill.contains("The one exception is the sprint ticket itself"),
            "a sprint record is outside the board's claims, not an exception to them")
        #expect(
            skill.contains("skills/tandem-sprint/SKILL.md"),
            "the dead-sprint clause stays the sprint skill's")
    }

    /// The close settles the lane's open set before it writes the close: no ticket is
    /// left `in-progress` behind a dead claim, `in-review` keeps its PR number, `human`
    /// only while a question is open. TAN-149.
    @Test("the close settles the lane's open set before it stamps")
    func closeSettlesTheLanesOpenSet() throws {
        let skill = try Self.file("skills/tandem-sprint/SKILL.md")
        let clause = try #require(
            skill.range(of: "The lane's open set is settled before the close is written"),
            "the close carries the settle sweep, in the closing section")
        let closeLine = try #require(
            skill.range(of: "`sprint close --lane \"$lane\" --note -`"),
            "the close verb is still shown")
        #expect(
            clause.lowerBound < closeLine.lowerBound,
            "the sweep runs before the close is written, not after it stamps")
        #expect(
            skill.contains("`on-hold` when it is parked on purpose"),
            "a parked ticket goes to `on-hold`, not back to `ready`")
        #expect(
            skill.contains("a ticket this sprint drew but did not finish"),
            "an unfinished batch ticket says so instead of sitting behind a dead claim")
        #expect(
            skill.contains("keeps its PR number in the log"),
            "an `in-review` ticket leaves the merge or the escalation as its only open move")
        #expect(
            skill.contains("`human` stays only where a question is actually open"),
            "an answered ticket is not left in the human's queue")
        #expect(
            skill.contains("One comment per ticket the close moves, none where nothing moved"),
            "the sweep comments on what it moved, and stays quiet where nothing did")
        #expect(
            skill.contains("signed as the role that owned the pass"),
            "the comment is signed by the role whose work it reports")
        #expect(
            skill.contains("The close note names what was settled"),
            "the close note carries the sweep, so the next session does not redo it")
    }

    /// Tidy's liveness line is the close sweep invoked on demand, not a second rule.
    @Test("tidy's liveness line points at the close sweep")
    func tidyLivenessLineIsTheCloseSweepOnDemand() throws {
        let skill = try Self.file("skills/tandem-tidy/SKILL.md")
        #expect(
            skill.contains("the same rule invoked on demand"),
            "tidy runs the sprint's close sweep on demand, it invents no verdict of its own")
        #expect(
            skill.contains("skills/tandem-sprint/SKILL.md"),
            "the demand run points at the rule's home")
    }

    /// The glossary defines Sprint, and the Avoid list names the retired phrase.
    @Test("the glossary defines Sprint and avoids the old phrase")
    func glossaryDefinesSprint() throws {
        let glossary = try Self.file("GLOSSARY.md")
        let start = try #require(glossary.range(of: "**Sprint**:"))
        let tail = glossary[start.lowerBound...]
        let entry = tail.range(of: "\n\n").map { tail[..<$0.lowerBound] } ?? tail
        let lower = entry.lowercased()
        #expect(lower.contains("lane"), "the entry names the lane")
        #expect(lower.contains("batch"), "the entry names the batch")
        #expect(lower.contains("run log"), "the entry names the run log")
        #expect(lower.contains("close"), "the entry names the close")
        #expect(lower.contains("two states"), "a sprint has its own two states")
        #expect(lower.contains("not a ticket"), "the entry says what a sprint is not")
        #expect(entry.contains("_Avoid_: sprint ticket"), "the retired phrase is the Avoid line")
    }

    /// The close's precondition (TAN-220): the sprint closes only after every batch
    /// ticket is merged and `done`. An escalation does not lift it; the sprint waits.
    @Test("the close opens with the precondition that the batch is merged and done")
    func closeWaitsForTheBatch() throws {
        let skill = try Self.file("skills/tandem-sprint/SKILL.md")
        let close = try #require(
            skill.components(separatedBy: "\n## ").first { $0.hasPrefix("7. Close") },
            "the sprint skill carries §7")
        let precondition = try #require(
            close.range(of: "**The precondition:"),
            "the close opens with its precondition")
        let closeVerb = try #require(
            close.range(of: "sprint close --lane"),
            "§7 still shows the close verb")
        #expect(
            precondition.lowerBound < closeVerb.lowerBound,
            "the precondition comes before the close verb, not after it")
        #expect(
            close.contains("merged and `done`"),
            "the precondition is the merge and the board write, both")
        #expect(
            close.contains("does not lift it"),
            "an escalation does not lift the precondition")
        #expect(
            close.contains("waits on the human"),
            "the sprint waits on the human rather than closing")
    }
}
