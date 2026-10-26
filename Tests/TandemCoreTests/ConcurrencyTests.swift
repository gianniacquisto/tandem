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


    // MARK: - The sidecar domains (P2): the transaction is the append's ordering

    @Test("concurrent comment appends to one ticket lose no comment")
    func concurrentCommentAppendsLoseNothing() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let ticket = try store.add("A ticket").value

        let perThread = 20
        let errors = Box<[Error]>([])
        let done = DispatchGroup()
        for group in ["A", "B"] {
            spawn(done) {
                for i in 0..<perThread {
                    do {
                        _ = try store.append(
                            ticketID: ticket.id.uuidString,
                            author: group, body: "\(group)-\(i)")
                    } catch { errors[] = errors[] + [error] }
                }
            }
        }
        #expect(done.wait(timeout: .now() + 20) == .success, "writers did not finish")
        #expect(errors[].isEmpty, "a comment append failed: \(errors[])")

        let comments = try store.list(ticketID: ticket.id.uuidString).value
        #expect(comments.count == perThread * 2, "lost a comment: got \(comments.count)")
        let bodies = Set(comments.map(\.body))
        for group in ["A", "B"] {
            for i in 0..<perThread {
                #expect(bodies.contains("\(group)-\(i)"), "missing \(group)-\(i)")
            }
        }
    }

    @Test("two first comment writes on a JSON log import once and lose neither append")
    func concurrentFirstCommentWritesImportOnce() throws {
        for round in 0..<5 {
            let dir = tempDir()
            defer { try? FileManager.default.removeItem(at: dir) }
            let store = Store(directory: dir)
            let ticket = try store.add("A ticket").value
            let comments = store.commentsDirectory
            try FileManager.default.createDirectory(
                at: comments, withIntermediateDirectories: true)
            let log = comments.appendingPathComponent("\(ticket.id.uuidString).jsonl")
            try #"{"author":"human","at":"2026-01-01T00:00:00Z","body":"legacy"}"#
                .appending("\n").write(to: log, atomically: false, encoding: .utf8)

            let errors = Box<[Error]>([])
            let start = DispatchGroup()
            start.enter()
            let done = DispatchGroup()
            for body in ["A", "B"] {
                spawn(done) {
                    start.wait()
                    do {
                        _ = try store.append(
                            ticketID: ticket.id.uuidString, author: "builder", body: body)
                    } catch { errors[] = errors[] + [error] }
                }
            }
            start.leave()
            #expect(done.wait(timeout: .now() + 20) == .success,
                    "round \(round): writers did not finish")
            #expect(errors[].isEmpty, "round \(round): \(errors[])")

            let bodies = Set(try store.list(ticketID: ticket.id.uuidString).value.map(\.body))
            #expect(bodies == ["legacy", "A", "B"], "round \(round): got \(bodies)")
            let copies = try FileManager.default.contentsOfDirectory(atPath: comments.path)
                .filter { $0.hasPrefix("\(ticket.id.uuidString).jsonl.pre-sqlite-") }
            #expect(copies.count == 1, "round \(round): one import, got \(copies)")
        }
    }

    @Test("two first session writes on a JSON home import once and lose neither record")
    func concurrentFirstSessionWritesImportOnce() throws {
        for round in 0..<5 {
            let dir = tempDir()
            defer { try? FileManager.default.removeItem(at: dir) }
            let fixture = #"{"version":1,"sessions":[{"id":"ses_legacy","project":"tandem","agent":null,"brief":"Legacy.","summary":null,"status":"running","outcome":null,"tickets":[],"startedAt":"2026-01-01T00:00:00Z","endedAt":null}]}"#
            try fixture.write(
                to: dir.appendingPathComponent("sessions.json"), atomically: false, encoding: .utf8)
            let sessions = Sessions(directory: dir)

            let errors = Box<[Error]>([])
            let start = DispatchGroup()
            start.enter()
            let done = DispatchGroup()
            for id in ["ses_a", "ses_b"] {
                spawn(done) {
                    start.wait()
                    do {
                        _ = try sessions.start(id: id, project: "tandem", brief: "Work.")
                    } catch { errors[] = errors[] + [error] }
                }
            }
            start.leave()
            #expect(done.wait(timeout: .now() + 20) == .success,
                    "round \(round): writers did not finish")
            #expect(errors[].isEmpty, "round \(round): \(errors[])")

            let ids = Set(try sessions.list().value.map(\.id))
            #expect(ids == ["ses_legacy", "ses_a", "ses_b"], "round \(round): got \(ids)")
            let copies = try FileManager.default.contentsOfDirectory(atPath: dir.path)
                .filter { $0.hasPrefix("sessions.json.pre-sqlite-") }
            #expect(copies.count == 1, "round \(round): one import, got \(copies)")
        }
    }

    @Test("two first sprint writes on a JSON home import once and keep one open sprint per lane")
    func concurrentFirstSprintWritesImportOnce() throws {
        for round in 0..<5 {
            let dir = tempDir()
            defer { try? FileManager.default.removeItem(at: dir) }
            let fixture = #"{"version":1,"sprints":[{"lane":"Tandem","number":1,"session":null,"mode":"serial","batch":[],"runs":[],"close":"Landed.","status":"closed","openedAt":"2026-01-01T00:00:00Z","closedAt":"2026-01-02T00:00:00Z"}]}"#
            try fixture.write(
                to: dir.appendingPathComponent("sprints.json"), atomically: false, encoding: .utf8)
            let sprints = Sprints(directory: dir)

            let errors = Box<[Error]>([])
            let start = DispatchGroup()
            start.enter()
            let done = DispatchGroup()
            for lane in ["Tandem", "Work"] {
                spawn(done) {
                    start.wait()
                    do { _ = try sprints.open(lane: lane) } catch { errors[] = errors[] + [error] }
                }
            }
            start.leave()
            #expect(done.wait(timeout: .now() + 20) == .success,
                    "round \(round): writers did not finish")
            #expect(errors[].isEmpty, "round \(round): \(errors[])")

            let list = try sprints.list().value
            #expect(list.count == 3, "round \(round): got \(list.map(\.name))")
            #expect(list.filter { $0.lane == "Tandem" && $0.status == .open }
                .map(\.number) == [2], "round \(round): the imported Sprint 1 stays closed")
            #expect(list.filter { $0.lane == "Work" }.map(\.number) == [1])
            let copies = try FileManager.default.contentsOfDirectory(atPath: dir.path)
                .filter { $0.hasPrefix("sprints.json.pre-sqlite-") }
            #expect(copies.count == 1, "round \(round): one import, got \(copies)")
        }
    }

    @Test("two first settings writes on a JSON home import once and keep both keys")
    func concurrentFirstSettingsWritesImportOnce() throws {
        for round in 0..<5 {
            let dir = tempDir()
            defer { try? FileManager.default.removeItem(at: dir) }
            try #"{"bikeshedPath":"/tmp/Notes","setupComplete":true}"#.write(
                to: dir.appendingPathComponent("settings.json"), atomically: false, encoding: .utf8)

            let errors = Box<[Error]>([])
            let start = DispatchGroup()
            start.enter()
            let done = DispatchGroup()
            spawn(done) {
                start.wait()
                do { try TandemSettings.setDefaultLane("Board", in: dir) }
                catch { errors[] = errors[] + [error] }
            }
            spawn(done) {
                start.wait()
                do { try TandemSettings.setShowHiddenFiles(true, in: dir) }
                catch { errors[] = errors[] + [error] }
            }
            start.leave()
            #expect(done.wait(timeout: .now() + 20) == .success,
                    "round \(round): writers did not finish")
            #expect(errors[].isEmpty, "round \(round): \(errors[])")

            #expect(TandemSettings.chosenBikeshed(in: dir)
                    == URL(fileURLWithPath: "/tmp/Notes").standardizedFileURL)
            #expect(TandemSettings.setupComplete(in: dir))
            #expect(TandemSettings.defaultLane(in: dir) == "Board")
            #expect(TandemSettings.showHiddenFiles(in: dir))
            let copies = try FileManager.default.contentsOfDirectory(atPath: dir.path)
                .filter { $0.hasPrefix("settings.json.pre-sqlite-") }
            #expect(copies.count == 1, "round \(round): one import, got \(copies)")
        }
    }

    @Test("sidecar writers and ticket writers share the one database without losing either")
    func sidecarAndTicketWritersShareTheTransaction() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let sessions = Sessions(directory: dir)
        let ticket = try store.add("seed").value

        let errors = Box<[Error]>([])
        let done = DispatchGroup()
        spawn(done) {
            for i in 0..<20 {
                do { _ = try store.add("t-\(i)") } catch { errors[] = errors[] + [error] }
            }
        }
        spawn(done) {
            for i in 0..<20 {
                do {
                    _ = try store.append(
                        ticketID: ticket.id.uuidString, author: "builder", body: "c-\(i)")
                } catch { errors[] = errors[] + [error] }
            }
        }
        spawn(done) {
            for i in 0..<20 {
                do { _ = try sessions.start(id: "ses_\(i)", project: "tandem", brief: "W.") }
                catch { errors[] = errors[] + [error] }
            }
        }
        #expect(done.wait(timeout: .now() + 30) == .success, "writers did not finish")
        #expect(errors[].isEmpty, "a shared-database write failed: \(errors[])")

        #expect(try store.list(.all).value.count == 21)
        #expect(try store.list(ticketID: ticket.id.uuidString).value.count == 20)
        #expect(try sessions.list().value.count == 20)
    }

    // MARK: - The first write (schema and import are one durable decision)

    @Test("concurrent first opens on a fresh home all succeed, one applying the schema")
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
                    #expect(try StoreSchema.version(of: db) == StoreSchema.currentVersion)
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
        #expect(try db.scalarInt("SELECT COUNT(*) FROM schema_migrations") == StoreSchema.currentVersion,
                "each migration must be recorded once, not once per opener")
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
