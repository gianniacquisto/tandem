import Foundation
import Testing

/// The trunk is `main` (ADR 0019, amended 2026-10-29): every agent contract a future
/// builder reads names `origin/main` as the base, and none of them names `dev` at all,
/// because the branch was deleted (`dev`'s last tag, v0.33.0, shipped from `main`).
///
/// The seam is the repo's own files read from the checkout root, the one
/// `SprintDocumentsTests` uses: a prose change has no higher seam, and this suite runs
/// in the `test` job of the Release workflow, so an edit that names `dev` again in an
/// instruction fails CI. The rows are byte-exact phrases on purpose, so a failure names
/// the file and the sentence the next builder would re-derive from.
///
/// A row pins the files known today; the sweep reads the directories, so a contract
/// added later is held to the same matcher without editing this suite. Only a document
/// that records the retirement (ADR 0019) is out of it: history is allowed to say `dev`.
@Suite("Trunk flow")
struct TrunkFlowTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    /// One row per live instruction: the file and what it must say. `dev` is checked for
    /// every row, in every sentence: a live instruction that names the retired branch
    /// sends the next builder to a branch that no longer exists.
    private static let liveInstructions: [(path: String, must: [String])] = [
        ("agents/builder.md", ["origin/main"]),
        ("skills/tandem-build/SKILL.md", ["origin/main"]),
        ("skills/tandem-sprint/SKILL.md", ["origin/main"]),
        ("README.md", ["Branch off `main`"]),
        ("docs/releasing.md", ["`main`'s head is the release"]),
        ("terse/README.md", ["`main`"]),
        ("terse/install.sh", ["on main?"]),
        ("AGENTS.md", []),
    ]

    /// A `dev` that names no branch, subtracted before the word scan, so a shell redirect
    /// or a state directory is not read as an instruction: `/dev/null` is the null device.
    /// `~/.tandem-dev` and `development` need no entry here, because the token rule below
    /// never yields a bare `dev` from either.
    private static let notBranchNames = ["/dev/null"]

    /// Every occurrence of the retired branch's name as a standalone word: a token of
    /// letters, digits, `-` and `_` that reads `dev`. `~/.tandem-dev`, the dev-mode state
    /// directory, is one token and not a branch name, and neither is `development`.
    private static func namedDev(_ text: String) -> [String] {
        var scrub = text
        for context in Self.notBranchNames {
            scrub = scrub.replacingOccurrences(of: context, with: " ")
        }
        return scrub.split { !($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }
            .map(String.init)
            .filter { $0.lowercased() == "dev" }
    }

    @Test("no live instruction names the retired branch")
    func noLiveInstructionNamesDev() throws {
        for row in Self.liveInstructions {
            let text = try Self.file(row.path)
            for phrase in row.must {
                #expect(
                    text.contains(phrase),
                    "\(row.path) must carry \(phrase): the trunk is `main` (ADR 0019)")
            }
            let named = Self.namedDev(text)
            #expect(
                named.isEmpty,
                "\(row.path) names `dev` \(named.count) time(s): the branch was deleted, the trunk is `main` (ADR 0019)")
        }
    }

    /// The branch names a file mentions as `origin/<name>`, cleaned of the punctuation
    /// around them (`origin/main...origin/<branch>` names one).
    private static func namedBases(_ text: String) -> Set<String> {
        Set(
            text.components(separatedBy: "origin/")
                .dropFirst()
                .map { component in
                    component.prefix { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" }
                }
                .map(String.init)
                .filter { !$0.isEmpty }
        )
    }

    /// Every `SKILL.md` under `skills/`, repo-relative. The listing is read from disk
    /// so a skill added later is swept without editing this suite.
    private static func skillDocuments() throws -> [String] {
        let walk = try #require(
            FileManager.default.enumerator(
                at: root.appendingPathComponent("skills"),
                includingPropertiesForKeys: nil),
            "skills/ must be readable from the checkout root")
        var documents: [String] = []
        for case let url as URL in walk where url.lastPathComponent == "SKILL.md" {
            documents.append(url.path.replacingOccurrences(of: root.path + "/", with: ""))
        }
        return documents.sorted()
    }

    /// The contracts the sweep holds to the named rows' matcher: every agent prompt and
    /// every `SKILL.md`, read from disk so an addition is swept without editing this suite.
    private static func sweptDocuments() throws -> [String] {
        let agents = try FileManager.default
            .contentsOfDirectory(atPath: root.appendingPathComponent("agents").path)
            .filter { $0.hasSuffix(".md") }
            .map { "agents/\($0)" }
        return agents.sorted() + (try skillDocuments())
    }

    /// TAN-229: the fixed rows above name the files known today; this sweep reads the
    /// directory, so a contract added later cannot quietly name `dev`. TAN-231: it holds
    /// each discovered file to the rows' own matcher, not to three phrasings, so a new
    /// instruction shape fails instead of shipping. ADR 0019 is out of the sweep: it
    /// records the retirement, and history is allowed to name the branch.
    @Test("no skill or agent prompt names the integration branch as dev")
    func noSkillOrAgentPromptNamesDev() throws {
        let contracts = try Self.sweptDocuments()
        #expect(
            contracts.contains("skills/tandem-sprint/SKILL.md"),
            "the sweep must read the sprint skill: an empty listing proves nothing")
        for path in contracts {
            let named = Self.namedDev(try Self.file(path))
            #expect(
                named.isEmpty,
                "\(path) names `dev` \(named.count) time(s): the branch was deleted, the trunk is `main` (ADR 0019)")
        }

        let releasing = try Self.file("docs/releasing.md")
        #expect(
            releasing.contains("`main`'s head is the release"),
            "docs/releasing.md must name `main` as the branch the tag is cut on")
    }

    /// TAN-231: both halves of the guard, from strings this repo does not contain, so the
    /// sweep is proven to catch a phrasing it was never written against and the ADR
    /// exemption is proven to be about the path, not about the wording.
    @Test("a new phrasing of the retired branch fails the sweep, and ADR 0019 passes")
    func sweepCatchesEveryPhrasing() throws {
        let fixture = "Open the PR: branch off `dev`, then merge it."
        #expect(
            !Self.namedDev(fixture).isEmpty,
            "a contract that says to branch off `dev` must fail the sweep, whatever its wording")
        #expect(
            Self.namedDev("2>/dev/null").isEmpty,
            "`/dev/null` is the null device, not a branch: the sweep must not fail on a shell redirect")

        let history = try Self.file("docs/adr/0019-integration-branch.md")
        #expect(
            !Self.namedDev(history).isEmpty,
            "ADR 0019 must still name the branch: it records the retirement")
        #expect(
            !(try Self.sweptDocuments()).contains("docs/adr/0019-integration-branch.md"),
            "ADR 0019 stays out of the sweep: it records the retirement, it does not instruct")
    }

    @Test("the builder prompt and the build skill agree on the base")
    func builderPromptAndBuildSkillAgree() throws {
        let builder = Self.namedBases(try Self.file("agents/builder.md"))
        let build = Self.namedBases(try Self.file("skills/tandem-build/SKILL.md"))
        #expect(
            builder == build,
            "the builder prompt names \(builder.sorted()) and the build skill names \(build.sorted()); one base, or a builder re-derives the wrong branch")
        #expect(
            builder == ["main"],
            "both must name `origin/main` as the base, not \(builder.sorted())")
    }
}
