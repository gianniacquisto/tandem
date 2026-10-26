import Foundation
import Testing
@testable import TandemCore

/// The hidden layer (ticket 134E56A8): the capture saddlebag
/// (`projects/<project>/.raw/`) and the consolidation archive
/// (`projects/<project>/.raw/.done/`) are pruned from the tree, the search,
/// the backlinks, and the wikilink resolution.
///
/// The reveal flag (`Bikeshed.showsHiddenFiles`) changes what the tree
/// *draws* — never what the curated graph *is*: the linkable note set is the
/// curated layer in both states.
@Suite("Bikeshed — the hidden layer")
struct BikeshedHiddenLayerTests {

    /// The pending capture's path — inside the saddlebag.
    static let capture = "projects/tandem/.raw/ses_1/20261021T100000Z-work-a-capture.md"
    /// The archived capture's path — inside the consolidation archive.
    static let archived = "projects/tandem/.raw/.done/ses_1/20261020T100000Z-work-archived.md"
    /// The curated note's path — the graph.
    static let curated = "projects/tandem/App.md"

    private func tempDir() -> URL {
        /// Nested one level below a unique parent so the bikeshed's sibling
        /// lockfile (`bikeshed.lock`) is never shared between concurrent tests.
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-hidden-\(UUID().uuidString)")
        let dir = parent.appendingPathComponent("bikeshed")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// A bikeshed with one project, one curated note, one pending capture, and
    /// one archived capture — the shape the memory layer has on disk.
    private func stage(_ dir: URL) throws {
        try write(Self.curated, "# App\ncurated", in: dir)
        try write(Self.capture, "a capture", in: dir)
        try write(Self.archived, "an archived capture", in: dir)
    }

    private func write(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    /// The node at `path` in a tree, or nil.
    private func find(_ nodes: [BikeshedNode], at path: String) -> BikeshedNode? {
        for child in nodes {
            if child.path == path { return child }
            if let hit = find(child.children, at: path) { return hit }
        }
        return nil
    }

    /// Every note path in a tree, depth-first.
    private func notePaths(_ nodes: [BikeshedNode]) -> [String] {
        nodes.flatMap { $0.kind == .note ? [$0.path] : notePaths($0.children) }
    }

    // MARK: - The tree

    @Test("the tree hides the hidden layer by default")
    func hiddenByDefault() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)

        let tree = try Bikeshed(directory: dir).list().value
        #expect(find(tree, at: "projects/tandem/.raw") == nil)
        #expect(notePaths(tree) == [Self.curated])
    }

    @Test("the reveal flag draws the saddlebag, its sessions, and the archive")
    func revealed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)

        let tree = try Bikeshed(directory: dir, showsHiddenFiles: true).list().value
        #expect(find(tree, at: "projects/tandem/.raw") != nil)
        #expect(find(tree, at: "projects/tandem/.raw/ses_1") != nil)
        #expect(find(tree, at: "projects/tandem/.raw/.done") != nil)
        #expect(find(tree, at: "projects/tandem/.raw/.done/ses_1") != nil)
        #expect(Set(notePaths(tree)) == Set([Self.curated, Self.capture, Self.archived]))
    }

    @Test("the reveal flag is a view: the linkable note set is the curated layer in both states")
    func linkableSetUnchanged() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)

        #expect(Bikeshed(directory: dir).notePaths() == [Self.curated])
        #expect(Bikeshed(directory: dir, showsHiddenFiles: true).notePaths() == [Self.curated])
    }

    // MARK: - Search, backlinks, wikilinks (the curated graph, both states)

    @Test("search ignores the hidden layer in both states")
    func searchIgnoresHidden() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)

        #expect(try Bikeshed(directory: dir).search(query: "capture").value.isEmpty)
        #expect(try Bikeshed(directory: dir, showsHiddenFiles: true)
            .search(query: "capture").value.isEmpty)
    }

    @Test("wikilink resolution ignores the hidden layer in both states")
    func resolveIgnoresHidden() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)

        for bikeshed in [Bikeshed(directory: dir),
                         Bikeshed(directory: dir, showsHiddenFiles: true)] {
            // Bare (the basename) and qualified (the full path) both stay broken.
            #expect(bikeshed.resolve("20261021T100000Z-work-a-capture") == .broken)
            #expect(bikeshed.resolve("projects/tandem/.raw/ses_1/20261021T100000Z-work-a-capture")
                    == .broken)
            #expect(bikeshed.resolve("App") == .resolved(path: Self.curated))
        }
    }

    @Test("backlinks ignore the hidden layer in both states")
    func backlinksIgnoreHidden() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)
        // A capture that links to the curated note: the capture is not a link
        // source, so the curated note gains no backlink from it.
        try write(Self.capture, "a capture linking [[App]]", in: dir)

        for bikeshed in [Bikeshed(directory: dir),
                         Bikeshed(directory: dir, showsHiddenFiles: true)] {
            #expect(try bikeshed.backlinks(path: Self.curated).value.isEmpty)
        }
    }

    @Test("a curated note's link to a capture is broken, in both states")
    func curatedLinkToCaptureIsBroken() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)
        try write(Self.curated, "# App\nsee [[20261021T100000Z-work-a-capture]]", in: dir)

        for bikeshed in [Bikeshed(directory: dir),
                         Bikeshed(directory: dir, showsHiddenFiles: true)] {
            #expect(bikeshed.resolve("20261021T100000Z-work-a-capture") == .broken)
            #expect(try bikeshed.backlinks(path: Self.capture).value.isEmpty)
        }
    }

    // MARK: - The revealed layer is a normal place to work

    @Test("a revealed capture is readable and writable")
    func revealedCaptureIsReadWrite() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)
        let bikeshed = Bikeshed(directory: dir, showsHiddenFiles: true)

        let content = try bikeshed.read(path: Self.capture).value
        #expect(content.text == "a capture")

        _ = try bikeshed.write(path: Self.capture, text: "edited capture").value
        #expect(try bikeshed.read(path: Self.capture).value.text == "edited capture")
    }

    @Test("deleting a revealed capture removes the file")
    func deleteRevealedCapture() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)
        let bikeshed = Bikeshed(directory: dir, showsHiddenFiles: true)

        _ = try bikeshed.delete(path: Self.capture).value
        #expect(!FileManager.default.fileExists(
            atPath: dir.appendingPathComponent(Self.capture).path))
        #expect(FileManager.default.fileExists(
            atPath: dir.appendingPathComponent(Self.archived).path))
    }

    @Test("creating inside a hidden folder is refused while it is hidden, allowed once revealed")
    func createFollowsTheFlag() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stage(dir)
        let target = "projects/tandem/.raw/ses_1/new.md"

        #expect(throws: BikeshedError.invalidName(name: target)) {
            try Bikeshed(directory: dir).create(path: target).value
        }
        _ = try Bikeshed(directory: dir, showsHiddenFiles: true).create(path: target).value
        #expect(FileManager.default.fileExists(atPath: dir.appendingPathComponent(target).path))
    }

    @Test("a capture's own write path stays valid: the flag does not change the capture layout")
    func captureStillWritesItsLayer() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir, showsHiddenFiles: true)

        let path = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "a capture", session: "ses_1").value
        #expect(path.hasPrefix("projects/tandem/.raw/ses_1/"))
        #expect(FileManager.default.fileExists(atPath: dir.appendingPathComponent(path).path))
    }
}
