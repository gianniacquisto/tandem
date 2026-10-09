import Darwin
import Foundation
import Testing
@testable import TandemCore

/// Concurrency + store-protocol tests (ADR 0001): no lost updates under
/// contention, a lock-free reader that never observes a partial file, the
/// bounded-retry "locked" error, and orphan temp-file cleanup.
///
/// Contention is exercised with threads (separate fds contend via `flock` on
/// macOS) rather than spawned processes. Each test uses its own temp dir.
@Suite("Concurrency")
struct ConcurrencyTests {

    /// A minimal thread-safe box for collecting errors across threads.
    private final class Box<T>: @unchecked Sendable {
        private let lock = NSLock()
        private var value: T
        init(_ value: T) { self.value = value }
        subscript() -> T {
            get { lock.withLock { value } }
            set { lock.withLock { value = newValue } }
        }
    }

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-conc-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    // MARK: - No lost update

    @Test("concurrent adds from two threads do not lose updates")
    func concurrentAddsNoLostUpdates() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let perThread = 20
        let groups = ["A", "B"]
        let done = DispatchGroup()
        for g in groups {
            done.enter()
            DispatchQueue.global().async {
                for i in 0..<perThread {
                    _ = try? store.add("\(g)-\(i)")
                }
                done.leave()
            }
        }
        #expect(done.wait(timeout: .now() + 15) == .success, "writer threads did not finish")

        let all = try store.list(.all).value
        #expect(all.count == perThread * groups.count, "lost an update: got \(all.count)")
        let titles = Set(all.map(\.title))
        for g in groups {
            for i in 0..<perThread {
                #expect(titles.contains("\(g)-\(i)"), "missing \(g)-\(i)")
            }
        }
    }

    // MARK: - Atomicity (reader never sees a partial file)

    @Test("lock-free reader never observes a partial file")
    func readerNeverSeesPartialFile() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")   // ensure the file exists before the race

        struct Envelope: Decodable { let version: Int; let todos: [Todo] }

        let errors = Box<[Error]>([])
        let writerDone = DispatchGroup()

        writerDone.enter()
        DispatchQueue.global().async {
            for i in 0..<120 {
                do { _ = try store.add("w-\(i)") } catch { errors[] = errors[] + [error] }
            }
            writerDone.leave()
        }

        // Reader: repeatedly read the raw file and decode it until the writer is
        // done. Atomic rename guarantees a read sees a complete old or new file; a
        // broken (non-atomic) write would surface here as a decode failure.
        var reads = 0
        for _ in 0..<2000 {
            reads += 1
            if let data = try? Data(contentsOf: store.fileURL) {
                do { _ = try JSONDecoder().decode(Envelope.self, from: data) }
                catch { errors[] = errors[] + [error] }
            }
            if writerDone.wait(timeout: .now()) == .success { break }
        }
        #expect(writerDone.wait(timeout: .now() + 15) == .success, "writer did not finish")
        #expect(reads > 0, "reader never ran")
        #expect(errors[].isEmpty, "reader observed an invalid store state: \(errors[])")
    }

    // MARK: - Bounded-retry "locked" error

    @Test("an op on a locked store throws locked after the bounded retry")
    func lockedStoreThrowsAfterBoundedRetry() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")

        // Hold the store's exclusive flock in another thread for ~2.5s.
        let hasLock = DispatchSemaphore(value: 0)
        let holder = DispatchGroup()
        holder.enter()
        DispatchQueue.global().async {
            let fd = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
            guard fd >= 0 else { holder.leave(); return }
            if flock(fd, LOCK_EX) == 0 { hasLock.signal() }
            usleep(2_500_000)   // 2.5s — longer than the store's ~2s retry window
            close(fd)
            holder.leave()
        }
        // Deterministic: only attempt the op once the holder definitely has the lock.
        #expect(hasLock.wait(timeout: .now() + 5) == .success, "holder never acquired the lock")

        #expect(throws: DomainError.locked) {
            _ = try store.add("should fail while locked")
        }
        #expect(holder.wait(timeout: .now() + 5) == .success, "holder did not release")
    }

    // MARK: - Orphan temp cleanup

    @Test("stale orphan temps are removed, fresh temps are kept")
    func orphanCleanup() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")
        let fm = FileManager.default

        // A crash-leftover orphan older than the 60s threshold.
        let orphan = dir.appendingPathComponent(".todos-\(UUID().uuidString).tmp")
        try "orphan".write(to: orphan, atomically: true, encoding: .utf8)
        try fm.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-120)], ofItemAtPath: orphan.path)
        // A fresh temp that must NOT be treated as an orphan.
        let fresh = dir.appendingPathComponent(".todos-\(UUID().uuidString).tmp")
        try "fresh".write(to: fresh, atomically: true, encoding: .utf8)

        // Any write op triggers orphan cleanup while holding the lock.
        _ = try store.add("trigger cleanup")

        #expect(!fm.fileExists(atPath: orphan.path), "stale orphan was not removed")
        #expect(fm.fileExists(atPath: fresh.path), "fresh temp was wrongly removed")
    }
}
