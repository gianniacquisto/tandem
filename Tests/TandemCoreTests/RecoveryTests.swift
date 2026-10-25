import Foundation
import Testing
@testable import TandemCore

/// Corrupt-store recovery (ADR 0001) and result-encoding tests.
///
/// Recovery: an invalid file is backed up (under the lock) to
/// `todos.json.corrupt-<unix-ts>` — or `tickets.json.corrupt-<unix-ts>` after
/// the handover, the artifact keeping the name of the file it was taken from —
/// and the store restarts empty; only the op that
/// performs the recovery surfaces a `note` naming the backup path. Lenient read:
/// unknown extra fields are ignored. Encoding: `StoreEncoding` + `tandemErrorText`
/// produce the exact schema-v1 shapes and actionable error text the CLI sends.
@Suite("Recovery and encoding")
struct RecoveryTests {

    private let validID = "00000000-0000-0000-0000-000000000001"
    private let validTS = "2026-01-01T00:00:00Z"

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-rec-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func writeRaw(_ text: String, to store: Store) throws {
        try text.write(to: store.storeFileURL, atomically: false, encoding: .utf8)
    }

    /// Backup filenames present in the store dir (`todos.json.corrupt-*` before the
    /// handover, `tickets.json.corrupt-*` after).
    private func corruptBackups(in dir: URL) -> [String] {
        guard let entries = try? FileManager.default.contentsOfDirectory(atPath: dir.path) else {
            return []
        }
        return entries
            .filter { $0.hasPrefix("todos.json.corrupt-") || $0.hasPrefix("tickets.json.corrupt-") }
            .sorted()
    }

    private func json(_ text: String) throws -> [String: Any] {
        let obj = try JSONSerialization.jsonObject(with: Data(text.utf8))
        guard let dict = obj as? [String: Any] else { throw TestError() }
        return dict
    }

    private struct TestError: Error {}

    // MARK: - Recovery

    @Test("corrupt file is backed up, store restarts empty, note names an existing backup")
    func corruptFileRecovery() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw("this is not json", to: store)

