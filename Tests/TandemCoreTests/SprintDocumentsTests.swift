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

    /// TAN-225 and TAN-229: the sprint that lands the batch is the pass that ships it.
    /// §7 carries the release step, it sits before the close verb because a tag can
    /// only be cut once the batch is merged and done, and since the trunk flip it is
    /// one act: a tag on `main`'s head, with no `Release vX.Y.Z` PR. The tap bump is
    /// still a hand act until TAN-224 lands, so the step names it as the one step the
    /// close cannot take.
    @Test("§7 cuts the release its batch landed, before the close verb")
    func closeCutsTheRelease() throws {
        let skill = try Self.file("skills/tandem-sprint/SKILL.md")
        let heading = try #require(
            skill.range(of: "## 7. Close"),
            "the sprint skill must still carry the close as §7")
        let tail = skill[heading.lowerBound...]
        let board = tail.range(of: "\n## The board")
        let section = String(board.map { tail[..<$0.lowerBound] } ?? tail)

        let release = try #require(
            section.range(of: "Cut the release"),
            "§7 must carry the release step; nothing else cuts the tag its batch landed")
        let closeVerb = try #require(
            section.range(of: "sprint close --lane"),
            "§7 still closes the record with the verb")
        #expect(
            release.lowerBound < closeVerb.lowerBound,
            "the release is the close's first act: a tag needs the batch merged and done")
        #expect(
            section.contains("git tag vX.Y.Z") && section.contains("push origin vX.Y.Z"),
            "the tag is cut and pushed, so the tag run publishes it")
        let tagLine = try #require(
            section.split(separator: "\n").first { $0.contains("git tag vX.Y.Z") },
            "the tag command is on its own line, so its target can be read")
        #expect(
            tagLine.contains("main"),
            "the tag names `main`'s head: `main` takes the merges, so its head is the release")
        #expect(
            !section.contains("Release vX.Y.Z"),
            "§7 must not carry a `Release vX.Y.Z` PR: the release PR is retired with the split")
        #expect(
            section.contains("Scripts/release-notes.sh vX.Y.Z --capture"),
            "the notes reach the bikeshed from a checkout with the tags")
        #expect(
            section.contains("TAP_TOKEN"),
            "the tap bump is a hand act until TAN-224 lands, and the step must say so")
        #expect(
            section.contains("cuts no tag"),
            "a sprint that landed nothing cuts no tag, and the close says so in one line")
        #expect(
            section.contains("names the tag"),
            "the close note names the tag its batch shipped in")

        let releasing = try Self.file("docs/releasing.md")
        #expect(
            releasing.contains("skills/tandem-sprint/SKILL.md"),
            "releasing.md points at the sprint close as the path that cuts the tag")
    }

    /// TAN-229: ADR 0019 is amended, not rewritten. The new dated section records
    /// the reversal the trunk flip made, and the two amendments before it stay.
    @Test("ADR 0019 records the reversal, dated")
    func adr0019RecordsTheReversal() throws {
        let adr = try Self.file("docs/adr/0019-integration-branch.md")
        #expect(
            adr.contains("## Amendment, 2026-10-29: the split is retired"),
            "ADR 0019 must carry the dated section that retires the split")
        #expect(
            adr.contains("## Amendment, 2026-10-23:"),
            "an amendment does not rewrite history: the 2026-10-23 amendment stays")
        #expect(
            adr.contains("# Integration branch: `dev` takes merges, `main` tracks the latest tag"),
            "an amendment does not rewrite history: the original title line stays")
        #expect(
            adr.contains("A release is a lightweight tag on `main`'s head"),
            "the amendment must name the tag on `main`'s head as the release")
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
        let body = close
            .drop(while: { $0 != "\n" })
            .drop(while: { $0.isNewline })
        let opening = try #require(
            body.components(separatedBy: "\n\n").first,
            "§7 carries an opening paragraph")
        #expect(
            opening.hasPrefix("**The precondition:"),
            "the precondition opens the close step, ahead of the settle sweep and the close verb")
        #expect(
            opening.contains("merged and `done`"),
            "the precondition is the merge and the board write, both")
        #expect(
            opening.contains("does not lift it"),
            "an escalation does not lift the precondition")
        #expect(
            opening.contains("waits on the human"),
            "the sprint waits on the human rather than closing")
        #expect(
            opening.contains("merge and the board write"),
            "the close comes after the merge and the board write")
    }

    /// TAN-220's Done when: no live instruction keeps the old escalation wording,
    /// in the skill the sprint hands its build phase to as much as in §7.
    @Test("no live instruction keeps the old escalation wording")
    func noLiveInstructionKeepsTheOldWording() throws {
        let sprint = try Self.file("skills/tandem-sprint/SKILL.md")
        let build = try Self.file("skills/tandem-build/SKILL.md")
        for (name, text) in [("tandem-sprint", sprint), ("tandem-build", build)] {
            #expect(
                !text.contains("does not stop the sprint"),
                "\(name) must not say an escalation leaves the sprint running")
            #expect(
                !text.contains("the close names it"),
                "\(name) must not say the close only names an escalated ticket")
        }
        #expect(
            build.contains("leaves the batch"),
            "an escalated ticket still leaves the batch")
        #expect(
            build.contains("the sprint waits"),
            "the sprint waits for the escalated ticket")
        #expect(
            build.contains("after the merge and the board write"),
            "the close comes after the merge and the board write")
    }
}
