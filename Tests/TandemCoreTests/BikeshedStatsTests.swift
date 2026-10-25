import Foundation
import Testing
@testable import TandemCore

/// `note stats` at the core seam: the read-only memory-health readout — the
/// curated-note count, the hidden saddlebag's pending/archived depth and
/// oldest-pending age, and the newest curated-note mtime, per project plus
/// totals. Runs against a temp dir via `Bikeshed`'s explicit-directory
/// initialiser — no fixture bikeshed, no real `~/.tandem`.
@Suite("Bikeshed stats")
struct BikeshedStatsTests {

    /// A unique temp directory (not pre-created: a missing bikeshed is a valid
    /// empty bikeshed). Nested one level below a unique parent so the sibling
    /// lockfile is never shared between concurrent tests.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-stats-\(UUID().uuidString)")
            .appendingPathComponent("bikeshed")
    }

    /// A fixed instant (2026-10-18T14:02:11Z), so the capture stamp and the
    /// rendered ISO timestamps are literals the test spells out.
    private let instant = Date(timeIntervalSince1970: 1_792_332_131)
    private let instantISO = "2026-10-18T14:02:11Z"

    /// Move a captured file into the project's consumed subtree
    /// (`.raw/.done/<session>/`), as the consolidation pass does.
    private func archive(_ capturePath: String, in dir: URL, session: String = "ses_done") throws {
        let fm = FileManager.default
        let source = dir.appendingPathComponent(capturePath)
        let doneDir = dir.appendingPathComponent("projects/tandem/.raw/.done/\(session)")
        try fm.createDirectory(at: doneDir, withIntermediateDirectories: true)
        try fm.moveItem(
            at: source, to: doneDir.appendingPathComponent(source.lastPathComponent))
    }

    // MARK: - Empty

    @Test("an empty bikeshed reports no projects and zero totals")
    func emptyBikeshed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let stats = try bikeshed.stats().value

        #expect(stats.bikeshed == dir.standardizedFileURL.path)
        #expect(stats.projects.isEmpty)
        #expect(stats.totals.curatedNotes == 0)
        #expect(stats.totals.rawPending == 0)
        #expect(stats.totals.rawArchived == 0)
    }

    // MARK: - Pending + archived

    @Test("a project reports its pending queue, its consumed captures, and its oldest pending")
    func pendingAndArchived() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // Two pending captures (the older one sets `oldestPending`) …
        _ = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: "# Older", session: "ses", at: instant)
        _ = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "# Newer", session: "ses",
            at: instant.addingTimeInterval(60))
        // … and one consumed capture.
        let consumed = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "# Consumed", session: "ses",
            at: instant.addingTimeInterval(120)).value
        try archive(consumed, in: dir)

        // A curated note, with a pinned mtime so `lastConsolidated` is a literal.
        _ = try bikeshed.write(path: "projects/tandem/Note.md", text: "# Note")
        let consolidated = Date(timeIntervalSince1970: 1_792_400_000)   // 2026-10-19T08:53:20Z
        try FileManager.default.setAttributes(
            [.modificationDate: consolidated],
            ofItemAtPath: dir.appendingPathComponent("projects/tandem/Note.md").path)

        let row = try #require(try bikeshed.stats().value.projects.first)

        #expect(row.project == "tandem")
        #expect(row.curatedNotes == 1)
        #expect(row.rawPending == 2)
        #expect(row.rawArchived == 1)
        #expect(row.oldestPending == instantISO)
        #expect(row.lastConsolidated == "2026-10-19T08:53:20Z")
    }

    // MARK: - No curated notes

    @Test("a project with captures but no curated notes reports zero curated notes and no consolidation time")
    func noCuratedNotes() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        _ = try bikeshed.capture(
            project: "fresh", kind: .decision, body: "# First", session: "ses", at: instant)

        let stats = try bikeshed.stats().value
        let row = try #require(stats.projects.first)

        #expect(row.project == "fresh")
        #expect(row.curatedNotes == 0)
        #expect(row.rawPending == 1)
        #expect(row.rawArchived == 0)
        #expect(row.lastConsolidated == nil)
        #expect(row.oldestPending == instantISO)
    }

    // MARK: - Counting rules

    @Test("curated notes count recursively and prune the hidden saddlebag; totals sum the rows")
    func countingRulesAndTotals() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // A curated note in a nested folder counts; the capture beside it does not.
        _ = try bikeshed.write(path: "projects/alpha/Top.md", text: "# Top")
        _ = try bikeshed.write(path: "projects/alpha/deep/Nested.md", text: "# Nested")
        _ = try bikeshed.write(path: "projects/beta/Only.md", text: "# Only")
        _ = try bikeshed.capture(
            project: "alpha", kind: .fact, body: "# Hidden", session: "ses", at: instant)

        let stats = try bikeshed.stats().value

        #expect(stats.projects.map(\.project) == ["alpha", "beta"])
        #expect(stats.projects.first(where: { $0.project == "alpha" })?.curatedNotes == 2)
        #expect(stats.projects.first(where: { $0.project == "beta" })?.curatedNotes == 1)
        #expect(stats.totals.curatedNotes == 3)
        #expect(stats.totals.rawPending == 1)
        #expect(stats.totals.rawArchived == 0)
    }

    @Test("--project narrows the report to one folder and an unknown name is an empty list")
    func projectNarrowing() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        _ = try bikeshed.write(path: "projects/alpha/A.md", text: "# A")
        _ = try bikeshed.write(path: "projects/beta/B.md", text: "# B")

        let narrowed = try bikeshed.stats(project: "beta").value
        #expect(narrowed.projects.map(\.project) == ["beta"])
        #expect(narrowed.totals.curatedNotes == 1)

        let none = try bikeshed.stats(project: "missing").value
        #expect(none.projects.isEmpty)
        #expect(none.totals.curatedNotes == 0)
    }

    // MARK: - Encoding

    @Test("the JSON keeps both queue-age keys present as null")
    func encodingKeepsNulls() throws {
        let stats = BikeshedStats(
            bikeshed: "/tmp/bikeshed",
            projects: [BikeshedProjectStats(
                project: "tandem", curatedNotes: 0, rawPending: 0, rawArchived: 0,
                oldestPending: nil, lastConsolidated: nil)],
            totals: BikeshedStatsTotals(curatedNotes: 0, rawPending: 0, rawArchived: 0))

        let json = try StoreEncoding.bikeshedStatsResult(stats)

        #expect(json.contains("\"oldestPending\":null"))
        #expect(json.contains("\"lastConsolidated\":null"))
        // One JSON object parseable back, sorted keys (bikeshed < projects < totals).
        let object = try #require(
            try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
        #expect(object["bikeshed"] as? String == "/tmp/bikeshed")
        #expect((object["projects"] as? [Any])?.count == 1)
    }
}