        let result = try store.list(.all)
        #expect(result.value.isEmpty, "store should restart empty")
        guard let note = result.note else {
            throw TestError() // no note → recovery did not surface
        }
        // The backup path named in the note must exist on disk.
        let backups = corruptBackups(in: dir)
        #expect(!backups.isEmpty, "no backup file was created")
        let namedPath = dir.appendingPathComponent(backups[0]).path
        #expect(note.contains(namedPath) || note.contains(backups[0]),
                "note \(note) does not name the backup \(backups[0])")
        #expect(FileManager.default.fileExists(atPath: namedPath), "named backup does not exist")
    }

    @Test("recovery happens once: the second op has no note and creates no new backup")
    func noDoubleRecovery() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw("garbage", to: store)

        let first = try store.list(.all)
        #expect(first.note != nil, "first op should carry the recovery note")
        let afterFirst = corruptBackups(in: dir)

        let second = try store.list(.all)
        #expect(second.note == nil, "second op should not carry a note")
        #expect(corruptBackups(in: dir) == afterFirst, "a second backup was created")
    }

    @Test("a valid file is never recovered")
    func validFileNotRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":1,"todos":[]}"#, to: store)

        let result = try store.list(.all)
        #expect(result.note == nil, "valid file must not trigger recovery")
        #expect(corruptBackups(in: dir).isEmpty, "no backup should exist for a valid file")
    }

    @Test("a file with neither array key is recovered, never read as an empty list")
    func neitherArrayKeyRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // Neither `todos` nor `tickets`: the array is required, so the file is invalid.
        try writeRaw(#"{"version":1}"#, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "a file with neither array key is invalid")
        #expect(result.value.isEmpty, "the store restarts empty")
        #expect(!corruptBackups(in: dir).isEmpty, "the invalid file was backed up")
    }

    @Test("an unknown version is recovered")
    func unknownVersionRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // v1 and v2 read; anything else is recovered.
        try writeRaw(#"{"version":3,"todos":[]}"#, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "unknown version should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
    }

    @Test("a missing required field is recovered")
    func missingFieldRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // No "title" field.
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","done":false,"createdAt":"\#(validTS)","completedAt":null}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "missing required field should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
    }

    @Test("a file without the legacy done mirror is read, not recovered")
    func missingMirrorIsValid() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // The post-migrator shape: status carries done, and `done` is absent.
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"x","status":"done","createdAt":"\#(validTS)","completedAt":"\#(validTS)"}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note == nil, "the retired mirror is not a required field")
        #expect(corruptBackups(in: dir).isEmpty)
        #expect(result.value.first?.status == .done)
    }

    @Test("a mistyped required field is recovered")
    func mistypedFieldRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // "done" is a string instead of a bool.
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"x","done":"yes","createdAt":"\#(validTS)","completedAt":null}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "mistyped field should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
    }

    @Test("a bad timestamp is recovered")
    func badTimestampRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"x","done":false,"createdAt":"not-a-date","completedAt":null}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "bad timestamp should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
    }

    @Test("an unknown extra field is NOT recovered (lenient read)")
    func extraFieldNotRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"hello","done":false,"createdAt":"\#(validTS)","completedAt":null,"extra":123}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note == nil, "extra field must not trigger recovery")
        #expect(corruptBackups(in: dir).isEmpty)
        #expect(result.value.count == 1)
        #expect(result.value.first?.title == "hello")
    }

    @Test("the store's own corrupt path says what it says today")
    func storeCorruptPathUnchanged() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw("this is not json", to: store)

        // The note: the store's own words, naming the backup it kept.
        let result = try store.list(.all)
        let note = try #require(result.note)
        let backups = corruptBackups(in: dir)
        #expect(backups.count == 1)
        let kept = dir.appendingPathComponent(backups[0]).path
        #expect(note == "Previous store was corrupt and backed up to \(kept); the store restarted empty.")
    }

    @Test("a store file that cannot be backed up still fails as a store error")
    func storeBackupFailureStillAStoreError() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        let store = Store(directory: dir)
        // The lockfile first: it must exist before the directory is closed to
        // writes, or the lock itself would be what fails.
        _ = try store.add("a ticket")
        try writeRaw("this is not json", to: store)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try store.list(.all)
            Issue.record("expected the store's own error")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("Tandem store error"))
            #expect(text.contains("could not back up corrupt store"))
        }
    }

    @Test("lenient read preserves all known fields alongside unknown ones")
    func lenientReadPreservesKnownFields() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"kept","done":true,"createdAt":"\#(validTS)","completedAt":"\#(validTS)","mystery":{"a":1}}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note == nil)
        guard let todo = result.value.first else { throw TestError() }
        #expect(todo.id == UUID(uuidString: validID))
        #expect(todo.title == "kept")
        #expect(todo.done == true)
        #expect(todo.completedAt != nil)
    }

    // MARK: - Encoding

    @Test("StoreEncoding.listResult shape, note present only when non-nil")
    func listResultEncoding() throws {
        let todo = Ticket(title: "one", description: "the body")
        // No note → no note key.
        let noNote = try json(StoreEncoding.listResult([todo], note: nil))
        #expect((noNote["tickets"] as? [Any])?.count == 1)
        #expect(noNote["todos"] == nil, "the old wire key is gone from the encoding")
        #expect(noNote["note"] == nil, "note key must be absent when note is nil")
        // Summary rows: no description key, its length instead.
        let row = try #require((noNote["tickets"] as? [[String: Any]])?.first)
        #expect(row["description"] == nil, "list rows are summaries, not whole tickets")
        #expect(row["descriptionLength"] as? Int == "the body".count)
        #expect(row["title"] as? String == "one")
        // No description → length 0, the key still there (the shape never varies by row).
        let bare = try json(StoreEncoding.listResult([Ticket(title: "bare")], note: nil))
        let bareRow = try #require((bare["tickets"] as? [[String: Any]])?.first)
        #expect(bareRow["descriptionLength"] as? Int == 0)
        // With note → top-level note.
        let withNote = try json(StoreEncoding.listResult([todo], note: "NOTE"))
        #expect(withNote["note"] as? String == "NOTE")
        // Empty → {"tickets":[]}.
        let empty = try json(StoreEncoding.listResult([], note: nil))
        #expect((empty["tickets"] as? [Any])?.isEmpty == true)
    }

    @Test("StoreEncoding.ticketResult is the ticket object plus optional note")
    func ticketResultEncoding() throws {
        let todo = Ticket(title: "thing")
        let obj = try json(StoreEncoding.ticketResult(todo, note: nil))
        #expect(obj["id"] as? String == todo.id.uuidString)
        #expect(obj["title"] as? String == "thing")
        // The wire keeps the derived mirror even though the store file no longer writes it.
        #expect(obj["done"] as? Bool == false)
        #expect(obj["createdAt"] is String)
        #expect(obj["completedAt"] is NSNull, "completedAt should be null while open")
        #expect(obj["note"] == nil)
        let withNote = try json(StoreEncoding.ticketResult(todo, note: "N"))
        #expect(withNote["note"] as? String == "N")
    }

    @Test("StoreEncoding.openList is the list shape")
    func openListEncoding() throws {
        let todo = Ticket(title: "open one", description: "a body")
        let obj = try json(StoreEncoding.openList([todo]))
        #expect((obj["tickets"] as? [Any])?.count == 1)
        #expect(obj["todos"] == nil, "the old wire key is gone from the encoding")
        #expect(obj["note"] == nil)
        // Summarized like the list: the recovery needs the id and the title.
        let row = try #require((obj["tickets"] as? [[String: Any]])?.first)
        #expect(row["description"] == nil)
        #expect(row["descriptionLength"] as? Int == "a body".count)
    }

    @Test("StoreEncoding lane results: the lanes after the write, plus moved on delete")
    func laneResultEncoding() throws {
        let added = try json(StoreEncoding.lanesResult(["Tandem", "fleet"], note: nil))
        #expect(added["lanes"] as? [String] == ["Tandem", "fleet"])
        #expect(added["note"] == nil)

        let withNote = try json(StoreEncoding.lanesResult([], note: "N"))
        #expect((withNote["lanes"] as? [Any])?.isEmpty == true)
        #expect(withNote["note"] as? String == "N")

        let deleted = try json(
            StoreEncoding.laneDeletionResult(
                LaneDeletion(lanes: ["fleet"], moved: 3), note: nil))
        #expect(deleted["lanes"] as? [String] == ["fleet"])
        #expect(deleted["moved"] as? Int == 3)
    }

    @Test("tandemErrorText renders each domain error actionably")
    func errorTextRendering() throws {
        #expect(tandemErrorText(DomainError.locked).contains("locked"))
        #expect(tandemErrorText(DomainError.emptyTitle).contains("Title is empty"))
        #expect(tandemErrorText(DomainError.invalidFilter("bogus")).contains("bogus"))

        let open = [Ticket(title: "the open one")]
        let unknown = tandemErrorText(DomainError.unknownID(id: "x", open: open))
        #expect(unknown.contains("x"))
        #expect(unknown.contains("the open one"), "unknownID text should carry the open list")

        let recovery = RecoveryFailure(underlying: DomainError.emptyTitle, note: "NOTE-XYZ")
        let recoveryText = tandemErrorText(recovery)
        #expect(recoveryText.contains("Title is empty"))
        #expect(recoveryText.contains("NOTE-XYZ"))
    }
}
