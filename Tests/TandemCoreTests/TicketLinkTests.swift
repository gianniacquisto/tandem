import Foundation
import Testing
@testable import TandemCore

/// `setLink` (Operations.swift) and the additive `link` field on `Ticket` — a ticket may
/// point at a Bikeshed note by bikeshed-root-relative path, GUI-only (ADR 0010).
@Suite("Ticket links")
struct TicketLinkTests {

    // MARK: - Helpers

    /// A unique, empty temp directory for one store. Callers clean it up with `defer`.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-link-\(UUID().uuidString)")
    }

    // MARK: - setLink

    @Test("setLink stores a trimmed bikeshed-relative path and it persists")
    func setLinkPersists() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Ship the bikeshed")
        let linked = try store.setLink(added.value.id.uuidString, to: "  projects/App.md  \n")
        #expect(linked.value.id == added.value.id)
        #expect(linked.value.link == "projects/App.md")

        // Re-read: the link is on disk, not just in the returned value.
        let persisted = try store.list(.all).value.first { $0.id == added.value.id }
        #expect(persisted?.link == "projects/App.md")
        #expect(persisted?.title == "Ship the bikeshed")
    }

    @Test("an empty or whitespace-only link unlinks the ticket")
    func emptyLinkClears() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        _ = try store.setLink(added.value.id.uuidString, to: "Ideas.md")
        #expect(try store.list(.all).value.first?.link == "Ideas.md")

        let cleared = try store.setLink(added.value.id.uuidString, to: "   \n")
        #expect(cleared.value.link == nil)
        #expect(try store.list(.all).value.first?.link == nil)
    }

    @Test("setLink to nil clears an existing link")
    func nilLinkClears() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        _ = try store.setLink(added.value.id.uuidString, to: "Ideas.md")

        let cleared = try store.setLink(added.value.id.uuidString, to: nil)
        #expect(cleared.value.link == nil)
        #expect(try store.list(.all).value.first?.link == nil)
    }

    @Test("setting the same link is a no-op: nothing is written")
    func unchangedLinkIsNoOp() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        _ = try store.setLink(added.value.id.uuidString, to: "Ideas.md")
        let fileURL = store.storeFileURL
        let before = try FileManager.default.attributesOfItem(atPath: fileURL.path)[.modificationDate] as? Date
        #expect(before != nil)

        let again = try store.setLink(added.value.id.uuidString, to: "Ideas.md")
        #expect(again.value.link == "Ideas.md")
        let after = try FileManager.default.attributesOfItem(atPath: fileURL.path)[.modificationDate] as? Date
        #expect(after == before)

        // An unchanged nil → nil is also a no-op.
        _ = try store.setLink(added.value.id.uuidString, to: nil)
        let unlinked = try store.setLink(added.value.id.uuidString, to: nil)
        #expect(unlinked.value.link == nil)
    }

    @Test("setLink with an unknown or non-UUID id throws unknownID")
    func unknownID() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        _ = try store.add("A")

        for id in [UUID().uuidString, "not-a-uuid"] {
            do {
                _ = try store.setLink(id, to: "Ideas.md")
                Issue.record("Expected unknownID to be thrown for \(id)")
            } catch let error as DomainError {
                guard case .unknownID = error else {
                    Issue.record("Expected unknownID, got \(error)")
                    return
                }
            }
        }
    }

    // MARK: - On disk (additive: the envelope stays version 1)

    @Test("the file reports version 1 with the link present and absent")
    func envelopeStaysVersionOne() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let added = try store.add("Task")
        _ = try store.setLink(added.value.id.uuidString, to: "Ideas.md")

        let fileURL = store.storeFileURL
        func readEntry() throws -> [String: Any]? {
            let text = try String(contentsOf: fileURL, encoding: .utf8)
            let obj = try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
            #expect((obj?["version"] as? Int) == 1)
            return (obj?["tickets"] as? [[String: Any]])?.first
        }

        // Link present: a real string.
        #expect((try readEntry())?["link"] as? String == "Ideas.md")

        // Link absent: JSON null, and the version is still 1.
        _ = try store.setLink(added.value.id.uuidString, to: nil)
        #expect((try readEntry())?["link"] is NSNull)

        // A fresh store over the same dir round-trips the nil link.
        let fresh = Store(directory: dir)
        #expect(try fresh.list(.all).value.first?.link == nil)
    }
}
