import CryptoKit
import Foundation
import Testing
@testable import TandemCore

/// Conflict detection (spec: "Conflict detection"): `read`'s hash reflects
/// the on-disk state, and the model compares the buffer hash against the
/// disk hash (clean → adopt the external change silently, dirty → withhold
/// and show the banner). The banner flow itself is GUI and manual; this
/// suite pins the core half of the contract: a stable content hash on
/// `read`, a `WriteRecord` hash the watcher can match the on-disk hash
/// against to dedup self-writes, and a hash that moves when the file
/// changes underneath.
@Suite("Bikeshed conflict detection")
struct BikeshedConflictTests {

    private func tempDir() -> URL {
        /// Nested one level below a unique parent so the vault's sibling
        /// lockfile (`bikeshed.lock`) is never shared between concurrent tests.
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-conflict-\(UUID().uuidString)")
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

    private func sha256(_ text: String) -> String {
        SHA256.hash(data: Data(text.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    @Test("read's hash is the SHA-256 of the on-disk bytes, stable across reads")
    func hashIsStableSha256() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "hello bikeshed", in: dir)

        let first = try bikeshed.read(path: "note.md").value
        let second = try bikeshed.read(path: "note.md").value
        #expect(first.hash == sha256("hello bikeshed"))
        #expect(first.hash == second.hash)
        #expect(first.text == "hello bikeshed")
    }

    @Test("write returns a hash equal to the on-disk read (self-write dedup)")
    func writeRecordMatchesDisk() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let record = try bikeshed.write(path: "note.md", text: "saved bytes").value
        let onDisk = try bikeshed.read(path: "note.md").value

        #expect(record.path == "note.md")
        #expect(record.hash == onDisk.hash)
        #expect(record.hash == sha256("saved bytes"))
    }

    @Test("an external change moves the hash — the clean/dirty comparison input")
    func externalChangeMovesHash() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("note.md", "original", in: dir)

        let before = try bikeshed.read(path: "note.md").value

        // An agent/editor writes the file directly (no lock, no API).
        try "external edit".write(
            to: dir.appendingPathComponent("note.md"), atomically: true, encoding: .utf8)

        let after = try bikeshed.read(path: "note.md").value
        #expect(after.hash != before.hash, "hash must reflect the external change")
        #expect(after.hash == sha256("external edit"))
        #expect(after.text == "external edit")
    }

    @Test("the hash is content-based: same bytes, same hash, any path")
    func hashContentBased() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a.md", "same content", in: dir)
        try stage("b.md", "same content", in: dir)

        let ha = try bikeshed.read(path: "a.md").value.hash
        let hb = try bikeshed.read(path: "b.md").value.hash
        #expect(ha == hb)
    }

    @Test("a created (empty) note hashes the empty byte string")
    func emptyNoteHash() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        _ = try bikeshed.create(path: "note.md")

        let content = try bikeshed.read(path: "note.md").value
        #expect(content.hash == sha256(""))
        #expect(content.text.isEmpty)
    }

    @Test("read's mtime tracks the on-disk modification time")
    func mtimeTracksDisk() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        _ = try bikeshed.write(path: "note.md", text: "one")

        let first = try bikeshed.read(path: "note.md").value
        // Age the file by 5 minutes, then rewrite it: the mtime must move.
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-300)],
            ofItemAtPath: dir.appendingPathComponent("note.md").path)
        _ = try bikeshed.write(path: "note.md", text: "two")
        let second = try bikeshed.read(path: "note.md").value

        #expect(second.modifiedAt > first.modifiedAt)
        #expect(second.modifiedAt > Date().addingTimeInterval(-30))
    }
}
