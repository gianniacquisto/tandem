import Foundation
import Testing
@testable import TandemCore

/// Corrupt-store recovery (ADR 0001) and result-encoding tests.
///
/// Recovery: an invalid file is backed up (under the lock) to
/// `todos.json.corrupt-<unix-ts>` and the store restarts empty; only the op that
/// performs the recovery surfaces a `note` naming the backup path. Lenient read:
/// unknown extra fields are ignored. Encoding: `StoreEncoding` + `tandemErrorText`
/// produce the exact schema-v1 shapes and actionable error text the MCP surface sends.
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
        try text.write(to: store.fileURL, atomically: false, encoding: .utf8)
    }

    /// Backup filenames present in the store dir (`todos.json.corrupt-*`).
    private func corruptBackups(in dir: URL) -> [String] {
        guard let entries = try? FileManager.default.contentsOfDirectory(atPath: dir.path) else {
            return []
        }
        return entries.filter { $0.hasPrefix("todos.json.corrupt-") }.sorted()
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

    @Test("an unknown version is recovered")
    func unknownVersionRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        try writeRaw(#"{"version":2,"todos":[]}"#, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "unknown version should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
    }

    @Test("a missing required field is recovered")
    func missingFieldRecovered() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        // No "done" field.
        let raw = #"{"version":1,"todos":[{"id":"\#(validID)","title":"x","createdAt":"\#(validTS)","completedAt":null}]}"#
        try writeRaw(raw, to: store)

        let result = try store.list(.all)
        #expect(result.note != nil, "missing required field should be recovered")
        #expect(!corruptBackups(in: dir).isEmpty)
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
        let todo = Todo(title: "one")
        // No note → no note key.
        let noNote = try json(StoreEncoding.listResult([todo], note: nil))
        #expect((noNote["todos"] as? [Any])?.count == 1)
        #expect(noNote["note"] == nil, "note key must be absent when note is nil")
        // With note → top-level note.
        let withNote = try json(StoreEncoding.listResult([todo], note: "NOTE"))
        #expect(withNote["note"] as? String == "NOTE")
        // Empty → {"todos":[]}.
        let empty = try json(StoreEncoding.listResult([], note: nil))
        #expect((empty["todos"] as? [Any])?.isEmpty == true)
    }

    @Test("StoreEncoding.todoResult is the todo object plus optional note")
    func todoResultEncoding() throws {
        let todo = Todo(title: "thing")
        let obj = try json(StoreEncoding.todoResult(todo, note: nil))
        #expect(obj["id"] as? String == todo.id.uuidString)
        #expect(obj["title"] as? String == "thing")
        #expect(obj["done"] as? Bool == false)
        #expect(obj["createdAt"] is String)
        #expect(obj["completedAt"] is NSNull, "completedAt should be null while open")
        #expect(obj["note"] == nil)
        let withNote = try json(StoreEncoding.todoResult(todo, note: "N"))
        #expect(withNote["note"] as? String == "N")
    }

    @Test("StoreEncoding.openList is the list shape")
    func openListEncoding() throws {
        let todo = Todo(title: "open one")
        let obj = try json(StoreEncoding.openList([todo]))
        #expect((obj["todos"] as? [Any])?.count == 1)
        #expect(obj["note"] == nil)
    }

    @Test("tandemErrorText renders each domain error actionably")
    func errorTextRendering() throws {
        #expect(tandemErrorText(DomainError.locked).contains("locked"))
        #expect(tandemErrorText(DomainError.emptyTitle).contains("Title is empty"))
        #expect(tandemErrorText(DomainError.invalidFilter("bogus")).contains("bogus"))

        let open = [Todo(title: "the open one")]
        let unknown = tandemErrorText(DomainError.unknownID(id: "x", open: open))
        #expect(unknown.contains("x"))
        #expect(unknown.contains("the open one"), "unknownID text should carry the open list")

        let recovery = RecoveryFailure(underlying: DomainError.emptyTitle, note: "NOTE-XYZ")
        let recoveryText = tandemErrorText(recovery)
        #expect(recoveryText.contains("Title is empty"))
        #expect(recoveryText.contains("NOTE-XYZ"))
    }
}
