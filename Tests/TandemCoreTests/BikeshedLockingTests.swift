import Darwin
import Foundation
import Testing
@testable import TandemCore

/// Locking and atomicity (ADR 0004): two app writers on threads under the
/// sidecar `flock` serialize — no lost updates, no torn notes; a holder
/// keeps the lock past the ~2 s bounded retry window → `BikeshedError.locked`;
/// an external lock-less writer interleaved — readers (lock-free) always
/// observe a complete note; orphaned `.note-*.tmp` files older than 60 s are
/// cleaned after acquiring the lock, fresh temps (in-flight writes) are
/// never touched.
@Suite("Bikeshed locking and atomicity")
struct BikeshedLockingTests {

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

    /// A unique temp dir, nested one level below a unique parent: the
    /// vault's sibling lockfile (`bikeshed.lock`, ADR 0004) must not be
    /// shared between concurrent tests.
    private func tempDir() -> URL {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-lock-\(UUID().uuidString)")
        let dir = parent.appendingPathComponent("vault")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    // MARK: - Contended writers serialize

    @Test("two app writers on threads interleave without lost or torn notes")
    func contendedWritersNoLostUpdates() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "", in: dir)
        try stage("B.md", "", in: dir)

        let perThread = 20
        let errors = Box<[Error]>([])
        let done = DispatchGroup()
        // Both threads hammer the SAME note: with per-write atomic rename and
        // the exclusive lock, the final content is one whole write — never a
        // mix of two.
        for tag in ["X", "Y"] {
            done.enter()
            DispatchQueue.global().async {
                for i in 0..<perThread {
                    do {
                        _ = try bikeshed.write(path: "A.md", text: "\(tag)\(i)")
                        _ = try bikeshed.write(path: "B.md", text: "\(tag)\(i)")
                    } catch {
                        errors[] = errors[] + [error]
                    }
                }
                done.leave()
            }
        }
        #expect(done.wait(timeout: .now() + 15) == .success, "writer threads did not finish")
        #expect(errors[].isEmpty, "writer hit an error: \(errors[])")

        // Each thread wrote i = 0...19 in order, so the final content is
        // exactly the last write — "X19" or "Y19": one whole value, never a
        // torn or mixed file.
        let finalA = try String(contentsOf: dir.appendingPathComponent("A.md"), encoding: .utf8)
        let finalB = try String(contentsOf: dir.appendingPathComponent("B.md"), encoding: .utf8)
        #expect(finalA == "X19" || finalA == "Y19", "torn or lost A.md: \(finalA)")
        #expect(finalB == "X19" || finalB == "Y19", "torn or lost B.md: \(finalB)")
    }

    // MARK: - Bounded retry, then locked

    @Test("a write on a locked vault throws locked after the bounded retry")
    func lockedVaultThrows() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "seed", in: dir)

        // Hold the vault's exclusive flock in another thread for ~2.5s —
        // longer than the ~2 s bounded retry window.
        let hasLock = DispatchSemaphore(value: 0)
        let holder = DispatchGroup()
        holder.enter()
        DispatchQueue.global().async {
            let fd = open(bikeshed.lockURL.path, O_RDWR | O_CREAT, 0o644)
            guard fd >= 0 else { holder.leave(); return }
            if flock(fd, LOCK_EX) == 0 { hasLock.signal() }
            usleep(2_500_000)   // 2.5 s
            close(fd)
            holder.leave()
        }
        // Deterministic: only attempt the op once the holder has the lock.
        // 15 s windows (matching ConcurrencyTests): ~10 s process stalls on
        // macOS 27 runners deschedule the holder past a 5 s window.
        #expect(hasLock.wait(timeout: .now() + 15) == .success, "holder never acquired the lock")

        #expect(throws: BikeshedError.locked) {
            _ = try bikeshed.write(path: "A.md", text: "should fail while locked")
        }
        #expect(holder.wait(timeout: .now() + 15) == .success, "holder did not release")

        // After release the same write succeeds (bounded retry, not a
        // permanent failure).
        _ = try bikeshed.write(path: "A.md", text: "after release")
        #expect(try String(contentsOf: dir.appendingPathComponent("A.md"), encoding: .utf8)
            == "after release")
    }

    // MARK: - Lock-free readers vs a lock-less external writer

    @Test("a lock-free reader never observes a partial note from an external writer")
    func externalWriterReadersSeeCompleteFiles() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "x", in: dir)

        let errors = Box<[Error]>([])
        let writerDone = DispatchGroup()

        // An external (agent/editor) writer: direct file writes, no lock.
        // Atomic temp+rename — how real tools write — so the on-disk note is
        // always a complete one of the written values.
        writerDone.enter()
        DispatchQueue.global().async {
            for i in 1...120 {
                let text = String(repeating: "x", count: i)
                do {
                    try text.write(to: dir.appendingPathComponent("A.md"), atomically: true, encoding: .utf8)
                } catch {
                    errors[] = errors[] + [error]
                }
            }
            writerDone.leave()
        }

        // The app's read path takes no lock: it must only ever see a complete
        // note — a run of 'x', never a torn or mixed file.
        var reads = 0
        for _ in 0..<2000 {
            reads += 1
            if let result = try? bikeshed.read(path: "A.md") {
                let text = result.value.text
                let complete = !text.isEmpty
                    && text.allSatisfy { $0 == "x" }
                    && text.count <= 120
                if !complete {
                    errors[] = errors[] + [BikeshedIOError.io("partial read: \(text.count) chars")]
                }
            }
            if writerDone.wait(timeout: .now()) == .success { break }
        }
        #expect(writerDone.wait(timeout: .now() + 15) == .success, "external writer did not finish")
        #expect(reads > 0, "reader never ran")
        #expect(errors[].isEmpty, "reader observed an incomplete note: \(errors[])")
    }

    // MARK: - Orphan cleanup

    @Test("stale orphan temps are removed after acquiring the lock; fresh temps are kept")
    func orphanCleanup() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("A.md", "seed", in: dir)
        let fm = FileManager.default

        // A crash-leftover orphan (older than the 60 s threshold), in a
        // subfolder — temps live in the note's folder.
        let orphan = dir.appendingPathComponent("deep/.note-\(UUID().uuidString).tmp")
        try fm.createDirectory(at: orphan.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "orphan".write(to: orphan, atomically: false, encoding: .utf8)
        try fm.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-120)], ofItemAtPath: orphan.path)
        // A fresh temp that must NOT be treated as an orphan.
        let fresh = dir.appendingPathComponent(".note-\(UUID().uuidString).tmp")
        try "fresh".write(to: fresh, atomically: false, encoding: .utf8)

        // Any locked write triggers the cleanup pass.
        _ = try bikeshed.write(path: "A.md", text: "trigger cleanup")

        #expect(!fm.fileExists(atPath: orphan.path), "stale orphan was not removed")
        #expect(fm.fileExists(atPath: fresh.path), "fresh temp was wrongly removed")
    }

    // MARK: - Helpers

    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }
}
