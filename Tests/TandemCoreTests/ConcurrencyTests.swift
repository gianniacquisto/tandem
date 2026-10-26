import Darwin
import Foundation
import Testing
@testable import TandemCore

/// Concurrency tests for the database-backed store (ADR 0031): no lost updates under
/// contention, and a lock-free reader that sees a consistent WAL snapshot while a writer
/// works. Contention is exercised with threads (each write opens its own connection, as
/// a separate process would). Each test uses its own temp dir.
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

    // MARK: - Atomicity (a reader sees a consistent snapshot)

    @Test("a lock-free reader sees a consistent snapshot while a writer works")
    func readerSeesAConsistentSnapshot() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")   // ensure the database exists before the race

        let errors = Box<[Error]>([])
        let writerDone = DispatchGroup()

        writerDone.enter()
        DispatchQueue.global().async {
            for i in 0..<60 {
                do { _ = try store.add("w-\(i)") } catch { errors[] = errors[] + [error] }
            }
            writerDone.leave()
        }

        // Reader: a WAL snapshot is always a complete state — a read never sees a
        // half-applied write, and a count between 1 and 61 is fine at any point.
        var reads = 0
        var lastCount = 0
        for _ in 0..<2000 {
            reads += 1
            do {
                let count = try store.list(.all).value.count
                #expect(count >= 1 && count <= 61, "impossible ticket count \(count)")
                #expect(count >= lastCount, "a read lost committed tickets: \(lastCount) -> \(count)")
                lastCount = count
            } catch {
                errors[] = errors[] + [error]
            }
            if writerDone.wait(timeout: .now()) == .success { break }
        }
        #expect(writerDone.wait(timeout: .now() + 15) == .success, "writer did not finish")
        #expect(reads > 0, "reader never ran")
        #expect(errors[].isEmpty, "reader observed an inconsistent store: \(errors[])")
        #expect(try store.list(.all).value.count == 61)
    }

    // MARK: - Ticket fields under contention

    @Test("two writers setting different fields never produce a half-edited ticket")
    func concurrentFieldSetsDoNotClobber() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let id = try store.add("shared").value.id.uuidString
        let statuses: [Status] = [.ready, .inProgress]
        let bodies = ["alpha", "beta"]
        let rounds = 25
        let done = DispatchGroup()

        done.enter()
        DispatchQueue.global().async {
            for i in 0..<rounds {
                _ = try? store.set(id, status: .set(statuses[i % statuses.count]))
            }
            done.leave()
        }
        done.enter()
        DispatchQueue.global().async {
            for i in 0..<rounds {
                _ = try? store.set(id, description: .set(bodies[i % bodies.count]))
            }
            done.leave()
        }
        #expect(done.wait(timeout: .now() + 15) == .success, "writer threads did not finish")

        // Each writer named one field, so neither field may carry the other's edit away.
        let todo = try #require(try store.list(.all).value.first)
        #expect(bodies.contains(todo.description ?? ""), "the description survived the status writes")
        #expect(todo.status == .ready || todo.status == .inProgress,
                "the status survived the description writes")
        #expect(todo.title == "shared")
    }
}
