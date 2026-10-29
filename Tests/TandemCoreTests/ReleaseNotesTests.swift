import Foundation
import Testing

/// The release body generator (7B005267, BCE67BAE): the GitHub release body
/// carries what shipped — the PRs merged on the integration branch since the
/// previous tag — grouped by kind, and the same note is filed into the
/// bikeshed through `note capture` so the consolidation pass can promote it.
/// The suite runs `Scripts/release-notes.sh` against throwaway fixture repos;
/// the PR lookup is stubbed through `RELEASE_NOTES_GH` and the capture verb
/// through `RELEASE_NOTES_TANDEM`, so no test touches the network or the
/// human's bikeshed.
@Suite("Release notes generator")
struct ReleaseNotesTests {

    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent() // Tests/TandemCoreTests
        .deletingLastPathComponent() // Tests
        .deletingLastPathComponent() // repo root

    private static let lead = "Tandem 0.2.0 for macOS 27 (Apple silicon). "
        + "Install: brew install --cask gianniacquisto/tap/tandem"

    private struct FixtureError: Error { let message: String }

    /// One stubbed PR: the number, its title, and its label names.
    private struct PR {
        let number: Int
        let title: String
        var labels: [String] = []
    }

    private struct Run {
        let body: String
        let errors: String
        let status: Int32
    }

