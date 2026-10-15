import Foundation
import Testing
@testable import TandemCore

/// Backlinks (spec: "backlinks(path)"): the vault notes containing at least
/// one wikilink that resolves to the given note, each with the line that
/// mentions it. Derived state, recomputed from the file set (no stored link
/// index): one row per linking note (the line of its first resolving
/// wikilink), self-links included, broken links contribute nothing, code and
/// embeds are not links. A missing target is `unknownNote`.
@Suite("Bikeshed backlinks")
struct BikeshedBacklinkTests {

    /// A unique temp dir, nested one level below a unique parent: the
    /// vault's sibling lockfile (`bikeshed.lock`, ADR 0004) must not be
    /// shared between concurrent tests.
    private func tempDir() -> URL {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-backlinks-\(UUID().uuidString)")
        let dir = parent.appendingPathComponent("vault")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    // MARK: - Basic rows

    @Test("backlinks: one row per linking note, with the line of the link")
    func basicRow() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "see [[B]] below", in: dir)
        try stage("B.md", "bee", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows == [Backlink(path: "A.md", line: "see [[B]] below")])

        // Nothing links to A.
        #expect(try bikeshed.backlinks(path: "A.md").value.isEmpty)
    }

    @Test("backlinks: multiple links in one note yield one row (first resolving line)")
    func oneRowPerNote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "first [[B]]\nsecond [[B]] again", in: dir)
        try stage("B.md", "bee", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows == [Backlink(path: "A.md", line: "first [[B]]")])
    }

    @Test("backlinks: every form that resolves produces a row")
    func allForms() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("a/A.md", "bare [[B]]", in: dir)
        try stage("a/qualified.md", "qualified [[B]] — via [[a/A]]", in: dir)
        try stage("a/aliased.md", "aliased [[B|the bee]]", in: dir)
        try stage("a/fraged.md", "fraged [[B#Top]]", in: dir)
        try stage("a/cased.md", "cased [[b]]", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows.map(\.path) == [
            "a/A.md", "a/aliased.md", "a/cased.md", "a/fraged.md", "a/qualified.md",
        ])
        #expect(rows.contains(Backlink(path: "a/qualified.md", line: "qualified [[B]] — via [[a/A]]")))
        #expect(rows.contains(Backlink(path: "a/aliased.md", line: "aliased [[B|the bee]]")))
        #expect(rows.contains(Backlink(path: "a/fraged.md", line: "fraged [[B#Top]]")))
        #expect(rows.contains(Backlink(path: "a/cased.md", line: "cased [[b]]")))
        #expect(rows.contains(Backlink(path: "a/A.md", line: "bare [[B]]")))
    }

    @Test("backlinks: self-links are included")
    func selfLinksIncluded() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "self [[B]] reference", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows == [Backlink(path: "B.md", line: "self [[B]] reference")])
    }

    @Test("backlinks: broken links contribute nothing")
    func brokenContributeNothing() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("onlyBroken.md", "[[Missing]] and [[ghost]]", in: dir)
        try stage("mixed.md", "[[Missing]] then [[B]]", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows == [Backlink(path: "mixed.md", line: "[[Missing]] then [[B]]")])
    }

    @Test("backlinks: code and embeds are not links")
    func codeAndEmbedsIgnored() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("coded.md", "```\n[[B]]\n```", in: dir)
        try stage("embedded.md", "![[B]]", in: dir)

        #expect(try bikeshed.backlinks(path: "B.md").value.isEmpty)
    }

    @Test("backlinks: rows are sorted by the linking note's path")
    func sortedByPath() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("z/z.md", "[[B]]", in: dir)
        try stage("a/a.md", "[[B]]", in: dir)
        try stage("m.md", "[[B]]", in: dir)

        let rows = try bikeshed.backlinks(path: "B.md").value
        #expect(rows.map(\.path) == ["a/a.md", "m.md", "z/z.md"])
    }

    // MARK: - Target handling

    @Test("backlinks: a missing target is unknownNote")
    func missingTarget() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "[[B]]", in: dir)

        #expect(throws: BikeshedError.unknownNote(path: "B.md")) {
            _ = try bikeshed.backlinks(path: "B.md")
        }
    }

    @Test("backlinks: a folder target is unknownNote")
    func folderTarget() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("d/n.md", "x", in: dir)

        #expect(throws: BikeshedError.unknownNote(path: "d")) {
            _ = try bikeshed.backlinks(path: "d")
        }
    }

    // MARK: - Recomputed from the file set

    @Test("backlinks: recomputed after note creation")
    func recomputedAfterCreate() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)

        #expect(try bikeshed.backlinks(path: "B.md").value.isEmpty)

        _ = try bikeshed.write(path: "C.md", text: "new link to [[B]]")
        #expect(try bikeshed.backlinks(path: "B.md").value
            == [Backlink(path: "C.md", line: "new link to [[B]]")])
    }

    @Test("backlinks: recomputed after note deletion")
    func recomputedAfterDelete() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        #expect(try bikeshed.backlinks(path: "B.md").value
            == [Backlink(path: "A.md", line: "see [[B]]")])

        _ = try bikeshed.delete(path: "A.md")
        #expect(try bikeshed.backlinks(path: "B.md").value.isEmpty)
    }

    @Test("backlinks: recomputed after rename — the row follows the rewritten line")
    func recomputedAfterRename() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try bikeshed.backlinks(path: "C.md").value
            == [Backlink(path: "A.md", line: "see [[C]]")])
        #expect(throws: BikeshedError.unknownNote(path: "B.md")) {
            _ = try bikeshed.backlinks(path: "B.md")
        }
    }
}
