import Foundation
import Testing
@testable import TandemCore

/// Rename + link rewrite (ADR 0005): one locked transaction — rewrite every
/// wikilink that resolves to the renamed note (every form, including the
/// note's own self-links; re-canonicalised against the post-rename file set),
/// then a single `rename(2)`. Wikilinks only: markdown links, frontmatter,
/// embeds, code, and non-resolving links stay byte-identical. New path
/// already taken → refused, nothing written.
@Suite("Bikeshed rename + link rewrite")
struct BikeshedRenameTests {

    /// A unique temp dir, nested one level below a unique parent: the
    /// bikeshed's sibling lockfile (`bikeshed.lock`, ADR 0004) must not be
    /// shared between concurrent tests.
    private func tempDir() -> URL {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-rename-\(UUID().uuidString)")
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

    private func read(_ relativePath: String, in dir: URL) throws -> String {
        try String(contentsOf: dir.appendingPathComponent(relativePath), encoding: .utf8)
    }

    private func exists(_ relativePath: String, in dir: URL) -> Bool {
        var isDir: ObjCBool = false
        return FileManager.default.fileExists(
            atPath: dir.appendingPathComponent(relativePath).path, isDirectory: &isDir)
    }

    // MARK: - Basic rename

    @Test("rename moves the note and rewrites bare links to it")
    func renameBareLink() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        let result = try bikeshed.rename(from: "B.md", to: "C.md")
        #expect(result.value == "C.md")

        #expect(!exists("B.md", in: dir))
        #expect(exists("C.md", in: dir))
        #expect(try read("C.md", in: dir) == "bee")
        #expect(try read("A.md", in: dir) == "see [[C]]")
    }

    @Test("rename returns the new path and the tree reflects the move")
    func renameTreeShape() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("projects/B.md", "bee", in: dir)
        try stage("A.md", "x", in: dir)

        _ = try bikeshed.rename(from: "projects/B.md", to: "C.md")