    /// The `## ` heading the bullet sits under, or `nil` when it is missing.
    private static func heading(of bullet: String, in body: String) -> String? {
        var heading = ""
        for line in body.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("## ") { heading = String(line) }
            if String(line) == bullet { return heading }
        }
        return nil
    }

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

    /// Merges a branch into `into` with an exact merge subject, the way
    /// GitHub writes PR merges.
    private static func merge(
        _ dir: URL,
        _ branch: String,
        subject: String,
        commits: Int = 1,
        into: String = "main"
    ) throws {
        try Self.git(dir, "checkout", "-b", branch, "--quiet")
        for index in 0..<max(commits, 1) {
            try Self.commit(dir, "\(branch.replacingOccurrences(of: "/", with: "-"))-\(index)")
        }
        try Self.git(dir, "checkout", into, "--quiet")
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

    /// A stub `gh`: answers `pr view <N> --json title,labels --jq …` for each
    /// stubbed PR with `<title>\t<label,label>` and fails the rest — the
    /// stand-in for an authed GitHub CLI. The reply lives in a file so a title
    /// quoting anything the shell would eat arrives byte-exact.
    private static func stubGH(_ rows: [PR]) throws -> String {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("gh-stub-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        var cases = ""
        for row in rows {
            let reply = dir.appendingPathComponent("\(row.number)")
            try Data("\(row.title)\t\(row.labels.joined(separator: ","))".utf8).write(to: reply)
            cases += "    \(row.number)) cat '\(reply.path)'; exit 0;;\n"
        }
        let path = dir.appendingPathComponent("gh")
        try Data("""
        #!/bin/sh
        [ "$1" = pr ] || exit 1
        case "$3" in
        \(cases)    *) exit 1;;
        esac
        """.utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        return path.path
    }

    /// A stub `tandem-cli`: records its arguments and the body it was handed,
    /// then answers like the verb does.
    private static func stubVerb() throws -> (path: String, arguments: URL, body: URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-stub-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("tandem-cli")
        let arguments = dir.appendingPathComponent("arguments")
        let body = dir.appendingPathComponent("body")
        try Data("""
        #!/bin/sh
        printf '%s\\n' "$@" > '\(arguments.path)'
        cat > '\(body.path)'
        printf '{"path":"projects/tandem/.raw/release-notes/x.md"}\\n'
        """.utf8).write(to: path)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path.path)
        return (path.path, arguments, body)
    }

    private static func runScript(
        in dir: URL,
        gh: String? = nil,
        tandem: String? = nil,
        capture: Bool = false,
        tag: String = "v0.2.0"
    ) throws -> Run {
        let process = Process()
        process.executableURL = Self.root.appendingPathComponent("Scripts/release-notes.sh")
        process.currentDirectoryURL = dir
        process.arguments = capture ? [tag, "--capture"] : [tag]
        var environment = ProcessInfo.processInfo.environment
        environment["RELEASE_NOTES_GH"] = gh
        environment["RELEASE_NOTES_TANDEM"] = tandem
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        process.waitUntilExit()
        return Run(
            body: String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
            errors: String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
            status: process.terminationStatus
        )
    }

    // MARK: the tests

    @Test("The PRs are grouped by kind: features, bugs and fixes, docs and infra")
    func prsAreGroupedByKind() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.merge(dir, "board/fix-b", subject: "Merge pull request #11 from gianniacquisto/board/fix-b")
        try Self.merge(dir, "board/docs-c", subject: "Merge pull request #12 from gianniacquisto/board/docs-c")
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([
            .init(number: 10, title: "Add the reassign picker"),
            .init(number: 11, title: "Fix the sheet's focus ring"),
            .init(number: 12, title: "Docs: releasing.md names the generator"),
        ])

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.status == 0)
        #expect(run.body == """
        \(Self.lead)

        ## Features

        - #10 Add the reassign picker

        ## Bugs and fixes

        - #11 Fix the sheet's focus ring

        ## Docs and infra

        - #12 Docs: releasing.md names the generator
        """ + "\n")
    }

    @Test("A section with nothing in it is omitted")
    func emptySectionsAreOmitted() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.git(dir, "tag", "v0.2.0")

        let run = try Self.runScript(in: dir, gh: try Self.stubGH([.init(number: 10, title: "Add the reassign picker")]))

        #expect(run.body == """
        \(Self.lead)

        ## Features

        - #10 Add the reassign picker
        """ + "\n")
    }

    @Test("A PR label outranks the title's words")
    func labelsOutrankTitles() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/fix-a", subject: "Merge pull request #10 from gianniacquisto/board/fix-a")
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([
            .init(number: 10, title: "Fix the reassign picker", labels: ["feature"]),
        ])

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.body.contains("## Features"), "a `feature` label wins over the title's `Fix`")
        #expect(!run.body.contains("## Bugs and fixes"))
    }

    @Test("A PR the rules cannot place falls into an explicit Other section")
    func unclassifiablePRsFallIntoSuchOther() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/tab-a", subject: "Merge pull request #10 from gianniacquisto/board/tab-a")
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([.init(number: 10, title: "Notes tab: the wiki face")])

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.body.contains("## Other (unclassified)"))
        #expect(run.body.contains("- #10 Notes tab: the wiki face"))
    }

    @Test("Each widened keyword places a title that carries only that word")
    func widenedKeywordsPlaceTheirTitles() throws {
        let cases: [(number: Int, title: String, heading: String)] = [
            (10, "store: back the ticket domain with tandem.db", "## Features"),
            (11, "sqlite: vacuum on quit", "## Features"),
            (12, "database: the import writes its marker first", "## Features"),
            (13, "migration: the legacy file is copied aside", "## Features"),
            (14, "cli: doctor prints every check", "## Features"),
            (15, "app: the ticket sheet carries the usage section", "## Features"),
            (16, "window: the tab bar keeps its place on relaunch", "## Features"),
            (17, "watcher: a write refreshes the list", "## Features"),
            (18, "board: the Status gates hold their column", "## Features"),
            (19, "help: lane list names its verb", "## Docs and infra"),
            (20, "sprint: the batch runs one ticket at a time", "## Docs and infra"),
        ]
        let dir = try Self.makeRepo()
        // A branch area of its own would classify — `board/pr-N` carries no
        // keyword, so the heading comes from the title alone.
        for row in cases {
            try Self.merge(
                dir,
                "board/pr-\(row.number)",
                subject: "Merge pull request #\(row.number) from gianniacquisto/board/pr-\(row.number)"
            )
        }
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH(cases.map { .init(number: $0.number, title: $0.title) })

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.status == 0)
        for row in cases {
            #expect(
                Self.heading(of: "- #\(row.number) \(row.title)", in: run.body) == row.heading,
                "#\(row.number) \(row.title)"
            )
        }
    }

    @Test("A title with no keyword falls back to the merge subject's branch area")
    func silentTitleFallsBackToTheBranchArea() throws {
        let dir = try Self.makeRepo()
        try Self.merge(
            dir,
            "board/sessions-migration",
            subject: "Merge pull request #10 from gianniacquisto/board/sessions-migration"
        )
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([.init(number: 10, title: "todo archive --id: archive one ticket")])

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.body.contains("## Features"))
        #expect(run.body.contains("- #10 todo archive --id: archive one ticket"))
        #expect(!run.body.contains("## Other (unclassified)"))
    }

    @Test("The v0.27.0 merges land in their sections and Other keeps only the ambiguous ones")
    func theV0270MergesAreClassified() throws {
        // The published v0.27.0 body put nine of these eleven under Other; the
        // four the ticket names (store, archive, app watcher, help wiring) and
        // the rest now land, and only the two a mechanical classifier cannot
        // read — a retro skill tweak and a reviewer merge-wait — stay behind.
        let merged: [(number: Int, branch: String, title: String, heading: String?)] = [
            (185, "board/wal-watcher",
             "app: watch the store's -wal beside the directory, so a CLI write refreshes the window (3D77D0BD)",
             "## Features"),
            (184, "board/release-notes-wiki",
             "Release notes in the wiki, grouped by features / bugs / docs+infra (BCE67BAE)",
             "## Docs and infra"),
            (182, "board/scaffold-nits",
             "Align the scaffold's own prose: risk routing and the Overview wording",
             "## Docs and infra"),
            (181, "board/sqlite-store-p1",
             "store: back the ticket domain with tandem.db (P1 of 6B96343A)",
             "## Features"),
            (183, "board/lane-list-help",
             "lane list meets the help wiring: known entry, verb text, help test",
             "## Docs and infra"),
            (168, "board/retro-child-session-record",
             "builder contract: start the session record before the first tool call",
             "## Docs and infra"),
            (158, "board/sessions-migration",
             "todo archive --id: archive one ticket",
             "## Features"),
            (171, "board/retro-transcript-truncation",
             "retro: read the transcript in cursor pages and flag partial reads",
             "## Other (unclassified)"),
            (170, "board/session-start-agent-flag",
             "AGENTS.md + sprint skill: every `session start` carries `--agent` explicitly",
             "## Docs and infra"),
            (169, "board/retro-zsh-shell-gotcha",
             "sprint skill: the harness shell is zsh — POSIX-safe loops only",
             "## Docs and infra"),
            (167, "board/reviewer-merge-wait",
             "reviewer: wait out pending checks on a BLOCKED merge, then merge",
             "## Other (unclassified)"),
        ]
        let dir = try Self.makeRepo()
        for row in merged {
            try Self.merge(
                dir,
                row.branch,
                subject: "Merge pull request #\(row.number) from gianniacquisto/\(row.branch)"
            )
        }
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH(merged.map { .init(number: $0.number, title: $0.title) })

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.status == 0)
        for row in merged {
            #expect(
                Self.heading(of: "- #\(row.number) \(row.title)", in: run.body) == row.heading,
                "#\(row.number) \(row.title)"
            )
        }
    }

    @Test("A failed title lookup falls back to the merge subject's branch")
    func titleLookupFallsBackToTheBranch() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/fix-a", subject: "Merge pull request #10 from gianniacquisto/board/fix-a")
        try Self.git(dir, "tag", "v0.2.0")

        // /usr/bin/false exists but always fails: the no-authed-gh path.
        let run = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(run.body == """
        \(Self.lead)

        ## Bugs and fixes

        - #10 (board/fix-a)
        """ + "\n")
    }

    @Test("Union merges inside PR branches are not listed")
    func unionMergesAreNotListed() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        // The union merge a PR branch carries when the integration branch moves
        // under it: on the branch, not on the integration branch's first-parent
        // path — so not a shipped PR.
        try Self.git(dir, "checkout", "board/feat-a", "--quiet")
        try Self.git(dir, "merge", "--no-edit", "-m", "Merge branch 'main' into board/feat-a", "main", "--quiet")
        try Self.git(dir, "checkout", "main", "--quiet")
        try Self.git(dir, "tag", "v0.2.0")

        let run = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(run.body == """
        \(Self.lead)

        ## Features

        - #10 (board/feat-a)
        """ + "\n")
    }

    @Test("A tag on the release merge ranges from the dev head the merge shipped")
    func releaseMergeTagRangesFromItsDevTip() throws {
        // The transition the next release runs: v0.2.0 is tagged on the dev
        // head (#10), its release PR (#21) moves `main` untagged, #11 lands on
        // dev, and v0.3.0 names the merge that shipped it. A range from the
        // tag itself sees only that merge, so every release would land in
        // Other. The release PR comes from `dev` (the instructions say so) or
        // from `release/vX.Y.Z`; both must resolve to the second parent.
        for source in ["dev", "release/v0.3.0"] {
            let dir = try Self.makeRepo()
            try Self.git(dir, "checkout", "-b", "dev", "--quiet")
            try Self.merge(
                dir,
                "board/feat-a",
                subject: "Merge pull request #10 from gianniacquisto/board/feat-a",
                into: "dev")
            try Self.git(dir, "tag", "v0.2.0")
            try Self.git(dir, "checkout", "main", "--quiet")
            try Self.git(
                dir, "merge", "--no-ff", "--no-edit", "-m",
                "Merge pull request #21 from gianniacquisto/dev",
                "v0.2.0", "--quiet")
            try Self.git(dir, "checkout", "dev", "--quiet")
            try Self.merge(
                dir,
                "board/feat-c",
                subject: "Merge pull request #11 from gianniacquisto/board/feat-c",
                into: "dev")
            if source != "dev" {
                try Self.git(dir, "checkout", "-b", source, "--quiet")
            }
            try Self.git(dir, "checkout", "main", "--quiet")
            try Self.git(
                dir, "merge", "--no-ff", "--no-edit", "-m",
                "Merge pull request #22 from gianniacquisto/\(source)",
                source, "--quiet")
            try Self.git(dir, "tag", "v0.3.0")
            let gh = try Self.stubGH([.init(number: 11, title: "Add the release picker")])

            let run = try Self.runScript(in: dir, gh: gh, tag: "v0.3.0")

            #expect(run.status == 0, "\(source)")
            #expect(run.body == """
            \(Self.lead.replacingOccurrences(of: "0.2.0", with: "0.3.0"))

            ## Features

            - #11 Add the release picker
            """ + "\n", "\(source)")
            #expect(!run.body.contains("#21"), "a release PR from `\(source)` shipped nothing of its own")
            #expect(!run.body.contains("#22"), "a release PR from `\(source)` shipped nothing of its own")
            #expect(!run.body.contains("#10"), "the previous release's PRs belong to its own notes")
            #expect(!run.body.contains("Other"), "the release merge must not be the only thing read")
        }
    }

    @Test("A dev tag on a merge whose branch mentions release still ranges from the tag")
    func devTagOnReleaseNamedBranchRangesFromTheTag() throws {
        // v0.32.0's shape: the tag sits on a board merge from
        // `board/tan-225-sprint-close-release`. Only a merge from `dev` or a
        // `release/vX.Y.Z` branch is the release merge, so this tag is its
        // own tip and the PRs before it are still listed.
        let dir = try Self.makeRepo()
        try Self.merge(
            dir,
            "board/tan-225-sprint-close-release",
            subject: "Merge pull request #10 from gianniacquisto/board/tan-225-sprint-close-release")
        try Self.merge(
            dir,
            "board/feat-b",
            subject: "Merge pull request #11 from gianniacquisto/board/feat-b")
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([
            .init(number: 10, title: "Sprint close: the release step"),
            .init(number: 11, title: "Add the release picker"),
        ])

        let run = try Self.runScript(in: dir, gh: gh)

        #expect(run.body.contains("- #10 Sprint close: the release step"))
        #expect(run.body.contains("- #11 Add the release picker"))
    }

    @Test("Non-PR merges are grouped by their subject too")
    func nonPrMergesAreGrouped() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "other", subject: "Merge branch 'other' into main")
        try Self.git(dir, "tag", "v0.2.0")

        let run = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(run.body == """
        \(Self.lead)

        ## Other (unclassified)

        - Merge branch 'other' into main
        """ + "\n")
    }

    @Test("A tag with no merged PRs since the previous tag carries the lead only")
    func tagWithNoMergesCarriesTheLeadOnly() throws {
        let dir = try Self.makeRepo()
        try Self.commit(dir, "on-main")
        try Self.git(dir, "tag", "v0.2.0")

        let run = try Self.runScript(in: dir, gh: "/usr/bin/false")

        #expect(run.body == "\(Self.lead)\n")
    }

    @Test("--capture files the grouped note through note capture as a fact")
    func captureFilesTheGroupedNote() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.merge(dir, "board/fix-b", subject: "Merge pull request #11 from gianniacquisto/board/fix-b")
        try Self.git(dir, "tag", "v0.2.0")
        let gh = try Self.stubGH([
            .init(number: 10, title: "Add the reassign picker"),
            .init(number: 11, title: "Fix the sheet's focus ring"),
        ])

        let stub = try Self.stubVerb()
        let run = try Self.runScript(in: dir, gh: gh, tandem: stub.path, capture: true)

        #expect(run.status == 0, "the capture must not fail the release body")
        // The verb's write, not the script's: the curated layer has one writer,
        // so the note lands in the saddlebag for the consolidation pass.
        let arguments = try String(contentsOf: stub.arguments, encoding: .utf8)
            .split(separator: "\n").map(String.init)
        #expect(arguments == [
            "note", "capture",
            "--kind", "fact",
            "--project", "tandem",
            "--session", "release-notes",
        ])
        let body = try String(contentsOf: stub.body, encoding: .utf8)
        #expect(body == """
        # Release 0.2.0: what shipped

        \(Self.lead)

        ## Features

        - #10 Add the reassign picker

        ## Bugs and fixes

        - #11 Fix the sheet's focus ring
        """ + "\n")
        // stdout stays the GitHub body: the release act pipes it into `gh`.
        #expect(run.body.hasPrefix(Self.lead))
        #expect(run.body.contains("## Features"))
        #expect(run.errors.contains("release-notes"), "the filed path is reported on stderr")
    }

    @Test("--capture names the missing verb instead of silently skipping the wiki")
    func captureNamesAMissingVerb() throws {
        let dir = try Self.makeRepo()
        try Self.merge(dir, "board/feat-a", subject: "Merge pull request #10 from gianniacquisto/board/feat-a")
        try Self.git(dir, "tag", "v0.2.0")

        let run = try Self.runScript(
            in: dir,
            gh: "/usr/bin/false",
            tandem: "/nonexistent/tandem-cli",
            capture: true
        )

        #expect(run.status != 0, "a release that asked for the wiki note must not pass silently")
        #expect(run.errors.contains("tandem-cli"))
    }

    @Test("The workflow publishes the generated body and the docs record the wiki note")
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
        #expect(releasing.contains("--capture"), "docs/releasing.md must name the bikeshed step")
        #expect(releasing.contains("note capture"), "docs/releasing.md must name the verb the note goes through")
        #expect(releasing.contains("Release"), "docs/releasing.md must name the note's shape")
        // The order the ticket flips: the release PR merges first, the tag names
        // the head it left on `main`. A live instruction that tags `dev` is the
        // race v0.32.0 lost, so it must not survive anywhere in the step.
        let mergeStep = try #require(releasing.range(of: "merge it once its `release` check is green"))
        let tagStep = try #require(releasing.range(of: "Tag `main`'s head"))
        #expect(mergeStep.lowerBound < tagStep.lowerBound, "the tag follows the merge")
        #expect(!releasing.contains("Tag the `dev` head"), "no live instruction tags the dev head")
        let adr = try String(
            contentsOf: Self.root.appendingPathComponent("docs/adr/0019-integration-branch.md"),
            encoding: .utf8
        )
        #expect(adr.contains("the tag follows the merge"), "ADR 0019 records the flip")
    }
}
