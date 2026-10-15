import Foundation
import Testing
@testable import TandemCore

/// Crash recovery (ADR 0004): simulate a mid-sequence crash — the writer
/// killed between the link-rewrite pass and the file rename (raw-staged: the
/// references already point at the new path, the note still has the old
/// name). The invariants: no torn files, no data loss, at most broken links;
/// and re-running the rename self-heals (the rewritten references already
/// resolve to the new path, so the re-run moves the file and rewrites
/// nothing). A kill can also leave an orphaned temp from an unfinished
/// atomic write; the next locked operation's cleanup pass removes temps
/// older than 60 s.
@Suite("Bikeshed crash recovery")
struct BikeshedCrashRecoveryTests {

    /// A unique temp dir, nested one level below a unique parent: the
    /// vault's sibling lockfile (`bikeshed.lock`, ADR 0004) must not be
    /// shared between concurrent tests.
    private func tempDir() -> URL {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-crash-\(UUID().uuidString)")
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

    private func read(_ relativePath: String, in dir: URL) throws -> String {
        try String(contentsOf: dir.appendingPathComponent(relativePath), encoding: .utf8)
    }

    private func exists(_ relativePath: String, in dir: URL) -> Bool {
        FileManager.default.fileExists(
            atPath: dir.appendingPathComponent(relativePath).path)
    }

    /// The state a SIGKILL between the rewrite pass and the file rename
    /// leaves on disk: A.md already references C.md; B.md is not moved yet.
    private func stageCrashedRename(in dir: URL) throws {
        try stage("A.md", "see [[C]]", in: dir)
        try stage("B.md", "bee", in: dir)
    }

    @Test("crashed rename state: no torn files, no data loss, at most broken links")
    func crashedStateIsSound() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stageCrashedRename(in: dir)
        let bikeshed = Bikeshed(directory: dir)

        // No torn content: every on-disk note decodes, no half-written temp
        // survived a completed atomic write.
        let entries = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        for name in entries where name.hasSuffix(".md") {
            let data = try Data(contentsOf: dir.appendingPathComponent(name))
            #expect(String(data: data, encoding: .utf8) != nil, "\(name) should be valid UTF-8")
        }
        #expect(!entries.contains { $0.hasPrefix(".note-") && $0.hasSuffix(".tmp") })
        // The content is intact.
        #expect(try read("B.md", in: dir) == "bee")
        #expect(try read("A.md", in: dir) == "see [[C]]")

        // At most broken links: [[C]] does not resolve yet; [[B]] still does.
        #expect(bikeshed.resolve("C") == .broken)
        guard case .resolved(let path) = bikeshed.resolve("B") else {
            Issue.record("[[B]] should still resolve to B.md")
            return
        }
        #expect(path == "B.md")
        #expect(throws: BikeshedError.unknownNote(path: "C.md")) {
            _ = try bikeshed.backlinks(path: "C.md")
        }
    }

    @Test("re-running the crashed rename self-heals the vault")
    func reRunSelfHeals() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try stageCrashedRename(in: dir)
        let bikeshed = Bikeshed(directory: dir)

        _ = try bikeshed.rename(from: "B.md", to: "C.md")

        // The file moved; the already-rewritten reference now resolves —
        // and was left untouched by the re-run (it never resolved to B.md).
        #expect(try read("C.md", in: dir) == "bee")
        #expect(!exists("B.md", in: dir))
        #expect(try read("A.md", in: dir) == "see [[C]]")
        guard case .resolved(let path) = bikeshed.resolve("C") else {
            Issue.record("[[C]] should resolve after the self-heal")
            return
        }
        #expect(path == "C.md")
        #expect(try bikeshed.backlinks(path: "C.md").value
            == [Backlink(path: "A.md", line: "see [[C]]")])
    }

    @Test("an orphaned temp older than 60 s from a crash is cleaned by the next locked op")
    func orphanTempCleaned() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("B.md", "bee", in: dir)
        try stage("A.md", "see [[B]]", in: dir)

        // A kill between the temp write and the rename(2) leaves a
        // same-folder temp; age it past the 60 s orphan threshold.
        let orphan = dir.appendingPathComponent(".note-crash-0000-0000-0000-000000000001.tmp")
        try "partial bytes".write(to: orphan, atomically: false, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-120)], ofItemAtPath: orphan.path)
        #expect(exists(".note-crash-0000-0000-0000-000000000001.tmp", in: dir))

        _ = try bikeshed.write(path: "A.md", text: "see [[B]] still")

        // The orphan is gone and the write landed intact.
        #expect(!exists(".note-crash-0000-0000-0000-000000000001.tmp", in: dir))
        #expect(try read("A.md", in: dir) == "see [[B]] still")
    }
}
