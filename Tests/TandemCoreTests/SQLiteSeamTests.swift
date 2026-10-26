import Foundation
import Testing
@testable import TandemCore

/// The SQLite seam (ADR 0031, ticket 6B96343A P1): creating `tandem.db` on a fresh
/// home, WAL on, `schema_migrations` applying in order, a failing migration rolling
/// back and failing the open, the `busy_timeout` window surfacing the store's "locked"
/// domain error, and the rolling backup the format keeps (ADR 0018).
///
/// These tests are allowed to know the storage: they are the seam's own suite, not an
/// operation suite. Everything above them talks only to `Store`'s public API.
@Suite("SQLite seam")
struct SQLiteSeamTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sqlite-\(UUID().uuidString)")
    }

    private func makeDir() throws -> URL {
        let dir = tempDir()
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func entries(in dir: URL) throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: dir.path).sorted()
    }

    // MARK: - Open and create

    @Test("a fresh home's first write creates tandem.db, WAL on, schema version 1")
    func freshHomeCreatesDatabase() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("first")

        #expect(FileManager.default.fileExists(atPath: store.databaseURL.path))
        #expect(store.schemaVersion() == 1)
        let db = try SQLiteDatabase(url: store.databaseURL, mode: .readOnly)
        defer { db.close() }
        #expect(try db.scalarString("PRAGMA journal_mode") == "wal")
        let versions = try db.rows("SELECT version FROM schema_migrations ORDER BY version") {
            $0.int(0)
        }
        #expect(versions == [1])
        let appliedAt = try #require(try db.scalarString("SELECT appliedAt FROM schema_migrations"))
        #expect(Timestamp.decode(appliedAt) != nil, "appliedAt is an ISO-8601 UTC timestamp")
    }

    @Test("the database's tables are the ones the format names")
    func schemaTables() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try Store(directory: dir).add("first")

        let db = try SQLiteDatabase(url: dir.appendingPathComponent("tandem.db"), mode: .readOnly)
        defer { db.close() }
        let tables = Set(try db.rows(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name") { $0.text(0) })
        #expect(tables.isSuperset(of: [
            "schema_migrations", "tickets", "lanes", "comments", "sessions", "sprints", "settings"
        ]))
    }

    @Test("a read creates nothing: no database, no schema, no directory")
    func readCreatesNothing() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }

        let result = try Store(directory: dir).list(.all)

        #expect(result.value.isEmpty)
        #expect(try entries(in: dir).isEmpty)
    }

    // MARK: - Migrations

    @Test("migrations apply in order, each recording its version and timestamp")
    func migrationsApplyInOrder() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = try SQLiteDatabase(url: dir.appendingPathComponent("tandem.db"))
        defer { db.close() }

        let migrations = [
            StoreSchema.Migration(version: 1, name: "one") { db in
                try db.execute("CREATE TABLE a (x INTEGER)")
            },
            StoreSchema.Migration(version: 2, name: "two") { db in
                try db.execute("CREATE TABLE b (x INTEGER)")
            }
        ]
        try StoreSchema.migrate(db, migrations: migrations)

        #expect(try StoreSchema.version(of: db) == 2)
        let versions = try db.rows("SELECT version FROM schema_migrations ORDER BY version") {
            $0.int(0)
        }
        #expect(versions == [1, 2])
        for stamp in try db.rows("SELECT appliedAt FROM schema_migrations") { $0.text(0) } {
            #expect(Timestamp.decode(stamp) != nil)
        }
    }

    @Test("a second migrate applies nothing new")
    func migrateIsIdempotent() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = try SQLiteDatabase(url: dir.appendingPathComponent("tandem.db"))
        defer { db.close() }
        try StoreSchema.migrate(db)
        try StoreSchema.migrate(db)
        #expect(try StoreSchema.version(of: db) == StoreSchema.currentVersion)
        #expect(try db.scalarInt("SELECT COUNT(*) FROM schema_migrations") == 1)
    }

    @Test("a failing migration rolls back and fails the open; nothing half-applies")
    func failingMigrationRollsBack() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("tandem.db")
        let db = try SQLiteDatabase(url: url)
        defer { db.close() }

        let migrations = [
            StoreSchema.Migration(version: 1, name: "good") { db in
                try db.execute("CREATE TABLE good (x INTEGER)")
            },
            StoreSchema.Migration(version: 2, name: "bad") { db in
                try db.execute("CREATE TABLE bad (x INTEGER)")
                // The migration fails after its first statement: the transaction must
                // take the created table back with it.
                try db.run("INSERT INTO missing_table (x) VALUES (1)")
            }
        ]

        #expect(throws: (any Error).self) {
            try StoreSchema.migrate(db, migrations: migrations)
        }
        #expect(try StoreSchema.version(of: db) == 1, "only the good migration is recorded")
        let tables = Set(try db.rows(
            "SELECT name FROM sqlite_master WHERE type = 'table'") { $0.text(0) })
        #expect(tables.contains("good"))
        #expect(!tables.contains("bad"), "the failed migration's table was rolled back")
    }

    @Test("the open fails loudly when tandem.db is not a database")
    func storeOpenFailsOnUnreadableDatabase() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try "this is not a database".write(
            to: dir.appendingPathComponent("tandem.db"), atomically: false, encoding: .utf8)
        #expect(throws: StoreIOError.self) { _ = try Store(directory: dir).add("x") }
        #expect(Store(directory: dir).health() == .unreadable)
    }

    // MARK: - Contention

    @Test("busy_timeout surfaces the locked domain error after the window")
    func busyTimeoutSurfacesLocked() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")

        // Another connection holds the write side for longer than the store's window.
        let holderDB = try SQLiteDatabase(url: store.databaseURL)
        try holderDB.run("BEGIN IMMEDIATE")

        let start = Date()
        do {
            _ = try store.add("blocked")
            Issue.record("expected the write to time out as locked")
        } catch DomainError.locked {
            // The store's own words, ADR 0007's translation rule.
        }
        let elapsed = Date().timeIntervalSince(start)
        try holderDB.run("ROLLBACK")
        holderDB.close()

        #expect(elapsed >= 1.5, "the write waited the store's window (elapsed \(elapsed)s)")
        #expect(elapsed < 15, "the write did not wait far past the window (elapsed \(elapsed)s)")
        #expect(try store.list(.all).value.map(\.title) == ["seed"], "the timed-out write wrote nothing")
    }

    @Test("the lock probe reports free, then held while a write transaction is open")
    func lockProbeReadsTheWriteTransaction() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("seed")
        #expect(!store.lockIsHeld())

        let holderDB = try SQLiteDatabase(url: store.databaseURL)
        try holderDB.run("BEGIN IMMEDIATE")
        #expect(store.lockIsHeld(), "an open write transaction is a held lock")
        try holderDB.run("ROLLBACK")
        holderDB.close()
        #expect(!store.lockIsHeld())
    }

    // MARK: - Rolling backup (ADR 0018)

    @Test("the first write has no backup; the second's backup holds the state before it")
    func rollingBackup() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        #expect(store.backupHealth() == .absent, "the first write has no file to roll")

        _ = try store.add("two")
        #expect(FileManager.default.fileExists(atPath: store.backupURL.path))
        let backup = try SQLiteDatabase(url: store.backupURL, mode: .readOnly)
        defer { backup.close() }
        let titles = try backup.rows("SELECT title FROM tickets ORDER BY position") { $0.text(0) }
        #expect(titles == ["one"], "the backup is the state before the second write")
        guard case .present(let age) = store.backupHealth() else {
            Issue.record("a backup exists but backupHealth says absent")
            return
        }
        #expect(age >= 0 && age < 60)
    }

    @Test("a backup that cannot be taken fails the write and leaves the store as it was")
    func failedBackupFailsTheWrite() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer {
            try? FileManager.default.setAttributes(
                [.posixPermissions: 0o700], ofItemAtPath: dir.path)
        }
        let store = Store(directory: dir)
        _ = try store.add("one")
        _ = try store.add("two")

        // A live connection keeps the WAL files in place, so the write under test reaches
        // the backup rather than failing to open the database in the read-only directory.
        let live = try SQLiteDatabase(url: store.databaseURL)
        try live.run("BEGIN IMMEDIATE")
        try live.run("UPDATE tickets SET title = title")
        try live.run("COMMIT")
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try store.add("three")
            Issue.record("expected the backup to fail the write")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("backup"), "got \(text)")
        }
        #expect(try store.list(.all).value.count == 2, "the failed write changed nothing")

        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: dir.path)
        live.close()
        _ = try store.add("three")
        #expect(try store.list(.all).value.count == 3)
    }

    @Test("reads never roll a backup")
    func readsNeverBackUp() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("one")
        _ = try store.add("two")
        let stamp = try FileManager.default.attributesOfItem(atPath: store.backupURL.path)[.modificationDate] as? Date

        _ = try store.list(.all)
        _ = try store.lanes()
        _ = try store.health()
        _ = store.schemaVersion()

        let after = try FileManager.default.attributesOfItem(atPath: store.backupURL.path)[.modificationDate] as? Date
        #expect(after == stamp)
    }

    @Test("a no-op mutate writes nothing and rolls no backup")
    func noOpWriteDoesNotRollBackup() throws {
        let dir = try makeDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        _ = try store.add("one")
        let id = try store.list(.all).value[0].id.uuidString

        // Reopening an open ticket changes nothing, so nothing is written — and the
        // format's one-generation backup never appears for a write that did not happen.
        _ = try store.reopen(id)
        _ = try store.reopen(id)
        #expect(store.backupHealth() == .absent)
    }
}
