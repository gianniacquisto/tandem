import Darwin
import Foundation
import Testing
@testable import TandemCore

/// The shared storage primitive (ADR 0007): the sidecar `flock` + atomic-write
/// + orphan-sweep mechanics that both `Store` and `Bikeshed` build on. Tested
/// once directly here, rather than only indirectly through the two domains.
@Suite("LockedDirectory primitive")
struct LockedDirectoryTests {

    private func tempDir() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sidecar-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func storage(at dir: URL, recursive: Bool = false) -> LockedDirectory {
        LockedDirectory(
            lockURL: dir.appendingPathComponent("sidecar.lock"),
            tempPrefix: ".side-",
            cleanRoot: dir,
            cleanRecursively: recursive)
    }

    // MARK: - Atomic write

    @Test("atomic write replaces the file and leaves no temp behind")
    func atomicWriteReplacesAndCleans() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = storage(at: dir)
        let file = dir.appendingPathComponent("data.txt")

        try store.atomicWrite(Data("one".utf8), to: file)
        #expect(try String(contentsOf: file, encoding: .utf8) == "one")
        try store.atomicWrite(Data("two".utf8), to: file)
        #expect(try String(contentsOf: file, encoding: .utf8) == "two")

        let leftovers = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix(".side-") }
        #expect(leftovers.isEmpty, "temp files left behind: \(leftovers)")
    }

    @Test("atomic write creates missing parent folders")
    func atomicWriteCreatesParents() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = storage(at: dir)
        let file = dir.appendingPathComponent("a/b/c.txt")

        try store.atomicWrite(Data("deep".utf8), to: file)

        #expect(try String(contentsOf: file, encoding: .utf8) == "deep")
    }

    // MARK: - Orphan sweep

    @Test("orphan sweep removes stale temps and keeps fresh ones (flat)")
    func orphanSweepFlat() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = storage(at: dir)
        let fm = FileManager.default

        let stale = dir.appendingPathComponent(".side-\(UUID().uuidString).tmp")
        try "stale".write(to: stale, atomically: false, encoding: .utf8)
        try fm.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-120)], ofItemAtPath: stale.path)
        let fresh = dir.appendingPathComponent(".side-\(UUID().uuidString).tmp")
        try "fresh".write(to: fresh, atomically: false, encoding: .utf8)

        try store.withLock {}

        #expect(!fm.fileExists(atPath: stale.path), "stale orphan was not removed")
        #expect(fm.fileExists(atPath: fresh.path), "fresh temp was wrongly removed")
    }

    @Test("orphan sweep reaches subfolders when recursive, and stays out of them when not")
    func orphanSweepRecursive() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let fm = FileManager.default

        let nested = dir.appendingPathComponent("deep/.side-\(UUID().uuidString).tmp")
        try fm.createDirectory(
            at: nested.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "stale".write(to: nested, atomically: false, encoding: .utf8)
        try fm.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-120)], ofItemAtPath: nested.path)

        // Flat sweep must not touch the subfolder's temp.
        try storage(at: dir, recursive: false).withLock {}
        #expect(fm.fileExists(atPath: nested.path), "flat sweep wrongly descended into a subfolder")

        // Recursive sweep does.
        try storage(at: dir, recursive: true).withLock {}
        #expect(!fm.fileExists(atPath: nested.path), "recursive sweep missed a nested orphan")
    }

    // MARK: - Locking

    @Test("withLock holds the exclusive lock for the body's duration")
    func withLockExcludes() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = storage(at: dir)
        let lockPath = store.lockURL.path

        try store.withLock {
            // An independent open file description must not take the same
            // exclusive flock while the body runs.
            let fd = open(lockPath, O_RDWR | O_CREAT, 0o644)
            #expect(fd >= 0)
            #expect(flock(fd, LOCK_EX | LOCK_NB) != 0, "a second fd took the lock while held")
            close(fd)
        }

        // Released on return: a fresh fd can take it again.
        let fd = open(lockPath, O_RDWR | O_CREAT, 0o644)
        #expect(fd >= 0)
        #expect(flock(fd, LOCK_EX | LOCK_NB) == 0, "the lock was not released after withLock")
        close(fd)
    }

    @Test("a held lock times out as LockedDirectoryError.locked")
    func lockTimesOut() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = storage(at: dir)

        let fd = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
        #expect(fd >= 0)
        #expect(flock(fd, LOCK_EX) == 0)
        defer { close(fd) }

        #expect(throws: LockedDirectoryError.locked) {
            try store.withLock {}
        }
    }

    // MARK: - Directory fsync

    @Test("a directory fsync over an unopenable directory throws the open failure")
    func fsyncDirectoryOpenFailureThrows() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        // cleanRoot sits behind a regular file, so open(2) fails with ENOTDIR.
        let notADir = dir.appendingPathComponent("not-a-dir")
        try "a file".write(to: notADir, atomically: false, encoding: .utf8)
        let store = LockedDirectory(
            lockURL: dir.appendingPathComponent("sidecar.lock"),
            tempPrefix: ".side-",
            cleanRoot: notADir.appendingPathComponent("sub"),
            cleanRecursively: false)

        #expect(throws: LockedDirectoryError.posix(op: "open directory", code: ENOTDIR)) {
            try store.fsyncDirectory()
        }
    }
}
