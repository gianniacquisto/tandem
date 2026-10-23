import Foundation
import Testing
@testable import TandemCore

/// The rolling backup (ADR 0018): before each successful write, the file about to
/// be replaced is copied to `todos.json.bak` — one generation, atomically, under
/// the write's own lock. These tests pin the three states the ticket names: no
/// `.bak` on the first write, the pre-write state on the second, and the previous
/// `.bak` untouched when a write fails.
@Suite("Rolling store backup")
struct StoreBackupTests {

    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-backup-\(UUID().uuidString)")
    }

    /// The `.bak` file's bytes, or nil when there is no backup.
    private func backupBytes(_ store: Store) -> Data? {
        try? Data(contentsOf: store.backupURL)
    }

    /// The titles in the store file itself (not the backup).
    private func fileTitles(_ store: Store) throws -> [String] {
        try titles(in: store.storeFileURL)
    }

    private func backupTitles(_ store: Store) throws -> [String] {
        try titles(in: store.backupURL)
    }

    private func titles(in url: URL) throws -> [String] {
        guard
            let object = try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        else {
            throw StoreIOError.io("test fixture is not a store file: \(url.path)")
        }
        // The writer emits the renamed `tickets` key; a backup of a pre-handover file
        // carries the v1 `todos` one.
        guard let todos = (object["tickets"] as? [[String: Any]])
            ?? (object["todos"] as? [[String: Any]])
        else {
            throw StoreIOError.io("test fixture is not a store file: \(url.path)")
        }
        return todos.compactMap { $0["title"] as? String }
    }

    // MARK: - The generations

    @Test("the first write leaves no .bak: there was no file to roll")
    func firstWriteHasNoBackup() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")

        #expect(backupBytes(store) == nil, "the first write must not create a backup")
        #expect(try fileTitles(store) == ["one"])
    }

    @Test("the second write's .bak holds the state before that write")
    func secondWriteBacksUpThePreviousState() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        _ = try store.add("two")

        #expect(try fileTitles(store) == ["two", "one"])
        #expect(try backupTitles(store) == ["one"], ".bak must hold the state before the second write")
    }

    @Test("each further write rolls the backup forward one generation")
    func backupRollsForward() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        _ = try store.add("two")
        _ = try store.add("three")

        #expect(try fileTitles(store) == ["three", "two", "one"])
        #expect(try backupTitles(store) == ["two", "one"], "only one previous copy is kept")
    }

    @Test("a write that changes nothing writes no backup")
    func unchangedWriteMakesNoBackup() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        _ = try store.add("two")
        let rolled = try backupTitles(store)

        let id = try store.list(.all).value.first { $0.title == "two" }?.id.uuidString ?? ""
        // Reopening an open ticket changes nothing, so nothing is written — and the
        // backup must not move for a write that did not happen.
        _ = try store.reopen(id)

        #expect(try backupTitles(store) == rolled)
    }

    @Test("the backup is not half-written: it is a complete store file from before the write")
    func backupIsACompletePreviousFile() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let first = try store.add("one")
        let beforeSecondWrite = try Data(contentsOf: store.storeFileURL)
        _ = try store.complete(first.value.id.uuidString)

        #expect(try Data(contentsOf: store.backupURL) == beforeSecondWrite,
                ".bak must be exactly the file the write replaced")
        #expect(try backupTitles(store) == ["one"])
        #expect(store.health() == .counts(open: 0, done: 1))
    }

    // MARK: - Failure

    @Test("a failed operation leaves the store and the previous .bak untouched")
    func failedOperationLeavesBackupUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        _ = try store.add("two")
        let storeBytes = try Data(contentsOf: store.storeFileURL)
        let backupBefore = try Data(contentsOf: store.backupURL)

        // An unknown id fails inside the mutation, before any write.
        #expect(throws: (any Error).self) {
            _ = try store.complete("00000000-0000-0000-0000-000000000001")
        }

        #expect(try Data(contentsOf: store.storeFileURL) == storeBytes)
        #expect(try Data(contentsOf: store.backupURL) == backupBefore,
                "a failed write must not touch the previous .bak")
    }

    @Test("a write that fails in the IO path leaves the store, the .bak and no temp behind")
    func failedWriteLeavesBackupUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("one")
        _ = try store.add("two")
        let storeBytes = try Data(contentsOf: store.storeFileURL)

        // A directory where the backup belongs: rolling it fails after the new bytes
        // are durable, so the whole write must be abandoned before the commit.
        try FileManager.default.removeItem(at: store.backupURL)
        try FileManager.default.createDirectory(at: store.backupURL, withIntermediateDirectories: false)

        #expect(throws: StoreIOError.self) {
            _ = try store.add("three")
        }

        #expect(try Data(contentsOf: store.storeFileURL) == storeBytes,
                "the store file must be untouched by the failed write")
        var isDir: ObjCBool = false
        #expect(FileManager.default.fileExists(atPath: store.backupURL.path, isDirectory: &isDir))
        #expect(isDir.boolValue, "the previous .bak must be untouched, not replaced")
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix(".todos-") }
        #expect(leftovers.isEmpty, "a failed write left temps behind: \(leftovers)")
    }

    // MARK: - Diagnostics

    @Test("backupHealth reports absence, then the age of the rolled copy")
    func backupHealthReportsPresenceAndAge() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        #expect(store.backupHealth() == .absent)

        _ = try store.add("one")
        #expect(store.backupHealth() == .absent, "one write is still no backup")
        _ = try store.add("two")

        guard case .present(let age) = store.backupHealth() else {
            Issue.record("a backup exists but backupHealth says absent")
            return
        }
        #expect(age >= 0 && age < 60, "age \(age)s is not a just-written backup")
    }
}
