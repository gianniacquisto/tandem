import Foundation
import Testing
@testable import TandemCore

/// The health readout (`note links`, and the app's Health panel): the
/// wikilinks that resolve to nothing, the notes nothing links to, and the
/// totals. Runs against a temp dir via `Bikeshed`'s explicit-directory
/// initialiser — no fixture bikeshed, no real `~/.tandem`.
@Suite("Bikeshed health")
struct BikeshedHealthTests {

    /// A unique temp directory (not pre-created: a missing bikeshed is a valid
    /// empty bikeshed). Nested one level below a unique parent so the sibling
    /// lockfile is never shared between concurrent tests.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-health-\(UUID().uuidString)")
            .appendingPathComponent("bikeshed")
    }

    /// Write a note straight to disk (the health report reads the file set, so
    /// the notes never need to go through the core's write path).
    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    // MARK: - Resolution: what is NOT broken

    @Test("bare, qualified, aliased and fragment links all resolve — none is broken")
    func everyLinkFormResolves() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "# Note", in: dir)
        try stage("projects/tandem/Deep.md", "# Deep", in: dir)
        try stage("Linker.md", """
        A bare link: [[Note]]
        A qualified link: [[projects/tandem/Deep]]
        An aliased link: [[Note|shown as this]]
        A fragment link: [[Note#Heading]]
        An aliased fragment: [[Note#Heading|shown]]
        """, in: dir)

        let health = try bikeshed.health().value

        #expect(health.broken.isEmpty)
        #expect(health.counts.broken == 0)
        #expect(health.counts.links == 5)
    }

    @Test("a link inside a code span is not a link, so it is not broken")
    func codeSpanIsNotALink() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "A literal `[[Missing]]` in a code span.", in: dir)

        let health = try bikeshed.health().value

        #expect(health.broken.isEmpty)
        #expect(health.counts.links == 0)
    }

    // MARK: - Broken links

    @Test("a broken link is reported with its note, its target, and its line number")
    func brokenLinkIsReported() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", """
        # Note

        First line.
        A link to nowhere: [[Missing]].
        """, in: dir)

        let health = try bikeshed.health().value
        let broken = try #require(health.broken.first)

        #expect(health.broken.count == 1)
        #expect(broken.note == "Note.md")
        #expect(broken.link == "Missing")
        #expect(broken.line == 4)
        #expect(health.counts.broken == 1)
    }

    @Test("the target is reported as written — the fragment and the alias are not part of it")
    func brokenTargetDropsFragmentAndAlias() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "[[Missing#Heading|shown as this]]", in: dir)

        let health = try bikeshed.health().value
        let broken = try #require(health.broken.first)

        #expect(broken.link == "Missing")
        #expect(broken.line == 1)
    }

    @Test("an ambiguous basename is a broken link, never a silent tie-break")
    func ambiguousBasenameIsBroken() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("one/Note.md", "# One", in: dir)
        try stage("two/Note.md", "# Two", in: dir)
        try stage("Linker.md", "[[Note]]", in: dir)

        let health = try bikeshed.health().value

        #expect(health.broken.count == 1)
        #expect(health.broken.first?.link == "Note")
    }

    // MARK: - Orphans

    @Test("a note nothing links to is an orphan")
    func orphanIsReported() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // Note and Reached link to each other, so each has an incoming link.
        try stage("Note.md", "# Note\n\nSee [[Reached]].", in: dir)
        try stage("Reached.md", "See [[Note]].", in: dir)
        try stage("Alone.md", "# Alone", in: dir)

        let health = try bikeshed.health().value

        #expect(health.orphans == ["Alone.md"])
        #expect(health.counts.orphans == 1)
    }

    @Test("the Hub (Index.md) and a project's anchor note are entry points, never orphans")
    func anchorsAreNotOrphans() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Index.md", "# Index\n\n- [[tandem]]\n", in: dir)
        try stage("projects/tandem/Tandem.md", "# Tandem", in: dir)
        try stage("projects/tandem/Other.md", "# Other", in: dir)

        let health = try bikeshed.health().value

        #expect(health.orphans == ["projects/tandem/Other.md"])
    }

    @Test("a note that links out but is linked to by nothing is still an orphan")
    func outgoingLinksDoNotRescue() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "# Note", in: dir)
        try stage("Lonely.md", "See [[Note]].", in: dir)

        let health = try bikeshed.health().value

        #expect(health.orphans == ["Lonely.md"])
    }

    // MARK: - The hidden saddlebag

    @Test("the hidden saddlebag is out of the graph: its broken links are ignored")
    func rawCapturesAreIgnored() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "# Note\n\nSee [[Other]].", in: dir)
        try stage("Other.md", "See [[Note]].", in: dir)
        try stage("projects/tandem/.raw/ses/20261020T120000Z-gotcha-thing.md",
                  "A capture mentioning [[Nowhere]].",
                  in: dir)

        let health = try bikeshed.health().value

        #expect(health.broken.isEmpty)
        #expect(health.orphans.isEmpty)
        #expect(health.counts.notes == 2)
    }

    // MARK: - Empty + encoding

    @Test("an empty bikeshed reports nothing broken, no orphans, and zero counts")
    func emptyBikeshed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let health = try bikeshed.health().value

        #expect(health.broken.isEmpty)
        #expect(health.orphans.isEmpty)
        #expect(health.counts.notes == 0)
        #expect(health.counts.links == 0)
        #expect(health.counts.broken == 0)
        #expect(health.counts.orphans == 0)
    }

    @Test("the JSON keeps the fixed shape: broken rows, orphan paths, and the counts")
    func encodingShape() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("Note.md", "[[Missing]]\n\nSee [[Other]].", in: dir)
        try stage("Other.md", "See [[Note]].", in: dir)
        try stage("Alone.md", "# Alone", in: dir)

        let json = try StoreEncoding.bikeshedHealthResult(bikeshed.health().value)
        let object = try #require(
            try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])

        let broken = try #require(object["broken"] as? [[String: Any]])
        #expect(broken.count == 1)
        #expect(broken.first?["note"] as? String == "Note.md")
        #expect(broken.first?["link"] as? String == "Missing")
        #expect(broken.first?["line"] as? Int == 1)

        let orphans = try #require(object["orphans"] as? [String])
        #expect(orphans == ["Alone.md"])

        let counts = try #require(object["counts"] as? [String: Any])
        #expect(counts["notes"] as? Int == 3)
        #expect(counts["links"] as? Int == 3)
        #expect(counts["broken"] as? Int == 1)
        #expect(counts["orphans"] as? Int == 1)
    }
}
