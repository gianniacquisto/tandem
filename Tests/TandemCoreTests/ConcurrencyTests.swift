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

    /// Run `body` on its own OS thread. The contention these tests exercise is between
    /// separate writers — processes in the field — and the shared dispatch pool starves
    /// under a parallel `swift test`, timing the test out on load rather than on
    /// behaviour (CI runs `--no-parallel` for the same reason). A dedicated thread keeps
    /// the race about the database.
    private func spawn(_ done: DispatchGroup, _ body: @escaping @Sendable () -> Void) {
        done.enter()
        Thread {
            body()
            done.leave()
        }.start()
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
            spawn(done) {
                for i in 0..<perThread {
                    _ = try? store.add("\(g)-\(i)")
                }
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

    // MARK: - The first write (schema and import are one durable decision)

    @Test("concurrent first opens on a fresh home all succeed, one applying v1")
    func concurrentFirstOpensMigrateOnce() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let dbURL = dir.appendingPathComponent("tandem.db")
        let errors = Box<[Error]>([])
        let start = DispatchGroup()
        start.enter()
        let done = DispatchGroup()
        for _ in 0..<8 {
            spawn(done) {
                start.wait()
                do {
                    let db = try SQLiteDatabase(url: dbURL, mode: .readWrite)
                    defer { db.close() }
                    try StoreSchema.migrate(db)
                    #expect(try StoreSchema.version(of: db) == 1)
                } catch {
                    errors[] = errors[] + [error]
                }
            }
        }
        start.leave()
        #expect(done.wait(timeout: .now() + 20) == .success, "first opens did not finish")
        #expect(errors[].isEmpty, "a first open failed: \(errors[])")

        let db = try SQLiteDatabase(url: dbURL, mode: .readOnly)
        defer { db.close() }
        #expect(try db.scalarInt("SELECT COUNT(*) FROM schema_migrations") == 1,
                "v1 must be recorded once, not once per opener")
    }

    @Test("two first writes on a JSON home import once and lose neither writer's rows")
    func concurrentFirstWritesImportOnce() throws {
        let fixture = """
            {"version":1,"lanes":[],"tickets":[
            {"id":"00000000-0000-0000-0000-000000000001","title":"legacy","status":"backlog","archived":false,"description":null,"assignee":null,"origin":"human","lane":null,"link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}
            """
        for round in 0..<5 {
            let dir = tempDir()
            defer { try? FileManager.default.removeItem(at: dir) }
            try fixture.write(
                to: dir.appendingPathComponent("tickets.json"), atomically: false, encoding: .utf8)
            let store = Store(directory: dir)
            let errors = Box<[Error]>([])
            let start = DispatchGroup()
            start.enter()
            let done = DispatchGroup()
            for title in ["A", "B"] {
                spawn(done) {
                    start.wait()
                    do { _ = try store.add(title) } catch { errors[] = errors[] + [error] }
                }
            }
            start.leave()
            #expect(done.wait(timeout: .now() + 20) == .success,
                    "round \(round): writers did not finish")
            #expect(errors[].isEmpty, "round \(round): \(errors[])")

            let titles = Set(try store.list(.all).value.map(\.title))
            #expect(titles == ["legacy", "A", "B"], "round \(round): got \(titles)")
            let copies = try FileManager.default.contentsOfDirectory(atPath: dir.path)
                .filter { $0.hasPrefix("tickets.json.pre-sqlite-") }
            #expect(copies.count == 1, "round \(round): one import, got \(copies)")
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

        spawn(writerDone) {
            for i in 0..<60 {
                do { _ = try store.add("w-\(i)") } catch { errors[] = errors[] + [error] }
            }
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

        spawn(done) {
            for i in 0..<rounds {
                _ = try? store.set(id, status: .set(statuses[i % statuses.count]))
            }
        }
        spawn(done) {
            for i in 0..<rounds {
                _ = try? store.set(id, description: .set(bodies[i % bodies.count]))
            }
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
