import Foundation
import Testing

/// The release body generator (7B005267): the GitHub release body carries
/// what shipped — the PRs merged on the integration branch since the
/// previous tag — not a fixed tag line. The suite runs
/// `Scripts/release-notes.sh` against throwaway fixture repos; the PR-title
/// lookup is stubbed through `RELEASE_NOTES_GH` so no test touches the
/// network.
@Suite("Release notes generator")
struct ReleaseNotesTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static let lead = "Tandem 0.2.0 for macOS 27 (Apple silicon). "
        + "Install: brew install --cask gianniacquisto/tap/tandem"

    private struct FixtureError: Error { let message: String }

    // MARK: fixture repo

    /// A throwaway git repo: `base` committed on `main`, tagged `v0.1.0`.
    private static func makeRepo() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("release-notes-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Self.git(dir, "init", "-b", "main", ".")
        try Self.git(dir, "config", "user.name", "Test")
        try Self.git(dir, "config", "user.email", "test@example.com")
        try Self.commit(dir, "base")
        try Self.git(dir, "tag", "v0.1.0")
        return dir
    }

    private static func commit(_ dir: URL, _ name: String) throws {
        try Data("contents".utf8).write(to: dir.appendingPathComponent(name))
        try Self.git(dir, "add", ".")
        try Self.git(dir, "commit", "-m", name, "--quiet")
    }

    /// Merges a branch into `main` with an exact merge subject, the way
    /// GitHub writes PR merges.
    private static func merge(
        _ dir: URL,
        _ branch: String,
        subject: String,
        commits: Int = 1
    ) throws {
        try Self.git(dir, "checkout", "-b", branch, "--quiet")
        for index in 0..<max(commits, 1) {
            try Self.commit(dir, "\(branch.replacingOccurrences(of: "/", with: "-"))-\(index)")
        }
        try Self.git(dir, "checkout", "main", "--quiet")
        try Self.git(dir, "merge", "--no-ff", "--no-edit", "-m", subject, branch, "--quiet")
    }

    private static func git(_ dir: URL, _ args: String...) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = dir
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw FixtureError(message: "git \(args.joined(separator: " ")) failed")
        }
    }

    /// A stub `gh`: answers `pr view <N>` with a fixed title, fails anything
    /// else — the stand-in for an authed GitHub CLI in the test's directory.
    private static func stubGH() throws -> String {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("gh-stub-\(UUID().uuidString)")
        try Data("#!/bin/sh\nif [ \"$1\" = pr ]; then echo \"Add the reassign picker\"; exit 0; fi\nexit 1\n"
            .utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        return path.path
    }

    private static func runScript(
        in dir: URL,
        gh: String? = nil,
        tag: String = "v0.2.0"
    ) throws -> String {
        let process = Process()
        process.executableURL = Self.root.appendingPathComponent("Scripts/release-notes.sh")
        process.currentDirectoryURL = dir
        process.arguments = [tag]
        var environment = ProcessInfo.processInfo.environment
        environment["RELEASE_NOTES_GH"] = gh
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        process.waitUntilExit()
        let body = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        guard process.terminationStatus == 0 else {
            throw FixtureError(message: "release-notes.sh failed: "
                + String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!)
        }
        _ = tag
        return body
    }

    // MARK: the tests

    @Test("The body carries the lead line and the merged PRs' titles")
    func bodyCarriesTheLeadAndTheTitles() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.git(dir, "tag", "v0.2.0")

        let body = try Self.runScript(in: dir, gh: try Self.stubGH())

        #expect(body == """
        \(Self.lead)

        ## What shipped

        - #10 Add the reassign picker
        """ + "\n")
    }

    @Test("A failed title lookup falls back to the merge subject's branch")
    func titleLookupFallsBackToTheBranch() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.git(dir, "tag", "v0.2.0")

        // /usr/bin/false exists but always fails: the no-authed-gh path.
        let body = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(body == """
        \(Self.lead)

        ## What shipped

        - #10 (board/feat-a)
        """ + "\n")
    }

    @Test("Union merges inside PR branches are not listed")
    func unionMergesAreNotListed() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        // The union merge a PR branch carries when dev moves under it: on the
        // branch, not on main's first-parent path — so not a shipped PR.
        try Self.git(dir, "checkout", "board/feat-a", "--quiet")
        try Self.git(dir, "merge", "--no-edit", "-m", "Merge branch 'main' into board/feat-a", "main", "--quiet")
        try Self.git(dir, "checkout", "main", "--quiet")
        try Self.git(dir, "tag", "v0.2.0")

        let body = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(body == """
        \(Self.lead)

        ## What shipped

        - #10 (board/feat-a)
        """ + "\n")
    }

    @Test("Non-PR merges are listed as their subject")
    func nonPrMergesListTheirSubject() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "other", subject: "Merge branch 'other' into main")
        try Self.git(dir, "tag", "v0.2.0")

        let body = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(body == """
        \(Self.lead)

        ## What shipped

        - Merge branch 'other' into main
        """ + "\n")
    }

    @Test("A tag with no merged PRs since the previous tag carries the lead only")
    func tagWithNoMergesCarriesTheLeadOnly() throws {
        let dir = try Self.makeRepo()
        try Self.commit(dir, "on-main")
        try Self.git(dir, "tag", "v0.2.0")

        let body = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(body == "\(Self.lead)\n")
    }

    @Test("The workflow publishes the generated body and the docs say so")
    func theWorkflowAndDocsNameTheGenerator() throws {
        let workflow = try String(
            contentsOf: Self.root.appendingPathComponent(".github/workflows/release.yml"),
            encoding: .utf8
        )
        let releasing = try String(
            contentsOf: Self.root.appendingPathComponent("docs/releasing.md"),
            encoding: .utf8
        )
        #expect(workflow.contains("Scripts/release-notes.sh"), "the publish step must run the generator")
        #expect(workflow.contains("fetch-depth: 0"), "the release job's checkout must carry the tags")
        #expect(releasing.contains("release-notes.sh"), "docs/releasing.md step 2 must name the generator")
        #expect(releasing.contains("what shipped"), "docs/releasing.md must say the body carries what shipped")
    }
}
