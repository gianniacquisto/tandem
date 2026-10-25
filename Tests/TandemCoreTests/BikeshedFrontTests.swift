import Foundation
import Testing
@testable import TandemCore

/// The wiki front page (4E5E9A0B): `frontPage()` derives one row per project
/// folder under `projects/` — the project's Overview (matched by normalized
/// name) with its scaffold page set (Decisions, Risks, Processes, at the
/// project root, in the fixed order), the "where this left off" state line
/// (the first non-empty line after the Overview's `## Where left off`
/// heading), and the freshness stamp (the Overview's on-disk mtime — the
/// consolidation pass rewrites the Overview exactly when the state changes).
/// Group sub-folders inside a project are neither rows nor scaffold pages;
/// a project folder without an Overview is skipped; the curated layer is the
/// only thing read (the hidden saddlebag is never a row or a page).
@Suite("Bikeshed front page")
struct BikeshedFrontTests {

    private func tempDir() -> URL {
        /// Nested one level below a unique parent so the bikeshed's sibling
        /// lockfile (`bikeshed.lock`) is never shared between concurrent tests.
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-front-\(UUID().uuidString)")
        let dir = parent.appendingPathComponent("bikeshed")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    // MARK: - rows

    @Test("front page: one row per project folder, sorted by name; topics/ is not a project")
    func oneRowPerProject() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n\n## Where left off\n\n2026-10-25: state.\n", in: dir)
        try stage("projects/code-rank/Code Rank.md", "# Code Rank\n", in: dir)
        try stage("topics/Something.md", "# Something\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.map(\.name) == ["Code Rank", "Tandem"])
        #expect(rows.map(\.folder) == ["projects/code-rank", "projects/tandem"])
    }

    @Test("front page: the Overview is matched by normalized name — folder `code-rank` ↔ `Code Rank.md`")
    func overviewMatchedByNormalizedName() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/code-rank/Code Rank.md", "# Code Rank\n", in: dir)
        try stage("projects/code-rank/Decoy.md", "# Decoy\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.count == 1)
        #expect(rows[0].pages.map(\.path) == ["projects/code-rank/Code Rank.md"])
    }

    @Test("front page: the project name is the Overview's title line, `#`s stripped; empty title falls back to the folder name")
    func nameIsTheOverviewTitle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "##   Tandem  \nbody\n", in: dir)
        try stage("projects/quiet/quiet.md", "\nbody\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.map(\.name) == ["quiet", "Tandem"])
    }

    // MARK: - scaffold pages

    @Test("front page: the scaffold page set is Overview, Decisions, Risks, Processes — fixed order, only existing pages")
    func scaffoldPagesInFixedOrder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        try stage("projects/tandem/tandem Decisions.md", "# Tandem Decisions\n", in: dir)
        try stage("projects/tandem/tandem Processes.md", "# Tandem Processes\n", in: dir)
        try stage("projects/tandem/tandem Risks.md", "# Tandem Risks\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows[0].pages.map(\.role) == [.overview, .decisions, .risks, .processes])
        #expect(rows[0].pages.map(\.path) == [
            "projects/tandem/tandem.md",
            "projects/tandem/tandem Decisions.md",
            "projects/tandem/tandem Risks.md",
            "projects/tandem/tandem Processes.md",
        ])
    }

    @Test("front page: missing scaffold pages are omitted; a project needs only its Overview")
    func missingScaffoldPagesOmitted() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        try stage("projects/tandem/tandem Decisions.md", "# Tandem Decisions\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows[0].pages.map(\.role) == [.overview, .decisions])
    }

    @Test("front page: group sub-folders are neither rows nor scaffold pages")
    func groupSubfoldersAreNeither() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        try stage("projects/tandem/App/App.md", "# App\n", in: dir)
        try stage("projects/tandem/App/tandem Decisions.md", "# not a page\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.count == 1)
        #expect(rows[0].pages.map(\.role) == [.overview])
    }

    @Test("front page: a project folder whose name matches no note is skipped")
    func projectWithoutOverviewSkipped() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        try stage("projects/misc/Loose.md", "# Loose\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.map(\.name) == ["Tandem"])
    }

    // MARK: - state line

    @Test("front page: the state line is the first non-empty line after `## Where left off`")
    func stateLineFromWhereLeftOff() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md",
                  "# Tandem\n\n## Where left off\n\n2026-10-25: the scaffold is live.\n\n## Threads\n\n- more\n",
                  in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows[0].stateLine == "2026-10-25: the scaffold is live.")
    }

    @Test("front page: no `## Where left off` heading → no state line")
    func stateLineNilWithoutHeading() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n\nplain body text\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows[0].stateLine == nil)
    }

    // MARK: - freshness

    @Test("front page: the freshness stamp is the Overview's on-disk mtime")
    func freshnessIsOverviewMtime() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        let stamp = Date(timeIntervalSince1970: 1_790_000_000)
        try FileManager.default.setAttributes(
            [.modificationDate: stamp],
            ofItemAtPath: dir.appendingPathComponent("projects/tandem/tandem.md").path)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows[0].freshness == stamp)
    }

    // MARK: - edges

    @Test("front page: no `projects/` folder — empty front page, never an error")
    func noProjectsFolderYieldsEmpty() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("notes/Loose.md", "# Loose\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.isEmpty)
    }

    @Test("front page: the hidden saddlebag is never read — no row, no page, no state line")
    func hiddenLayerNeverRead() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage("projects/tandem/tandem.md", "# Tandem\n", in: dir)
        try stage("projects/tandem/.raw/ses/2026-10-25-decision-x.md", "## Where left off\n\nraw state\n", in: dir)

        let rows = try Bikeshed(directory: dir).frontPage().value
        #expect(rows.count == 1)
        #expect(rows[0].pages.map(\.role) == [.overview])
        #expect(rows[0].stateLine == nil)
    }
}
