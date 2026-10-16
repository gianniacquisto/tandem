import CryptoKit
import Foundation
import Testing
@testable import TandemCore

/// Note operations (spec: "Bikeshed core — operations", "Vault protocol"):
/// create / read / write / list / delete — tree shape (folders first,
/// alphabetical), hidden files and attachments excluded, missing folder =
/// empty tree, name validation, existing target = error not overwrite,
/// write stores exactly the bytes given (frontmatter pass-through: the core
/// never parses note text).
///
/// Every test runs against a temp dir via `Bikeshed`'s explicit-directory
/// initialiser — no fixture vault, no real `~/.tandem`, no network.
@Suite("Bikeshed note operations")
struct BikeshedNoteOpsTests {

    /// A unique temp directory (not pre-created: a missing vault folder is a
    /// valid empty vault, and some tests rely on that), nested one level
    /// below a unique parent so the vault's sibling lockfile
    /// (`bikeshed.lock`, ADR 0004) is never shared between concurrent tests.
    /// Callers clean up with `defer` (removing the parent covers the lock).
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-ops-\(UUID().uuidString)")
            .appendingPathComponent("vault")
    }

    /// Write a raw file straight into the vault (bypassing the core) to stage
    /// pre-existing on-disk state: attachments, hidden files, name violators.
    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    /// One row of a tree's shape: the node's path and whether it is a note.
    private struct NodeShape: Equatable {
        let path: String
        let isNote: Bool
    }

    /// The ordered (path, kind) pairs of a tree, depth-first — the tree shape.
    private func shape(_ nodes: [VaultNode]) -> [NodeShape] {
        var out: [NodeShape] = []
        func walk(_ list: [VaultNode]) {
            for node in list {
                out.append(NodeShape(path: node.path, isNote: node.kind == .note))
                walk(node.children)
            }
        }
        walk(nodes)
        return out
    }

    private func sha256(_ text: String) -> String {
        let digest = SHA256.hash(data: Data(text.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: - list

    @Test("list on a missing vault folder is an empty tree, never an error")
    func listMissingFolderIsEmpty() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let result = try bikeshed.list()
        #expect(result.value.isEmpty)
        #expect(result.note == nil)
        #expect(!FileManager.default.fileExists(atPath: dir.path))
    }

    @Test("list: folders first, alphabetical within each level")
    func listTreeShape() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        for path in ["b.md", "A.md", "z/note.md", "z/alpha.md", "z/beta.md", "apple/x.md"] {
            _ = try bikeshed.create(path: path)
        }

        // Folders first (apple before z), then notes (A.md before b.md,
        // case-insensitive). Inside z: alpha, beta, note.
        let expected: [NodeShape] = [
            .init(path: "apple", isNote: false), .init(path: "apple/x.md", isNote: true),
            .init(path: "z", isNote: false), .init(path: "z/alpha.md", isNote: true),
            .init(path: "z/beta.md", isNote: true), .init(path: "z/note.md", isNote: true),
            .init(path: "A.md", isNote: true), .init(path: "b.md", isNote: true),
        ]
        #expect(shape(try bikeshed.list().value) == expected)
    }

    @Test("list excludes hidden files, hidden folders, and non-.md attachments")
    func listExcludesHiddenAndAttachments() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("ok.md", "visible", in: dir)
        try stage(".hidden.md", "hidden note", in: dir)
        try stage("photo.png", "not markdown", in: dir)
        try stage(".DS_Store", "junk", in: dir)
        try stage(".hiddendir/inside.md", "in a hidden folder", in: dir)
        try stage("sub/figure.svg", "attachment in a folder", in: dir)

        // Hidden entries and attachments are out; the folder holding an
        // attachment is still a folder (folders are part of the tree) and
        // folders sort first.
        let shape = shape(try bikeshed.list().value)
        #expect(shape.map(\.path) == ["sub", "ok.md"], "unexpected tree: \(shape)")
        #expect(shape[0].isNote == false && shape[1].isNote == true)
    }

    @Test("list treats .md case-insensitively (NOTE.MD is a note)")
    func listUppercaseExtensionIsANote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("NOTE.MD", "uppercase", in: dir)

        let shape = shape(try bikeshed.list().value)
        #expect(shape.map(\.path) == ["NOTE.MD"])
        #expect(shape.first?.isNote == true)
    }

    // MARK: - create

    @Test("create makes an empty note, creating parent folders")
    func createMakesEmptyNote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let result = try bikeshed.create(path: "Projects/My Note.md")
        #expect(result.value == "Projects/My Note.md")
        #expect(result.note == nil)

        let fileURL = dir.appendingPathComponent("Projects/My Note.md")
        let fm = FileManager.default
        var isDir: ObjCBool = false
        #expect(fm.fileExists(atPath: fileURL.path, isDirectory: &isDir) && !isDir.boolValue)
        #expect(try Data(contentsOf: fileURL).isEmpty, "a new note is empty")

        // The vault folder itself was created lazily.
        #expect(FileManager.default.fileExists(atPath: dir.path))
    }

    @Test("create allows spaces but rejects #, |, and malformed / paths")
    func createNameValidation() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // Spaces are allowed.
        _ = try bikeshed.create(path: "My Note.md")

        // # and | are the wikilink grammar's separators: rejected.
        for bad in ["bad#name.md", "bad|name.md", "folder/bad#name.md", "folder/bad|name.md"] {
            do {
                _ = try bikeshed.create(path: bad)
                Issue.record("Expected invalidName for \(bad)")
            } catch let error as BikeshedError {
                guard case .invalidName(let name) = error else {
                    Issue.record("Expected invalidName for \(bad), got \(error)")
                    return
                }
                #expect(name == bad)
            }
        }

        // Malformed / use: empty components, leading/trailing slash, dot
        // components (vault escape).
        for bad in ["a//b.md", "/absolute.md", "trailing/.md", "../escape.md", "a/../b.md", "./dot.md", ""] {
            #expect(throws: BikeshedError.invalidName(name: bad)) {
                _ = try bikeshed.create(path: bad)
            }
        }

        // Nothing but the valid note was created.
        #expect(shape(try bikeshed.list().value).map(\.path) == ["My Note.md"])
    }

    @Test("create requires a .md name (case-insensitive)")
    func createRequiresMarkdownExtension() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        #expect(throws: BikeshedError.invalidName(name: "notmarkdown")) {
            _ = try bikeshed.create(path: "notmarkdown")
        }
        #expect(throws: BikeshedError.invalidName(name: "image.png")) {
            _ = try bikeshed.create(path: "image.png")
        }
        // Uppercase .MD is a note name.
        _ = try bikeshed.create(path: "UPPER.MD")
        #expect(shape(try bikeshed.list().value).map(\.path) == ["UPPER.MD"])
    }

    @Test("create on an existing target is an error, not an overwrite")
    func createExistingTarget() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        _ = try bikeshed.write(path: "a.md", text: "original")

        do {
            _ = try bikeshed.create(path: "a.md")
            Issue.record("Expected alreadyExists")
        } catch let error as BikeshedError {
            guard case .alreadyExists(let path) = error else {
                Issue.record("Expected alreadyExists, got \(error)")
                return
            }
            #expect(path == "a.md")
        }

        // The original content is intact.
        #expect(try bikeshed.read(path: "a.md").value.text == "original")

        // The same rule inside a folder.
        _ = try bikeshed.create(path: "f/n.md")
        #expect(throws: BikeshedError.alreadyExists(path: "f/n.md")) {
            _ = try bikeshed.create(path: "f/n.md")
        }
        // And a path whose parent folder would have to be a file is refused.
        _ = try bikeshed.create(path: "f.md")
        #expect(throws: BikeshedError.alreadyExists(path: "f.md")) {
            _ = try bikeshed.create(path: "f.md/n.md")
        }
    }

    // MARK: - read

    @Test("read returns the note text, its on-disk hash, and its mtime")
    func readReturnsTextHashMtime() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let text = "# Heading\n\nbody text"
        _ = try bikeshed.write(path: "a.md", text: text)
        let before = Date()

        let note = try bikeshed.read(path: "a.md").value
        #expect(note.text == text)
        #expect(note.hash == sha256(text), "hash is the SHA-256 of the on-disk bytes")
        #expect(note.modifiedAt >= before.addingTimeInterval(-5))
        #expect(note.modifiedAt <= Date().addingTimeInterval(5))
    }

    @Test("read on a missing note throws unknownNote")
    func readMissingNote() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        do {
            _ = try bikeshed.read(path: "missing.md")
            Issue.record("Expected unknownNote")
        } catch let error as BikeshedError {
            guard case .unknownNote(let path) = error else {
                Issue.record("Expected unknownNote, got \(error)")
                return
            }
            #expect(path == "missing.md")
        }
    }

    // MARK: - write

    @Test("write stores exactly the bytes given (frontmatter pass-through)")
    func writeStoresExactBytes() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // Frontmatter + wikilinks + code: the core never parses any of it.
        let text = """
        ---
        tags: [x, y]
        link: [[Somewhere]]
        ---

        # Title
        See [[Somewhere#Section|here]] and `[[inline code is not a link]]`.
        """
        let record = try bikeshed.write(path: "fm.md", text: text).value
        #expect(record.path == "fm.md")
        #expect(record.hash == sha256(text))

        // Straight off disk, bypassing the core: byte-for-byte the same.
        let onDisk = try String(
            contentsOf: dir.appendingPathComponent("fm.md"), encoding: .utf8)
        #expect(onDisk == text, "write must store exactly the bytes given")
        #expect(try bikeshed.read(path: "fm.md").value.text == text)
    }

    @Test("write overwrites an existing note and records the new hash")
    func writeOverwrites() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let first = try bikeshed.write(path: "a.md", text: "one").value
        let second = try bikeshed.write(path: "a.md", text: "two").value
        #expect(first.hash != second.hash)
        #expect(second.hash == sha256("two"))
        #expect(try bikeshed.read(path: "a.md").value.text == "two")
    }

    // MARK: - delete

    @Test("delete removes a note, and a folder recursively")
    func deleteRemoves() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        _ = try bikeshed.create(path: "a.md")
        _ = try bikeshed.create(path: "f/inner.md")
        _ = try bikeshed.create(path: "keep.md")

        _ = try bikeshed.delete(path: "a.md")
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("a.md").path))

        _ = try bikeshed.delete(path: "f")
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("f").path))

        let shape = shape(try bikeshed.list().value)
        #expect(shape.map(\.path) == ["keep.md"])
    }

    @Test("delete on a missing path throws unknownNote")
    func deleteMissing() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        #expect(throws: BikeshedError.unknownNote(path: "nope.md")) {
            _ = try bikeshed.delete(path: "nope.md")
        }
    }

    // MARK: - Pre-existing violators

    @Test("pre-existing name violators are tolerated: listed and readable")
    func preExistingViolatorsTolerated() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        try stage("bad#name.md", "an agent wrote this", in: dir)
        try stage("bad|pipe.md", "another one", in: dir)

        // The tree shows them (the vault is what is on disk).
        let shape = shape(try bikeshed.list().value)
        #expect(Set(shape.map(\.path)) == Set(["bad#name.md", "bad|pipe.md"]))

        // They are editable through the core.
        #expect(try bikeshed.read(path: "bad#name.md").value.text == "an agent wrote this")
        _ = try bikeshed.write(path: "bad#name.md", text: "edited")
        #expect(try bikeshed.read(path: "bad#name.md").value.text == "edited")

        // But the app never *creates* such names (create/rename validation).
        #expect(throws: BikeshedError.invalidName(name: "bad#name.md")) {
            _ = try bikeshed.create(path: "bad#name.md")
        }
    }
}
