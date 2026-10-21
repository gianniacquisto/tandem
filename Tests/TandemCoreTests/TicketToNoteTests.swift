import Foundation
import Testing
@testable import TandemCore

/// Promoting a ticket to a bikeshed note (`TicketToNote`) — the note is made from
/// the ticket's title and description, the ticket's `link` points at it, and
/// nothing else about the ticket changes. The note is created, never
/// overwritten; a ticket that already links to a note is refused.
///
/// Every test runs against temp dirs — a store and a bikeshed side by side
/// under one unique parent, so the bikeshed's sibling lockfile is never shared.
@Suite("Ticket → note promotion")
struct TicketToNoteTests {

    // MARK: - Fixture

    /// A store and a bikeshed, each in its own directory under one unique
    /// parent: the bikeshed's lockfile is a *sibling* of the bikeshed folder
    /// (ADR 0004), the store's lives inside the store's own directory, so the
    /// two never contend for one lock.
    private struct Fixture {
        let root: URL
        let store: Store
        let bikeshed: Bikeshed
    }

    private func fixture() -> Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-promo-\(UUID().uuidString)")
        return Fixture(
            root: root,
            store: Store(directory: root.appendingPathComponent("store")),
            bikeshed: Bikeshed(directory: root.appendingPathComponent("bikeshed")))
    }

    private func clean(_ f: Fixture) {
        try? FileManager.default.removeItem(at: f.root)
    }

    /// The promotion under test.
    private func promote(_ f: Fixture, _ id: String) throws -> OperationResult<PromotedNote> {
        try TicketToNote(store: f.store, bikeshed: f.bikeshed).promote(id)
    }

    /// Writes a file straight into the bikeshed, bypassing the core — the way a
    /// note the human wrote by hand looks to the promotion.
    private func stage(_ relativePath: String, _ text: String, in f: Fixture) throws {
        let url = f.bikeshed.directory.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    // MARK: - The promotion

    @Test("promotion writes the note, links the ticket, and changes nothing else")
    func promotesToNote() throws {
        let f = fixture()
        defer { clean(f) }
        _ = try f.store.addLane("Cycle")
        let added = try f.store.add("Ship the bikeshed", lane: "Cycle")
        let id = added.value.id.uuidString
        _ = try f.store.set(id, description: .set("The note body."))
        _ = try f.store.set(id, status: .set(.inProgress))

        let promoted = try promote(f, id)

        // The note lands under the lane, named after the title.
        #expect(promoted.value.path == "projects/cycle/ship-the-bikeshed.md")

        // Its bytes: the title as the heading, the description as the body. The
        // ticket's own state is not written into the note.
        let note = try f.bikeshed.read(path: promoted.value.path)
        #expect(note.value.text == "# Ship the bikeshed\n\nThe note body.\n")

        // The ticket, as the store now holds it: linked, and otherwise untouched —
        // same title, body, status, and lane.
        let stored = try f.store.list(.all).value.first { $0.id == added.value.id }
        #expect(stored?.link == "projects/cycle/ship-the-bikeshed.md")
        #expect(stored?.title == "Ship the bikeshed")
        #expect(stored?.description == "The note body.")
        #expect(stored?.status == .inProgress)
        #expect(stored?.lane == "Cycle")
    }

    @Test("a ticket with no lane lands in projects/uncategorized")
    func noLaneLandsInUncategorized() throws {
        let f = fixture()
        defer { clean(f) }
        let added = try f.store.add("No lane here")

        let promoted = try promote(f, added.value.id.uuidString)

        #expect(promoted.value.path == "projects/uncategorized/no-lane-here.md")
        #expect(try f.bikeshed.read(path: promoted.value.path).value.text == "# No lane here\n")
    }

    @Test("the note's name is a slug of the title")
    func slugRules() throws {
        let f = fixture()
        defer { clean(f) }
        let added = try f.store.add("Fix the / thing: v2!")

        let promoted = try promote(f, added.value.id.uuidString)

        #expect(promoted.value.path == "projects/uncategorized/fix-the-thing-v2.md")
    }

    // MARK: - Never overwriting

    @Test("a taken name gets a -2 suffix; the first note is untouched")
    func collisionSuffixesTheName() throws {
        let f = fixture()
        defer { clean(f) }
        let first = try f.store.add("Same title")
        let second = try f.store.add("Same title")

        let one = try promote(f, first.value.id.uuidString)
        let two = try promote(f, second.value.id.uuidString)

        #expect(one.value.path == "projects/uncategorized/same-title.md")
        #expect(two.value.path == "projects/uncategorized/same-title-2.md")
        #expect(try f.bikeshed.read(path: one.value.path).value.text == "# Same title\n")
        #expect(try f.bikeshed.read(path: two.value.path).value.text == "# Same title\n")
        #expect(f.bikeshed.notePaths().count == 2)
    }

    @Test("a note the human wrote by hand is never overwritten")
    func existingNoteIsNotOverwritten() throws {
        let f = fixture()
        defer { clean(f) }
        try stage("projects/uncategorized/ship-it.md", "# Ship it\n\nMy own prose.\n", in: f)
        let added = try f.store.add("Ship it")

        let promoted = try promote(f, added.value.id.uuidString)

        // The promotion filed itself beside the note, not over it.
        #expect(promoted.value.path == "projects/uncategorized/ship-it-2.md")
        #expect(try f.bikeshed.read(path: "projects/uncategorized/ship-it.md").value.text
                == "# Ship it\n\nMy own prose.\n")
        #expect(try f.bikeshed.read(path: promoted.value.path).value.text == "# Ship it\n")
    }

    // MARK: - Refusing a second promotion

    @Test("promoting a linked ticket again is refused, and the note is left alone")
    func refusesSecondPromotion() throws {
        let f = fixture()
        defer { clean(f) }
        let added = try f.store.add("Write the ADR")
        let id = added.value.id.uuidString
        let first = try promote(f, id)
        let before = try f.bikeshed.read(path: first.value.path).value

        do {
            _ = try promote(f, id)
            Issue.record("Expected the second promotion to be refused")
        } catch let error as PromotionError {
            #expect(error == .alreadyLinked(path: first.value.path))
        }

        // Nothing was written: the note is the same, and no second note appeared.
        #expect(try f.bikeshed.read(path: first.value.path).value == before)
        #expect(f.bikeshed.notePaths().count == 1)
        #expect(try f.store.list(.all).value.first?.link == first.value.path)
    }

    @Test("the refusal names the note the ticket already points at")
    func refusalMessageNamesTheNote() throws {
        let f = fixture()
        defer { clean(f) }
        let added = try f.store.add("Write the ADR")
        let id = added.value.id.uuidString
        let first = try promote(f, id)

        do {
            _ = try promote(f, id)
            Issue.record("Expected the second promotion to be refused")
        } catch let error as PromotionError {
            #expect(error.message.contains(first.value.path))
        }
    }

    // MARK: - The link round-trips

    @Test("the ticket's link reads the note back")
    func linkRoundTrip() throws {
        let f = fixture()
        defer { clean(f) }
        let added = try f.store.add("Round trip")
        let id = added.value.id.uuidString
        _ = try f.store.set(id, description: .set("Body."))
        _ = try promote(f, id)

        // The link the store holds is a bikeshed path: reading it back gives the
        // note the promotion wrote.
        let link = try f.store.list(.all).value.first { $0.id == added.value.id }?.link
        #expect(link != nil)
        let note = try f.bikeshed.read(path: link!)
        #expect(note.value.text == "# Round trip\n\nBody.\n")
    }

    @Test("promoting an unknown id throws unknownID")
    func unknownIDThrows() throws {
        let f = fixture()
        defer { clean(f) }
        _ = try f.store.add("A ticket")

        for id in [UUID().uuidString, "not-a-uuid"] {
            do {
                _ = try promote(f, id)
                Issue.record("Expected unknownID for \(id)")
            } catch let error as DomainError {
                guard case .unknownID = error else {
                    Issue.record("Expected unknownID, got \(error)")
                    return
                }
            }
        }
        #expect(f.bikeshed.notePaths().isEmpty)
    }
}