        #expect(try read("C.md", in: dir) == "bee")
        // The emptied folder stays in the tree (folders first, even empty).
        let tree = try bikeshed.list().value
        #expect(tree.map(\.name) == ["projects", "A.md", "C.md"])
        #expect(tree[0].kind == .folder)
        #expect(tree[0].children.isEmpty)
    }

    // MARK: - Every wikilink form

    @Test("rename rewrites every form: bare, alias, fragment, both, reversed order")
    func renameAllForms() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        let original = """
        1 [[B]]
        2 [[B|An Alias]]
        3 [[B#Heading]]
        4 [[B#Heading|Alias]]
        5 [[B|Alias#Heading]]
        """
        try stage("A.md", original, in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        let rewritten = try read("A.md", in: dir)
        #expect(rewritten == original
            .replacing("1 [[B]]", with: "1 [[C]]")
            .replacing("2 [[B|An Alias]]", with: "2 [[C|An Alias]]")
            .replacing("3 [[B#Heading]]", with: "3 [[C#Heading]]")
            .replacing("4 [[B#Heading|Alias]]", with: "4 [[C#Heading|Alias]]")
            .replacing("5 [[B|Alias#Heading]]", with: "5 [[C|Alias#Heading]]"))
    }

    @Test("rename rewrites qualified and .md-qualified links")
    func renameQualified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("projects/plan.md", "p", in: dir)
        try stage("A.md", "a [[projects/plan]] b [[projects/plan.md]]", in: dir)

        _ = try bikeshed.rename(from: "projects/plan.md", to: "plan.md")

        #expect(try read("A.md", in: dir) == "a [[plan]] b [[plan]]")
    }

    @Test("rename is case-insensitive: [[b]] to B.md is rewritten")
    func renameCaseInsensitiveTarget() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[b]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "see [[C]]")
    }

    @Test("rename includes the note's own self-links")
    func renameSelfLink() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "note to [[B]] and [[B|self]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("C.md", in: dir) == "note to [[C]] and [[C|self]]")
    }

    // MARK: - Re-canonicalisation

    @Test("rename into a taken basename rewrites to the qualified form")
    func renameAmbiguousBecomesQualified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("x.md", "x", in: dir)
        try stage("y.md", "y", in: dir)
        try stage("A.md", "root [[x]] sub [[y]]", in: dir)

        // After the move two notes are named y (y.md and a/y.md), so the
        // rewrite of [[x]] must use the qualified path; [[y]] still names
        // y.md and is untouched.
        _ = try bikeshed.rename(from: "x.md", to: "a/y.md")

        #expect(try read("A.md", in: dir) == "root [[a/y]] sub [[y]]")
    }

    @Test("rename to a unique basename rewrites to the bare form")
    func renameUniqueStaysBare() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "a x", in: dir)
        try stage("A.md", "see [[a/x]]", in: dir)

        _ = try bikeshed.rename(from: "a/x.md", to: "x.md")

        #expect(try read("A.md", in: dir) == "see [[x]]")
    }

    @Test("a bare link broken by duplicates is not rewritten (it resolved to nothing)")
    func renameBrokenBareUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("x.md", "root x", in: dir)
        try stage("b/x.md", "b x", in: dir)
        try stage("A.md", "broken [[x]]", in: dir)

        _ = try bikeshed.rename(from: "x.md", to: "y.md")

        // [[x]] never resolved to x.md, so it is untouched — and it now
        // resolves to b/x.md on its own (derived state, no rewrite).
        #expect(try read("A.md", in: dir) == "broken [[x]]")
        guard case .resolved(let path) = bikeshed.resolve("x") else {
            Issue.record("[[x]] should now resolve to b/x.md")
            return
        }
        #expect(path == "b/x.md")
    }

    // MARK: - Wikilinks only

    @Test("rename leaves markdown links byte-identical")
    func renameMarkdownLinkUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "[text](B.md) and [[B]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "[text](B.md) and [[C]]")
    }

    @Test("rename leaves wikilinks in leading frontmatter byte-identical")
    func renameFrontmatterUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        let original = """
        ---
        links: [[B]]
        ---
        body [[B]]
        """
        try stage("A.md", original, in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == """
        ---
        links: [[B]]
        ---
        body [[C]]
        """)
    }

    @Test("an unterminated leading --- block is not frontmatter: its links are rewritten")
    func renameUnterminatedFrontmatterRewritten() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "---\nlinks: [[B]]\nno closing fence", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "---\nlinks: [[C]]\nno closing fence")
    }

    @Test("rename leaves embeds byte-identical")
    func renameEmbedUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "![[B]] and [[B]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "![[B]] and [[C]]")
    }

    @Test("rename leaves non-resolving links byte-identical")
    func renameBrokenLinkUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "[[Missing]] and [[B]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "[[Missing]] and [[C]]")
    }

    @Test("rename leaves wikilinks in code blocks byte-identical")
    func renameCodeBlockUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "```\n[[B]]\n```\nreal [[B]]", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        #expect(try read("A.md", in: dir) == "```\n[[B]]\n```\nreal [[C]]")
    }

    // MARK: - Refusals

    @Test("rename to a taken path is refused and nothing is written")
    func renameTakenRefused() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("C.md", "cee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        do {
            _ = try bikeshed.rename(from: "B.md", to: "C.md")
            Issue.record("Expected alreadyExists")
        } catch let error as BikeshedError {
            guard case .alreadyExists(let path) = error else {
                Issue.record("Expected alreadyExists, got \(error)")
                return
            }
            #expect(path == "C.md")
        }

        // Nothing written: the note is where it was, references unchanged.
        #expect(exists("B.md", in: dir))
        #expect(exists("C.md", in: dir))
        #expect(try read("C.md", in: dir) == "cee")
        #expect(try read("A.md", in: dir) == "see [[B]]")
    }

    @Test("rename of a missing note is unknownNote")
    func renameMissingOld() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)

        #expect(throws: BikeshedError.unknownNote(path: "Nope.md")) {
            _ = try bikeshed.rename(from: "Nope.md", to: "C.md")
        }
        #expect(exists("B.md", in: dir))
    }

    @Test("rename to an invalid new name is refused before anything is written")
    func renameInvalidNewName() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        for bad in ["bad#name.md", "bad|name.md", "noext", "../escape.md", ".hidden.md"] {
            #expect(throws: BikeshedError.invalidName(name: bad)) {
                _ = try bikeshed.rename(from: "B.md", to: bad)
            }
        }
        #expect(exists("B.md", in: dir))
        #expect(try read("A.md", in: dir) == "see [[B]]")
    }

    @Test("rename into a new folder creates the parent directories")
    func renameCreatesParents() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)

        _ = try bikeshed.rename(from: "B.md", to: "deep/nested/C.md")

        #expect(try read("deep/nested/C.md", in: dir) == "bee")
        #expect(!exists("B.md", in: dir))
    }
}
