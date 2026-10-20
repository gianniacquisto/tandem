import Foundation
import Testing
@testable import TandemCore

/// Move (the tree's drag-to-re-parent): the same machinery as `rename`
/// (ADR 0005), in one locked transaction — every wikilink that resolves to a
/// moved note is rewritten to its new canonical target, then the note (or the
/// folder, with everything under it) moves. A folder moved into its own
/// subtree and a name already taken in the destination are refused, with
/// nothing written.
@Suite("Bikeshed move")
struct BikeshedMoveTests {

    /// A unique temp dir, nested one level below a unique parent: the
    /// bikeshed's sibling lockfile (`bikeshed.lock`, ADR 0004) must not be
    /// shared between concurrent tests.
    private func tempDir() -> URL {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-move-\(UUID().uuidString)")
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

    // MARK: - Note move

    @Test("move takes a note into a folder and rewrites the links to it")
    func moveNote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        try stage("links.md", "see [[x]]", in: dir)

        let result = try bikeshed.move(path: "a/x.md", into: "b").value

        #expect(result.path == "b/x.md")
        #expect(result.oldPath == "a/x.md")
        #expect(result.isFolder == false)
        #expect(result.movedNotes.map(\.newPath) == ["b/x.md"])
        #expect(!exists("a/x.md", in: dir))
        #expect(exists("b/x.md", in: dir))
        #expect(try read("b/x.md", in: dir) == "the note")
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    @Test("move rewrites every wikilink form: bare, qualified, alias, fragment")
    func moveNoteAllForms() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        let original = """
        1 [[x]]
        2 [[x|An Alias]]
        3 [[x#Heading]]
        4 [[x#Heading|Alias]]
        5 [[a/x]]
        """
        try stage("links.md", original, in: dir)

        _ = try bikeshed.move(path: "a/x.md", into: "b").value

        // The name is unchanged and still unique, so every link is
        // re-canonicalised to the bare form — including the qualified one.
        let rewritten = try read("links.md", in: dir)
        #expect(rewritten == """
        1 [[x]]
        2 [[x|An Alias]]
        3 [[x#Heading]]
        4 [[x#Heading|Alias]]
        5 [[x]]
        """)
    }

    @Test("move to the bikeshed root")
    func moveToRoot() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("links.md", "see [[x]]", in: dir)

        let result = try bikeshed.move(path: "a/x.md", into: "").value

        #expect(result.path == "x.md")
        #expect(exists("x.md", in: dir))
        #expect(!exists("a/x.md", in: dir))
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    @Test("move rewrites the moved note's own self-links")
    func moveSelfLink() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "note to [[x]] and [[x|self]]", in: dir)
        try stage("b/keep.md", "k", in: dir)

        _ = try bikeshed.move(path: "a/x.md", into: "b").value

        #expect(try read("b/x.md", in: dir) == "note to [[x]] and [[x|self]]")
    }

    @Test("move into a folder that leaves the basename ambiguous rewrites to the qualified form")
    func moveAmbiguousBecomesQualified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the other note", in: dir)
        try stage("z/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        try stage("links.md", "see [[z/x]]", in: dir)

        _ = try bikeshed.move(path: "z/x.md", into: "b").value

        // Two notes are now named x (a/x.md and b/x.md): the link must name
        // the path, or it resolves to nothing.
        #expect(try read("links.md", in: dir) == "see [[b/x]]")
        #expect(try read("b/x.md", in: dir) == "the note")
    }

    @Test("move leaves markdown links, embeds and code byte-identical")
    func moveWikilinksOnly() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        let original = """
        [text](a/x.md) and [[x]]
        ![[x]]
        ```
        [[x]]
        ```
        """
        try stage("links.md", original, in: dir)

        _ = try bikeshed.move(path: "a/x.md", into: "b").value

        #expect(try read("links.md", in: dir) == original)
    }

    @Test("moving a note into the folder it is already in changes nothing")
    func moveNoOp() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("links.md", "see [[x]]", in: dir)

        let result = try bikeshed.move(path: "a/x.md", into: "a").value

        #expect(result.path == "a/x.md")
        #expect(result.movedNotes.isEmpty)
        #expect(try read("a/x.md", in: dir) == "the note")
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    // MARK: - Folder move

    @Test("move takes a folder and its notes into another folder")
    func moveFolder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("a/deep/y.md", "the deep note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        try stage("links.md", "see [[x]] and [[y]]", in: dir)

        let result = try bikeshed.move(path: "a", into: "b").value

        #expect(result.path == "b/a")
        #expect(result.isFolder)
        #expect(Set(result.movedNotes.map(\.newPath)) == ["b/a/x.md", "b/a/deep/y.md"])
        #expect(!exists("a", in: dir))
        #expect(exists("b/a/x.md", in: dir))
        #expect(exists("b/a/deep/y.md", in: dir))
        #expect(try read("b/a/x.md", in: dir) == "the note")
        #expect(try read("b/a/deep/y.md", in: dir) == "the deep note")
        // Both links still resolve — the notes they name moved with the folder.
        #expect(try read("links.md", in: dir) == "see [[x]] and [[y]]")
    }

    @Test("a folder move rewrites qualified links to the notes inside it")
    func moveFolderQualifiedLinks() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        try stage("links.md", "see [[a/x]]", in: dir)

        _ = try bikeshed.move(path: "a", into: "b").value

        // The note's name is unchanged and still unique, so the link is
        // re-canonicalised to the bare form.
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    @Test("a folder move rewrites the links between the notes it carries")
    func moveFolderInnerLinks() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "see [[y]]", in: dir)
        try stage("a/y.md", "the other note", in: dir)
        try stage("b/keep.md", "k", in: dir)

        _ = try bikeshed.move(path: "a", into: "b").value

        #expect(try read("b/a/x.md", in: dir) == "see [[y]]")
    }

    @Test("a folder move into a folder holding the same basename rewrites to the qualified form")
    func moveFolderAmbiguousBecomesQualified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/x.md", "the other note", in: dir)
        try stage("b/keep.md", "k", in: dir)
        try stage("links.md", "see [[a/x]]", in: dir)

        _ = try bikeshed.move(path: "a", into: "b").value

        // Two notes named x now: the link must name the path.
        #expect(try read("links.md", in: dir) == "see [[b/a/x]]")
    }

    @Test("a folder move takes its attachments with it")
    func moveFolderCarriesAttachments() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("a/pic.png", "not really a png", in: dir)
        try stage("b/keep.md", "k", in: dir)

        _ = try bikeshed.move(path: "a", into: "b").value

        #expect(exists("b/a/pic.png", in: dir))
        #expect(!exists("a", in: dir))
    }

    @Test("the tree reflects a folder move")
    func moveFolderTreeShape() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/keep.md", "k", in: dir)

        _ = try bikeshed.move(path: "a", into: "b").value

        let tree = try bikeshed.list().value
        #expect(tree.map(\.name) == ["b"])
        #expect(tree[0].children.map(\.name) == ["a", "keep.md"])
    }

    // MARK: - Refusals

    @Test("moving a folder into its own descendant is refused as a cycle")
    func moveFolderCycle() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("a/deep/y.md", "the deep note", in: dir)
        try stage("links.md", "see [[x]]", in: dir)

        #expect(throws: BikeshedError.moveCycle(path: "a", into: "a/deep")) {
            _ = try bikeshed.move(path: "a", into: "a/deep").value
        }
        // Nothing written: the folder is where it was, links unchanged.
        #expect(exists("a/x.md", in: dir))
        #expect(exists("a/deep/y.md", in: dir))
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    @Test("moving a folder into itself is refused as a cycle")
    func moveFolderIntoItself() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)

        #expect(throws: BikeshedError.moveCycle(path: "a", into: "a")) {
            _ = try bikeshed.move(path: "a", into: "a").value
        }
        #expect(exists("a/x.md", in: dir))
    }

    @Test("moving a note onto a taken name is refused and nothing is written")
    func moveNoteCollision() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/x.md", "the other note", in: dir)
        try stage("links.md", "see [[a/x]]", in: dir)

        #expect(throws: BikeshedError.alreadyExists(path: "b/x.md")) {
            _ = try bikeshed.move(path: "a/x.md", into: "b").value
        }
        #expect(exists("a/x.md", in: dir))
        #expect(try read("b/x.md", in: dir) == "the other note")
        #expect(try read("links.md", in: dir) == "see [[a/x]]")
    }

    @Test("moving a folder onto a taken name is refused and nothing is written")
    func moveFolderCollision() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)
        try stage("b/a/y.md", "the other note", in: dir)
        try stage("links.md", "see [[x]]", in: dir)

        #expect(throws: BikeshedError.alreadyExists(path: "b/a")) {
            _ = try bikeshed.move(path: "a", into: "b").value
        }
        #expect(exists("a/x.md", in: dir))
        #expect(exists("b/a/y.md", in: dir))
        #expect(try read("links.md", in: dir) == "see [[x]]")
    }

    @Test("moving a missing note is unknownNote")
    func moveMissingSource() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("b/keep.md", "k", in: dir)

        #expect(throws: BikeshedError.unknownNote(path: "Nope.md")) {
            _ = try bikeshed.move(path: "Nope.md", into: "b").value
        }
    }

    @Test("moving into a folder that does not exist is unknownNote")
    func moveMissingDestination() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)

        #expect(throws: BikeshedError.unknownNote(path: "nope")) {
            _ = try bikeshed.move(path: "a/x.md", into: "nope").value
        }
        #expect(exists("a/x.md", in: dir))
    }

    @Test("a bikeshed escape in the destination is refused")
    func moveEscapeDestination() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "the note", in: dir)

        #expect(throws: BikeshedError.invalidName(name: "../escape")) {
            _ = try bikeshed.move(path: "a/x.md", into: "../escape").value
        }
        #expect(exists("a/x.md", in: dir))
    }
}
