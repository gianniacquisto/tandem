import Foundation
import Testing

/// The trunk is `main` (ADR 0019, amended 2026-10-29): every agent contract a future
/// builder reads names `origin/main` as the base, and none of them names `origin/dev`.
///
/// The seam is the repo's own files read from the checkout root, the one
/// `SprintDocumentsTests` uses: a prose change has no higher seam, and this suite runs
/// in the `test` job of the Release workflow, so an edit that puts `origin/dev` back
/// into an instruction fails CI. The rows are byte-exact phrases on purpose, so a
/// failure names the file and the sentence the next builder would re-derive from.
@Suite("Trunk flow")
struct TrunkFlowTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static func file(_ path: String) throws -> String {
        try String(contentsOf: root.appendingPathComponent(path), encoding: .utf8)
    }

    /// One row per live instruction: the file, what it must say, what it must not.
    private static let liveInstructions: [(path: String, must: [String], mustNot: [String])] = [
        ("agents/builder.md", ["origin/main"], ["origin/dev"]),
        ("skills/tandem-build/SKILL.md", ["origin/main"], ["origin/dev"]),
        ("README.md", ["Branch off `main`"], ["Branch off `dev`", "target `dev`"]),
        ("terse/README.md", ["`main`"], ["checkout dev"]),
        ("terse/install.sh", ["on main?"], ["on dev?"]),
    ]

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

    @Test("no live instruction branches from dev")
    func noLiveInstructionBranchesFromDev() throws {
        for row in Self.liveInstructions {
            let text = try Self.file(row.path)
            for phrase in row.must {
                #expect(
                    text.contains(phrase),
                    "\(row.path) must carry \(phrase): the trunk is `main` (ADR 0019)")
            }
            for phrase in row.mustNot {
                #expect(
                    !text.contains(phrase),
                    "\(row.path) must not carry \(phrase): `dev` is no longer the base (ADR 0019)")
            }
        }
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
