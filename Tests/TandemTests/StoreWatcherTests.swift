import Foundation
import Testing
import TandemCore
@testable import Tandem

/// The app's watcher, against a real store on disk.
///
/// The bug (3D77D0BD): the store writes through SQLite in WAL mode, so a write's
/// only remaining trace after the transaction commits is an append to
/// `tandem.db-wal`. A `DispatchSource` on the *directory* desk does not see a
/// regular file's content change (measured: five appends → zero events; one new
/// entry → two events), so the app never refreshed for a CLI write. The directory
/// events it did see come from the store's rolling backup, which is taken *inside*
/// the transaction — a reload there reads the pre-commit snapshot.
@Suite("StoreWatcher")
struct StoreWatcherTests {

    /// A serial queue of the watcher's own: the app's `onEvent` runs on the main
    /// queue, but a test process has no main run loop to pump, so the tests hand
    /// the watcher a queue they can block on.
    private let eventQueue = DispatchQueue(label: "tandem.tests.store-watcher")

    @Test("the watch set is the store directory and tandem.db-wal beside the database")
    func watchedPaths() {
        let directory = URL(fileURLWithPath: "/tmp/tandem-home")
        #expect(StoreWatcher.watchedPaths(in: directory).map(\.lastPathComponent)
            == ["tandem-home", "tandem.db-wal"])
        #expect(StoreWatcher.walURL(in: directory)
            == directory.appendingPathComponent("tandem.db-wal"))
    }

    @Test("an append to an existing -wal reaches onEvent — no directory entry changes")
    func walAppendIsSeen() throws {
        let home = try makeImportedHome()
        let wal = StoreWatcher.walURL(in: home)
        #expect(FileManager.default.fileExists(atPath: wal.path))

        let counter = Counter()
        let watcher = StoreWatcher(directory: home, queue: eventQueue)
        watcher.onEvent = { counter.bump() }
        watcher.start()
        defer { watcher.stop() }

        try append(4096, to: wal)

        #expect(counter.waitFor(1), "an append to the existing -wal fired no event")
    }

    @Test("a -wal replaced after the watcher attached is re-established")
    func walIsReEstablished() throws {
        let home = try makeImportedHome()
        let wal = StoreWatcher.walURL(in: home)

        let counter = Counter()
        let watcher = StoreWatcher(directory: home, queue: eventQueue)
        watcher.onEvent = { counter.bump() }
        watcher.start()
        defer { watcher.stop() }

        // SQLite removes the journal on a clean last close; a later write creates a
        // new inode. The directory watch is what tells the watcher to re-attach.
        try FileManager.default.removeItem(at: wal)
        try Data().write(to: wal)
        #expect(counter.waitFor(1), "the new -wal entry fired no directory event")
        counter.reset()

        try append(4096, to: wal)

        #expect(counter.waitFor(1), "the re-created -wal was not watched")
    }

    @Test("a home with no database still refreshes on a new entry — the JSON fallback keeps working")
    func directoryEntryIsSeen() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-watcher-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: home) }

        let counter = Counter()
        let watcher = StoreWatcher(directory: home, queue: eventQueue)
        watcher.onEvent = { counter.bump() }
        watcher.start()
        defer { watcher.stop() }

        try Data("{}".utf8).write(to: home.appendingPathComponent("tickets.json"))

        #expect(counter.waitFor(1), "a new entry in a database-less home fired no event")
    }

    // MARK: - Helpers

    /// A home whose first write has already happened: `tandem.db` and the `-wal`
    /// exist, so a second write appends to an existing journal inode.
    private func makeImportedHome() throws -> URL {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-watcher-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        _ = try Store(directory: home).add("imported")
        return home
    }

    /// Append to a file in place — a content change with no directory entry event.
    private func append(_ bytes: Int, to url: URL) throws {
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(repeating: 0x41, count: bytes))
    }

    /// The watcher's `onEvent` runs on a serial queue in tests, so the wait is a
    /// poll — there is no main run loop to pump in a test process.
    private final class Counter: @unchecked Sendable {
        private let lock = NSLock()
        private var count = 0

        func bump() {
            lock.lock()
            count += 1
            lock.unlock()
        }

        func reset() {
            lock.lock()
            count = 0
            lock.unlock()
        }

        func waitFor(_ target: Int, timeout: TimeInterval = 3) -> Bool {
            let deadline = Date().addingTimeInterval(timeout)
            while current < target, Date() < deadline {
                Thread.sleep(forTimeInterval: 0.02)
            }
            return current >= target
        }

        private var current: Int {
            lock.lock()
            defer { lock.unlock() }
            return count
        }
    }
}
