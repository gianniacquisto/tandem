import Foundation
import Testing
@testable import TandemCore

/// `addFromNote` (NoteToTicket.swift) — the reverse of the ticket→note link: a
/// selection in a note becomes a ticket, and the ticket points back at the note.
///
/// The rules under test: the title is the selection's first line (trimmed, and
/// without a checkbox's syntax), the description is the selection, the link is
/// the note's bikeshed-relative path, an empty selection is rejected, and the
/// note itself is never touched.
@Suite("Note to ticket")
struct NoteToTicketTests {

    // MARK: - Helpers

    /// A unique, empty temp directory for one store. Callers clean it up with `defer`.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-note-to-todo-\(UUID().uuidString)")
    }

    // MARK: - The ticket a selection makes

    @Test("a selection becomes a ticket: the first line is the title, the selection the description")
    func selectionMakesTicket() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.addFromNote(
            "Ship the bikeshed\n\nIt is late, but it is the last thing.",
            notePath: "projects/App.md")

        #expect(added.value.title == "Ship the bikeshed")
        #expect(added.value.description == "Ship the bikeshed\n\nIt is late, but it is the last thing.")
        #expect(added.value.link == "projects/App.md")
        // Born in the default lane, in backlog, filed by the human.
        #expect(added.value.lane == nil)
        #expect(added.value.status == .backlog)
        #expect(added.value.origin == "human")
    }

    @Test("the title is trimmed")
    func titleIsTrimmed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.addFromNote("   Fix the flaky test   \nmore detail", notePath: "notes/Ideas.md")
        #expect(added.value.title == "Fix the flaky test")
    }

    @Test("a selection that starts with a blank line is titled after its first real line")
    func blankFirstLineIsSkipped() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.addFromNote("\n\nWrite the ADR", notePath: "notes/Ideas.md")
        #expect(added.value.title == "Write the ADR")
    }

    @Test("a checkbox line is titled with its text, not its syntax")
    func checkboxLineIsTitled() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        for marker in ["- [ ] ", "- [x] ", "* [ ] ", "+ [ ] ", "1. [ ] "] {
            let added = try store.addFromNote("\(marker)Fix the flaky test", notePath: "notes/Ideas.md")
            #expect(added.value.title == "Fix the flaky test", "marker: \(marker)")
            // The description keeps the line as written — the note's syntax stays
            // in the note, and the ticket shows what was sent.
            #expect(added.value.description == "\(marker)Fix the flaky test")
        }
    }

    @Test("a line that is not a task item keeps its text")
    func plainLineIsUntouched() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        // A dash that is not a task marker, and a bracket that is not one either.
        let dash = try store.addFromNote("- not a task", notePath: "notes/Ideas.md")
        #expect(dash.value.title == "- not a task")
        let bracket = try store.addFromNote("[draft] not a task", notePath: "notes/Ideas.md")
        #expect(bracket.value.title == "[draft] not a task")
    }

    // MARK: - Rejections

    @Test("an empty or whitespace-only selection is rejected")
    func emptySelectionIsRejected() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        for selection in ["", "   ", "\n\n", " \t "] {
            do {
                _ = try store.addFromNote(selection, notePath: "notes/Ideas.md")
                Issue.record("Expected emptySelection for \(selection.debugDescription)")
            } catch let error as DomainError {
                guard case .emptySelection = error else {
                    Issue.record("Expected emptySelection, got \(error)")
                    return
                }
            }
        }
        // Nothing was filed.
        #expect(try store.list(.all).value.isEmpty)
    }

    @Test("a note path that is not bikeshed-relative is rejected")
    func invalidNotePathIsRejected() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        for path in ["", "  ", "/notes/Ideas.md", "../escape.md", "notes//Ideas.md"] {
            do {
                _ = try store.addFromNote("A title", notePath: path)
                Issue.record("Expected invalidNotePath for \(path.debugDescription)")
            } catch let error as DomainError {
                guard case .invalidNotePath = error else {
                    Issue.record("Expected invalidNotePath, got \(error)")
                    return
                }
            }
        }
        #expect(try store.list(.all).value.isEmpty)
    }

    // MARK: - The link, on disk

    @Test("the link is the bikeshed-relative path, and it persists")
    func linkPersists() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.addFromNote("Ship it", notePath: "projects/tandem/App.md")
        #expect(added.value.link == "projects/tandem/App.md")

        // Re-read: the link is on disk, not just in the returned value.
        let persisted = try store.list(.all).value.first { $0.id == added.value.id }
        #expect(persisted?.link == "projects/tandem/App.md")
        #expect(persisted?.description == "Ship it")
    }

    @Test("the ticket lands on top of the default lane, in backlog")
    func landsOnTop() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("An older ticket")
        let added = try store.addFromNote("A newer one", notePath: "notes/Ideas.md")

        let todos = try store.list(.all).value
        #expect(todos.first?.id == added.value.id)
        #expect(todos.map(\.title) == ["A newer one", "An older ticket"])
    }

    @Test("the ticket files into the lane it is given")
    func filesIntoTheGivenLane() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.addLane("Tandem")
        let added = try store.addFromNote("A lane ticket", notePath: "notes/Ideas.md", lane: "Tandem")
        #expect(added.value.lane == "Tandem")

        // An unknown lane is rejected, like every other lane argument.
        do {
            _ = try store.addFromNote("A lane ticket", notePath: "notes/Ideas.md", lane: "Nope")
            Issue.record("Expected unknownLane")
        } catch let error as DomainError {
            guard case .unknownLane = error else {
                Issue.record("Expected unknownLane, got \(error)")
                return
            }
        }
    }

    // MARK: - The note is not modified

    @Test("the note is not modified: the file's bytes are untouched")
    func noteIsNotModified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let bikeshed = Bikeshed(directory: dir.appendingPathComponent("bikeshed"))

        let text = "- [ ] Ship the bikeshed\n\nSome prose.\n"
        _ = try bikeshed.create(path: "projects/App.md")
        _ = try bikeshed.write(path: "projects/App.md", text: text)
        let noteURL = dir.appendingPathComponent("bikeshed/projects/App.md")
        let before = try Data(contentsOf: noteURL)

        _ = try store.addFromNote("Ship the bikeshed", notePath: "projects/App.md")

        let after = try Data(contentsOf: noteURL)
        #expect(after == before)
        #expect(try String(contentsOf: noteURL, encoding: .utf8) == text)
    }
}
