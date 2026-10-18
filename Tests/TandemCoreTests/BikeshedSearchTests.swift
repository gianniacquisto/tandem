import Foundation
import Testing
@testable import TandemCore

/// Search (spec: "search(query)"): case-insensitive substring over title
/// (the note's first line) + body (the rest — frontmatter included, the
/// core never parses it). One row per note, not per match; a title hit
/// wins and shows the first line; a body hit shows a snippet around the
/// first match (±40 chars, whitespace collapsed, ellipses at sliced
/// edges). Sorted by bikeshed path; empty bikeshed and empty (or
/// whitespace-only) query → empty results.
@Suite("Bikeshed search")
struct BikeshedSearchTests {

    private func tempDir() -> URL {
        /// Nested one level below a unique parent so the bikeshed's sibling
        /// lockfile (`bikeshed.lock`) is never shared between concurrent tests.
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-search-\(UUID().uuidString)")
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

    // MARK: - Title hits

    @Test("search: a title hit shows the first line, original case")
    func titleHit() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "My Title\nbody text", in: dir)

        let hits = try bikeshed.search(query: "my title").value
        #expect(hits == [SearchHit(path: "note.md", display: "My Title")])
    }

    @Test("search: title match is case-insensitive")
    func titleCaseInsensitive() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "My Title\nbody", in: dir)

        let hits = try bikeshed.search(query: "MY").value
        #expect(hits == [SearchHit(path: "note.md", display: "My Title")])
    }

    @Test("search: a title hit wins over a body hit — one row, first line")
    func titleWinsOverBody() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "Needle Title\nthe needle in the body", in: dir)

        let hits = try bikeshed.search(query: "needle").value
        #expect(hits == [SearchHit(path: "note.md", display: "Needle Title")])
    }

    // MARK: - Body hits (snippets)

    @Test("search: a body hit shows a snippet around the first match")
    func bodySnippet() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = String(repeating: "x", count: 60) + " NEEDLE " + String(repeating: "y", count: 60)
        try stage("note.md", "Title line\n" + body, in: dir)

        let hits = try bikeshed.search(query: "needle").value
        // The ±40 context is measured from the match edges: the slice is
        // 40 chars before "NEEDLE" (39 x's + the space) and 40 after
        // (the space + 39 y's).
        let expected = "…"
            + String(repeating: "x", count: 39) + " NEEDLE "
            + String(repeating: "y", count: 39) + "…"
        #expect(hits == [SearchHit(path: "note.md", display: expected)])
    }

    @Test("search: a short body shows no ellipses")
    func bodyNoEllipses() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "Title\nneedle at start", in: dir)

        let hits = try bikeshed.search(query: "needle").value
        #expect(hits == [SearchHit(path: "note.md", display: "needle at start")])
    }

    @Test("search: the snippet collapses whitespace across lines")
    func bodySnippetCollapsesWhitespace() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "Title\nfirst line\nsecond needle here\nthird line", in: dir)

        let hits = try bikeshed.search(query: "needle").value
        guard let hit = hits.first else {
            Issue.record("expected one hit")
            return
        }
        #expect(hit.path == "note.md")
        #expect(hit.display.contains("second needle here"))
        #expect(!hit.display.contains("\n"), "whitespace must be collapsed: \(hit.display)")
        #expect(!hit.display.contains("  "), "no double spaces: \(hit.display)")
    }

    @Test("search: multiple matches in one note yield one row (first match)")
    func oneRowPerNote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = String(repeating: "x", count: 60) + " needle one " + String(repeating: "x", count: 60)
            + " needle two " + String(repeating: "x", count: 60)
        try stage("note.md", "Title\n" + body, in: dir)

        let hits = try bikeshed.search(query: "needle").value
        #expect(hits.count == 1)
        #expect(hits.first?.display.contains("needle one") == true)
        #expect(hits.first?.display.contains("needle two") == false)
    }

    // MARK: - Scope and ordering

    @Test("search: frontmatter is searched (it is body, opaque to the core)")
    func frontmatterSearched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "---\ntag: secret-needle\n---\nbody", in: dir)

        let hits = try bikeshed.search(query: "secret").value
        #expect(hits.count == 1)
        #expect(hits.first?.display.contains("secret-needle") == true)
    }

    @Test("search: results are sorted by bikeshed path (case-insensitive)")
    func sortedByPath() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("z/z.md", "needle", in: dir)
        try stage("a/a.md", "needle", in: dir)
        try stage("M.md", "needle", in: dir)

        let hits = try bikeshed.search(query: "needle").value
        #expect(hits.map(\.path) == ["a/a.md", "M.md", "z/z.md"])
    }

    @Test("search: attachments and hidden files are not searched")
    func attachmentsNotSearched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("photo.txt", "needle", in: dir)
        try stage(".hidden.md", "needle", in: dir)

        #expect(try bikeshed.search(query: "needle").value.isEmpty)
    }

    @Test("search: no match yields an empty result")
    func noMatch() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "nothing here", in: dir)

        #expect(try bikeshed.search(query: "needle").value.isEmpty)
    }

    // MARK: - Query shape

    @Test("search: an empty or whitespace-only query yields no results")
    func emptyQuery() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "anything", in: dir)

        #expect(try bikeshed.search(query: "").value.isEmpty)
        #expect(try bikeshed.search(query: "   ").value.isEmpty)
        #expect(try bikeshed.search(query: "\n\t").value.isEmpty)
    }

    @Test("search: the query is trimmed")
    func queryTrimmed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "Title\nthe needle here", in: dir)

        let hits = try bikeshed.search(query: "  needle  ").value
        #expect(hits.count == 1)
    }

    @Test("search: a substring match is not word-boundary sensitive")
    func substringNotWordBoundary() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "Title\npineapple and needles", in: dir)

        #expect(try bikeshed.search(query: "eap").value.count == 1)    // pin-eap-ple
        #expect(try bikeshed.search(query: "eedl").value.count == 1)   // n-eedl-es
    }
}
