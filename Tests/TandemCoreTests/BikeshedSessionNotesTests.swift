import Foundation
import Testing
@testable import TandemCore

/// The notes a session wrote — the derived list the umbrella asks `session
/// show` to name (`projects/*/.raw/**/<session>/**`).
///
/// **Derived, not stored.** The session id is the folder a capture landed in,
/// and the consolidation pass moves what it consumes to `.raw/.done/<session>/`
/// — the same folder name in the archive. So the list is a scan of the file
/// set, and it cannot drift from the files.
///
/// Every test runs against a temp dir via `Bikeshed`'s explicit-directory
/// initialiser — no fixture bikeshed, no real `~/.tandem`.
@Suite("Bikeshed session notes")
struct BikeshedSessionNotesTests {

    /// A unique temp directory (not pre-created: a missing bikeshed folder is a
    /// valid empty bikeshed). Nested one level below a unique parent so the
    /// bikeshed's sibling lockfile is never shared between concurrent tests.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-session-notes-\(UUID().uuidString)")
            .appendingPathComponent("bikeshed")
    }

    /// Move a capture to the archive, the way the consolidation pass does.
    private func archive(_ path: String, in bikeshed: Bikeshed) throws {
        let url = try #require(BikeshedPath.url(under: bikeshed.directory, for: path))
        let done = url.deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent(".done")
            .appendingPathComponent(url.deletingLastPathComponent().lastPathComponent)
        try FileManager.default.createDirectory(at: done, withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: url, to: done.appendingPathComponent(url.lastPathComponent))
    }

    @Test("a session's notes are the captures filed under its folder")
    func notesAreTheCapturesUnderTheSessionFolder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let mine = try bikeshed.capture(
            project: "tandem", kind: .work, body: "# Mine\n", session: "ses_1").value
        let theirs = try bikeshed.capture(
            project: "tandem", kind: .work, body: "# Theirs\n", session: "ses_2").value
        _ = try bikeshed.capture(
            project: "fleet", kind: .gotcha, body: "# Elsewhere\n", session: "ses_1").value

        let notes = try bikeshed.sessionNotes(session: "ses_1").value
        #expect(notes.contains(mine))
        #expect(!notes.contains(theirs), "another session's capture is not this session's note")
        #expect(notes.count == 2, "one per project, both under the session's folder")
    }

    @Test("a consolidated capture is still the session's note, at its archived path")
    func aConsolidatedCaptureIsStillNamed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let pending = try bikeshed.capture(
            project: "tandem", kind: .work, body: "# Kept\n", session: "ses_1").value
        let consumed = try bikeshed.capture(
            project: "tandem", kind: .decision, body: "# Folded\n", session: "ses_1").value
        try archive(consumed, in: bikeshed)

        // The capture moved, so the path it is named by moved with it: the
        // list follows the files, it does not remember where they were.
        let archived = "projects/tandem/.raw/.done/ses_1/"
            + consumed.split(separator: "/").last.map(String.init)!

        let notes = try bikeshed.sessionNotes(session: "ses_1").value
        #expect(notes.contains(pending), "a pending capture is named")
        #expect(notes.contains(archived), "a consumed capture is named, at the path it moved to")
        #expect(!notes.contains(consumed), "the path it no longer lives at is not named")
        #expect(notes.count == 2)
    }

    @Test("a session that wrote nothing has no notes, and that is not an error")
    func anEmptySessionIsNotAnError() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        #expect(try bikeshed.sessionNotes(session: "ses_1").value.isEmpty)

        _ = try bikeshed.capture(
            project: "tandem", kind: .work, body: "# Mine\n", session: "ses_1").value
        #expect(try bikeshed.sessionNotes(session: "ses_other").value.isEmpty,
                "another session's notes are not this session's")
    }

    @Test("a missing bikeshed is an empty list, never an error")
    func aMissingBikeshedIsEmpty() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        #expect(try bikeshed.sessionNotes(session: "ses_1").value.isEmpty)
    }
}
