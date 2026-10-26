import Darwin
import Foundation
import Testing
@testable import TandemCore

/// The sprints file (spec `4F746EFE`, R1): `sprints.json` beside
/// `todos.json`, its record, and its operations — `open`, `ticket`, `run`,
/// `close`, `list`, `show`. A record is created once, appended to, and stamped
/// once; it is a fact about a sprint that ran, not a log line. The pair
/// (lane, number) is the identity — there is no UUID on this record.
///
/// The suite runs against an explicit directory, never `TandemHome`: a test
/// that reached the human's real `~/.tandem` would write a sprint record into
/// their home. The store the `ticket` operation validates against lives in a
/// *separate* directory, so the sprints directory's exact file set —
/// `sprints.json` and `sprints.lock`, nothing else — stays provable.
@Suite("Sprints file")
struct SprintsTests {

    /// A sprints file and its directory, plus a store in its own directory
    /// (empty unless the test adds tickets to it).
    private func tempSprints() throws -> (Sprints, Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sprints-tests-\(UUID().uuidString)")
        let storeDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sprints-store-tests-\(UUID().uuidString)")
        return (Sprints(directory: dir), Store(directory: storeDir), dir)
    }

    private func file(_ dir: URL) -> URL {
        dir.appendingPathComponent("sprints.json")
    }

    /// Write a sprints file by hand: the envelope and the records, exactly as
    /// the core writes them. The timestamps are the file's own, so the list's
    /// order is the file's truth rather than the clock's.
    private func writeFile(_ dir: URL, _ records: [[String: Any]]) throws {
        let envelope: [String: Any] = ["version": 1, "sprints": records]
        let data = try JSONSerialization.data(
            withJSONObject: envelope, options: [.prettyPrinted, .sortedKeys])
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try data.write(to: file(dir))
    }

    /// One hand-written record. `openedAt` and `closedAt` are the file's own
    /// ISO-8601 UTC stamps.
    private func record(
        lane: String,
        number: Int,
        status: String,
        mode: String = "serial",
        session: String? = nil,
        batch: [String] = [],
        runs: [String] = [],
        close: String? = nil,
        openedAt: String,
        closedAt: String? = nil,
        extra: [String: Any] = [:]
    ) -> [String: Any] {
        var record: [String: Any] = [
            "lane": lane,
            "number": number,
            "session": session ?? NSNull(),
            "mode": mode,
            "batch": batch,
            "runs": runs,
            "close": close ?? NSNull(),
            "status": status,
            "openedAt": openedAt,
            "closedAt": closedAt ?? NSNull(),
        ]
        for (key, value) in extra { record[key] = value }
        return record
    }

    // MARK: - open

    @Test("the first open creates sprints.json in the given directory, and nothing else")
    func firstOpenCreatesTheFile() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        let fm = FileManager.default
        #expect(!fm.fileExists(atPath: file(dir).path))

        let result = try sprints.open(lane: "Tandem", mode: .parallel, session: "ses_1")
        let record = result.value
        #expect(record.lane == "Tandem")
        #expect(record.number == 1)
        #expect(record.session == "ses_1")
        #expect(record.mode == .parallel)
        #expect(record.status == .open)
        #expect(record.batch.isEmpty)
        #expect(record.runs.isEmpty)
        #expect(record.close == nil)
        #expect(record.closedAt == nil)
        #expect(record.name == "Sprint 1 — Tandem")
        #expect(result.note == nil)

        // The database, and no legacy file, no lockfile, no temp file.
        let names = try fm.contentsOfDirectory(atPath: dir.path).sorted()
        #expect(names.contains("tandem.db"))
        #expect(!names.contains("sprints.json"))
        #expect(!names.contains("sprints.lock"))
    }

    @Test("a second open on a lane with an open sprint returns the record unchanged, and the file's bytes do not move")
    func secondOpenDoesNotForkTheSprint() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        let first = try sprints.open(lane: "Tandem", mode: .serial, session: "ses_1")
        let before = try sprints.list().value

        // A different mode and session: the open record stands, either way.
        let second = try sprints.open(lane: "Tandem", mode: .parallel, session: "ses_9")
        #expect(second.value == first.value)
        #expect(second.note == "Sprint 1 — Tandem is already open")
        #expect(try sprints.list().value == before)

        // One record, and it is the first one — the second open changed nothing.
        let listed = try sprints.list().value
        #expect(listed.count == 1)
        #expect(listed.first?.mode == .serial)
        #expect(listed.first?.session == "ses_1")
    }

    @Test("a lane whose sprints are all closed opens a new one, numbered after the lane's highest")
    func openAfterCloseNumbersUpward() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")
        _ = try sprints.close(lane: "Tandem", number: 1, note: "Landed.")

        let second = try sprints.open(lane: "Tandem")
        #expect(second.value.number == 2)
        #expect(second.value.status == .open)
        #expect(second.note == nil)
        // The closed one keeps its number and its shape.
        let shown = try sprints.show(lane: "Tandem", number: 1).value
        #expect(shown.status == .closed)
        #expect(shown.close == "Landed.")
    }

    @Test("numbering is per-lane: two lanes both start at 1")
    func numberingIsPerLane() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")
        _ = try sprints.open(lane: "code-rank")

        #expect(try sprints.show(lane: "Tandem", number: 1).value.lane == "Tandem")
        #expect(try sprints.show(lane: "code-rank", number: 1).value.lane == "code-rank")
        #expect(try sprints.list().value.count == 2)
    }

    @Test("an empty or whitespace-only lane is refused, and nothing is written")
    func emptyLaneRefused() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }

        for lane in ["", "   ", "\n  \n"] {
            do {
                _ = try sprints.open(lane: lane)
                Issue.record("expected emptyLaneName for lane \(lane.debugDescription)")
            } catch DomainError.emptyLaneName {
                // The domain failure, not a usage one.
            }
        }
        #expect(!FileManager.default.fileExists(atPath: file(dir).path))
        #expect(try sprints.list().value.isEmpty)

        // The other lane-taking operations refuse it too, rather than
        // reporting an empty lane as an unknown sprint.
        do {
            _ = try sprints.show(lane: "", number: 1)
            Issue.record("expected emptyLaneName")
        } catch DomainError.emptyLaneName {
        }
        do {
            _ = try sprints.run(lane: "  ", number: 1, note: "Landed a thing.")
            Issue.record("expected emptyLaneName")
        } catch DomainError.emptyLaneName {
        }
        do {
            _ = try sprints.close(lane: "\n", number: 1)
            Issue.record("expected emptyLaneName")
        } catch DomainError.emptyLaneName {
        }
        do {
            _ = try sprints.ticket(lane: "", number: 1, ticketID: UUID(), on: Store(directory: dir))
            Issue.record("expected emptyLaneName")
        } catch DomainError.emptyLaneName {
        }
    }

    @Test("the lane is trimmed, and the trimmed lane is the one stored")
    func laneIsTrimmed() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }

        let opened = try sprints.open(lane: "  Tandem  ")
        #expect(opened.value.lane == "Tandem")
        // The same lane, however it is spelled, reaches the same record.
        #expect(try sprints.show(lane: " Tandem ", number: 1).value.lane == "Tandem")
        // And a second open under the padded spelling does not fork the sprint.
        let again = try sprints.open(lane: " Tandem ")
        #expect(again.note == "Sprint 1 — Tandem is already open")
        #expect(try sprints.list().value.count == 1)
    }

    @Test("a nil or empty opening session is no session on the record")
    func emptySessionReadsAsAbsent() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }

        let opened = try sprints.open(lane: "Tandem", session: "  ")
        #expect(opened.value.session == nil)
        // The row carries the null, not an empty string.
        #expect(try sprints.show(lane: "Tandem", number: 1).value.session == nil)
    }

    // MARK: - ticket

    @Test("ticket appends to the batch once, keeps the stored order, and round-trips the file")
    func ticketDedupesAndKeepsOrder() throws {
        let (sprints, store, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.removeItem(at: store.directory) }
        _ = try sprints.open(lane: "Tandem")
        let first = try store.add("First ticket.").value.id
        let second = try store.add("Second ticket.").value.id
        let third = try store.add("Third ticket.").value.id

        _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: first, on: store)
        _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: second, on: store)
        let dupe = try sprints.ticket(lane: "Tandem", number: 1, ticketID: first, on: store)
        #expect(dupe.value.batch == [first, second])
        #expect(dupe.note == nil)

        let added = try sprints.ticket(lane: "Tandem", number: 1, ticketID: third, on: store)
        #expect(added.value.batch == [first, second, third])

        // And the order survives the round trip through the file.
        let shown = try sprints.show(lane: "Tandem", number: 1).value
        #expect(shown.batch == [first, second, third])
    }

    @Test("a ticket id the store does not carry is refused, and nothing is written")
    func unknownTicketIDRefused() throws {
        let (sprints, store, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.removeItem(at: store.directory) }
        _ = try sprints.open(lane: "Tandem")
        let onBoard = try store.add("A real ticket.").value.id
        _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: onBoard, on: store)
        let before = try sprints.list().value

        let stranger = UUID()
        do {
            _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: stranger, on: store)
            Issue.record("expected unknownTicketID")
        } catch let DomainError.unknownTicketID(id) {
            #expect(id == stranger.uuidString)
        }

        // The refusal wrote nothing: the record did not move, and the batch
        // holds only the ticket the store carries.
        #expect(try sprints.list().value == before)
        #expect(try sprints.show(lane: "Tandem", number: 1).value.batch == [onBoard])

        // The failure the human sees says the ticket is not on the board.
        let text = tandemErrorText(DomainError.unknownTicketID(stranger.uuidString))
        #expect(text.contains(stranger.uuidString))
        #expect(text.contains("not on the board"))
    }

    @Test("a ticket deleted after the write is not an error — the batch keeps the id")
    func deletedTicketIsNotAnError() throws {
        let (sprints, store, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.removeItem(at: store.directory) }
        _ = try sprints.open(lane: "Tandem")
        let added = try store.add("Here today.").value.id
        _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: added, on: store)
        // Off the board — the record keeps the id, and every read succeeds.
        _ = try store.delete(added.uuidString)

        let shown = try sprints.show(lane: "Tandem", number: 1).value
        #expect(shown.batch == [added])
        #expect(try sprints.list().value.first?.batch == [added])
    }

    // MARK: - run

    @Test("run appends one dated entry per call, in call order")
    func runAppendsDatedEntries() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")

        let first = try sprints.run(lane: "Tandem", number: 1, note: "  Landed the record.  ")
        #expect(first.value.runs.count == 1)
        #expect(first.note == nil)

        _ = try sprints.run(lane: "Tandem", number: 1, note: "Landed the CLI.")
        let shown = try sprints.show(lane: "Tandem", number: 1).value
        #expect(shown.runs.count == 2)
        // Each entry is dated by the core: the ISO-8601 stamp, then the note.
        let entry = try #require(shown.runs.first)
        #expect(entry.hasSuffix(" — Landed the record."))
        let stamp = String(try #require(entry.split(separator: "—", maxSplits: 1).first))
            .trimmingCharacters(in: .whitespaces)
        #expect(stamp.hasSuffix("Z"))
        #expect(Timestamp.decode(stamp) != nil)
        #expect(shown.runs[1].hasSuffix(" — Landed the CLI."))
    }

    @Test("an empty or whitespace-only run note is refused, and nothing is written")
    func emptyRunNoteRefused() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")
        _ = try sprints.run(lane: "Tandem", number: 1, note: "Landed a thing.")
        let before = try sprints.list().value

        for note in ["", "   ", "\n  \n"] {
            do {
                _ = try sprints.run(lane: "Tandem", number: 1, note: note)
                Issue.record("expected emptySprintNote for note \(note.debugDescription)")
            } catch DomainError.emptySprintNote {
                // The domain failure, not a usage one.
            }
        }
        #expect(try sprints.list().value == before)
        #expect(try sprints.show(lane: "Tandem", number: 1).value.runs.count == 1)
    }

    // MARK: - close

    @Test("close stamps the record once; a second close keeps the first text and stamp")
    func closeStampsOnce() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")

        let closed = try sprints.close(
            lane: "Tandem", number: 1, note: "  The record landed; the CLI waits.  ")
        #expect(closed.value.status == .closed)
        #expect(closed.value.close == "The record landed; the CLI waits.")
        #expect(closed.value.closedAt != nil)
        #expect(closed.note == nil)

        let again = try sprints.close(lane: "Tandem", number: 1, note: "A different close.")
        #expect(again.value.status == .closed)
        #expect(again.value.close == "The record landed; the CLI waits.")
        #expect(again.value.closedAt == closed.value.closedAt)
        #expect(again.note == "Sprint 1 — Tandem is already closed")

        // And the file carries the first close, not the second.
        let shown = try sprints.show(lane: "Tandem", number: 1).value
        #expect(shown.close == "The record landed; the CLI waits.")
        #expect(shown.status == .closed)
    }

    @Test("a close with no note stamps the close alone; an empty close note is refused")
    func closeNoteIsOptionalButNotEmpty() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")

        // No note at all: the stamp alone is a close.
        let stamped = try sprints.close(lane: "Tandem", number: 1)
        #expect(stamped.value.status == .closed)
        #expect(stamped.value.close == nil)
        #expect(stamped.value.closedAt != nil)

        // An empty *body* is a domain failure, like a comment's empty body —
        // on a second lane, where the close is still open.
        _ = try sprints.open(lane: "code-rank")
        do {
            _ = try sprints.close(lane: "code-rank", number: 1, note: "   ")
            Issue.record("expected emptySprintNote")
        } catch DomainError.emptySprintNote {
        }
        #expect(try sprints.show(lane: "code-rank", number: 1).value.status == .open)
    }

    // MARK: - list

    @Test("list is newest-opened first, and filters by lane and status")
    func listIsNewestFirstAndFilters() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(lane: "Tandem", number: 1, status: "closed",
                   close: "Landed.", openedAt: "2026-10-01T10:00:00Z",
                   closedAt: "2026-10-01T11:00:00Z"),
            record(lane: "code-rank", number: 1, status: "open",
                   openedAt: "2026-10-02T10:00:00Z"),
            record(lane: "Tandem", number: 2, status: "open",
                   openedAt: "2026-10-03T10:00:00Z"),
        ])

        let all = try sprints.list().value
        #expect(all.map { "\($0.lane)#\($0.number)" } == ["Tandem#2", "code-rank#1", "Tandem#1"])

        let tandem = try sprints.list(lane: "Tandem").value
        #expect(tandem.map(\.number) == [2, 1])
        let codeRank = try sprints.list(lane: "code-rank").value
        #expect(codeRank.map(\.number) == [1])

        let open = try sprints.list(status: .open).value
        #expect(open.map { "\($0.lane)#\($0.number)" } == ["Tandem#2", "code-rank#1"])
        let closed = try sprints.list(status: .closed).value
        #expect(closed.map(\.number) == [1])

        // Both filters together.
        let both = try sprints.list(lane: "Tandem", status: .closed).value
        #expect(both.map(\.number) == [1])
    }

    @Test("records that share an openedAt keep the order they were recorded in")
    func sharedTimestampsKeepStoredOrder() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(lane: "Tandem", number: 1, status: "open",
                   openedAt: "2026-10-03T10:00:00Z"),
            record(lane: "code-rank", number: 1, status: "open",
                   openedAt: "2026-10-03T10:00:00Z"),
        ])

        let listed = try sprints.list().value
        #expect(listed.map(\.lane) == ["Tandem", "code-rank"])
    }

    @Test("a record read from the file keeps every field it was written with")
    func readBackKeepsTheFields() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = UUID()
        try writeFile(dir, [
            record(lane: "Tandem", number: 3, status: "closed", mode: "parallel",
                   session: "ses_1", batch: [ticket.uuidString],
                   runs: ["2026-10-03T10:30:00Z — Landed the record."],
                   close: "The record landed.", openedAt: "2026-10-03T10:00:00Z",
                   closedAt: "2026-10-03T11:00:00Z"),
        ])

        let shown = try sprints.show(lane: "Tandem", number: 3).value
        #expect(shown.lane == "Tandem")
        #expect(shown.number == 3)
        #expect(shown.session == "ses_1")
        #expect(shown.mode == .parallel)
        #expect(shown.batch == [ticket])
        #expect(shown.runs == ["2026-10-03T10:30:00Z — Landed the record."])
        #expect(shown.close == "The record landed.")
        #expect(shown.status == .closed)
        #expect(Timestamp.encode(shown.openedAt) == "2026-10-03T10:00:00Z")
        #expect(Timestamp.encode(try #require(shown.closedAt)) == "2026-10-03T11:00:00Z")
        #expect(shown.name == "Sprint 3 — Tandem")
    }

    // MARK: - show

    @Test("an unknown (lane, number) fails with the numbers the lane holds")
    func unknownSprintThrows() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sprints.open(lane: "Tandem")
        // A lane holds one open sprint, so the second record needs the first
        // closed: open and closed alike count toward the lane's numbers.
        _ = try sprints.close(lane: "Tandem", number: 1)
        _ = try sprints.open(lane: "Tandem")   // Sprint 2 — Tandem

        // An unknown number in a known lane.
        do {
            _ = try sprints.show(lane: "Tandem", number: 3)
            Issue.record("expected unknownSprint")
        } catch let DomainError.unknownSprint(lane, number, known) {
            #expect(lane == "Tandem")
            #expect(number == 3)
            #expect(known == [1, 2])
        }
        // A known number in an unknown lane.
        do {
            _ = try sprints.show(lane: "fleet", number: 1)
            Issue.record("expected unknownSprint")
        } catch let DomainError.unknownSprint(lane, number, known) {
            #expect(lane == "fleet")
            #expect(number == 1)
            #expect(known.isEmpty)
        }

        // The mutating operations refuse it too.
        let store = Store(directory: dir.appendingPathComponent("unused-store"))
        do {
            _ = try sprints.ticket(lane: "Tandem", number: 9, ticketID: UUID(), on: store)
            Issue.record("expected unknownSprint")
        } catch DomainError.unknownSprint {
        }
        do {
            _ = try sprints.run(lane: "Tandem", number: 9, note: "Landed.")
            Issue.record("expected unknownSprint")
        } catch DomainError.unknownSprint {
        }
        do {
            _ = try sprints.close(lane: "Tandem", number: 9)
            Issue.record("expected unknownSprint")
        } catch DomainError.unknownSprint {
        }
    }

    // MARK: - The file's shape

    @Test("the record round-trips through the database with every field")
    func recordRoundTripsThroughTheDatabase() throws {
        let (sprints, store, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.removeItem(at: store.directory) }
        let ticket = try store.add("The batch's one ticket.").value.id
        _ = try sprints.open(lane: "  Tandem  ", mode: .parallel, session: "ses_1")
        _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: ticket, on: store)
        _ = try sprints.run(lane: "Tandem", number: 1, note: "Landed the record.")
        _ = try sprints.close(lane: "Tandem", number: 1, note: "The record landed.")

        let record = try sprints.show(lane: "Tandem", number: 1).value
        #expect(record.lane == "Tandem")
        #expect(record.number == 1)
        #expect(record.session == "ses_1")
        #expect(record.mode == .parallel)
        #expect(record.batch == [ticket])
        #expect(record.runs.count == 1)
        #expect(record.close == "The record landed.")
        #expect(record.status == .closed)
        #expect(record.closedAt != nil)
    }

    @Test("an unknown key is ignored on import and never reaches the database row")
    func unknownKeysAreIgnoredOnImport() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(lane: "Tandem", number: 1, status: "open",
                   openedAt: "2026-10-03T10:00:00Z",
                   extra: ["aKeyThisBuildDoesNotKnow": 42]),
        ])

        // Lenient read: the unknown key does not make the file invalid.
        #expect(try sprints.list().value.map(\.number) == [1])

        // The first write imports the record this build knows; the unknown key
        // has no column and is not carried.
        _ = try sprints.run(lane: "Tandem", number: 1, note: "Landed a thing.")
        #expect(try sprints.show(lane: "Tandem", number: 1).value.runs.count == 1)
    }

    @Test("a missing required key invalidates the file")
    func missingRequiredKeyInvalidatesTheFile() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            ["lane": "Tandem", "number": 1, "mode": "serial", "batch": [],
             "status": "open", "openedAt": "2026-10-03T10:00:00Z"],  // no runs
        ])

        let listed = try sprints.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
    }

    @Test("a state this build does not know invalidates the file")
    func unknownStatusInvalidatesTheFile() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(lane: "Tandem", number: 1, status: "halfway",
                   openedAt: "2026-10-03T10:00:00Z"),
        ])

        let listed = try sprints.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
    }

    @Test("a mode this build does not know invalidates the file")
    func unknownModeInvalidatesTheFile() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(lane: "Tandem", number: 1, status: "open", mode: "ad hoc",
                   openedAt: "2026-10-03T10:00:00Z"),
        ])

        let listed = try sprints.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
    }

    // MARK: - Unknown ids

    @Test("an unknown ticket id's message is the domain failure the CLI exits with")
    func unknownTicketIDMessage() throws {
        let (sprints, store, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.removeItem(at: store.directory) }
        _ = try sprints.open(lane: "Tandem")

        // An empty store: nothing is on the board, so nothing is accepted.
        do {
            _ = try sprints.ticket(lane: "Tandem", number: 1, ticketID: UUID(), on: store)
            Issue.record("expected unknownTicketID")
        } catch DomainError.unknownTicketID {
        }
    }

    // MARK: - Recovery

    @Test("a corrupt sprints file is moved aside, and the next read is empty with a note naming sprints.json")
    func corruptFileIsMovedAside() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("{not a sprints file\n".utf8).write(to: file(dir))

        let listed = try sprints.list()
        #expect(listed.value.isEmpty)
        let note = try #require(listed.note)
        #expect(note.contains("corrupt"))

        // The corrupt file is kept, not deleted, and the live file is gone: the
        // list restarted empty, and the next write makes a new file.
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix("sprints.json.corrupt-") }
        #expect(kept.count == 1)
        // The recovery note names *this* file — sprints.json — never the
        // store's todos.json and never the sessions file's sessions.json.
        #expect(note.contains("sprints.json"))
        #expect(note.contains(kept[0]))
        #expect(!note.contains("todos.json"))
        #expect(!note.contains("sessions.json"))
        #expect(!FileManager.default.fileExists(atPath: file(dir).path))

        // The next open starts the list over, and it is numbered from 1 again.
        let reopened = try sprints.open(lane: "Tandem")
        #expect(reopened.value.number == 1)
    }

    @Test("a corrupt sprints file that cannot be moved aside fails in the sprints file's own words")
    func corruptFileThatCannotBeMovedAsideFails() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("{not a sprints file\n".utf8).write(to: file(dir))
        // Read-only directory: the corrupt file cannot be renamed aside, so the
        // recovery fails and the file stays where it is.
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try sprints.list()
            Issue.record("expected corruptSprintsFile")
        } catch let DomainError.corruptSprintsFile(path, detail) {
            #expect(path == file(dir).path)
            #expect(!detail.isEmpty)
        }

        // The failure the human sees names the sprints file, and never calls
        // it a store or sessions error: those files are fine.
        let text = tandemErrorText(DomainError.corruptSprintsFile(
            path: file(dir).path, detail: "Permission denied"))
        #expect(text.contains("sprints.json"))
        #expect(text.contains(file(dir).path))
        #expect(!text.contains("Tandem store error"))
        #expect(!text.contains("sessions.json"))
    }

    @Test("the sprints file's decode failures are its own signal, not the sessions file's")
    func decodeFailuresAreTheSprintsFileOwnSignal() throws {
        let decoder = JSONDecoder()

        // A status this build does not know: the file is invalid, and the
        // signal says so in the sprints file's own nouns.
        let unknownStatus = """
        {"lane":"Tandem","number":1,"session":null,"mode":"serial","batch":[],"runs":[],
         "close":null,"status":"halfway","openedAt":"2026-10-03T10:00:00Z","closedAt":null}
        """
        do {
            _ = try decoder.decode(Sprint.self, from: Data(unknownStatus.utf8))
            Issue.record("expected the sprints file's own decode signal")
        } catch let error as CorruptSprints {
            guard case .unknownStatus(let status) = error else {
                Issue.record("expected unknownStatus, got \(error)")
                return
            }
            #expect(status == "halfway")
        } catch {
            Issue.record("expected CorruptSprints, got \(error)")
        }

        // A mode this build does not know: the same signal, its own case.
        let unknownMode = """
        {"lane":"Tandem","number":1,"session":null,"mode":"ad hoc","batch":[],"runs":[],
         "close":null,"status":"open","openedAt":"2026-10-03T10:00:00Z","closedAt":null}
        """
        do {
            _ = try decoder.decode(Sprint.self, from: Data(unknownMode.utf8))
            Issue.record("expected the sprints file's own decode signal")
        } catch let error as CorruptSprints {
            guard case .unknownMode(let mode) = error else {
                Issue.record("expected unknownMode, got \(error)")
                return
            }
            #expect(mode == "ad hoc")
        } catch {
            Issue.record("expected CorruptSprints, got \(error)")
        }

        // An unreadable timestamp: the same signal, a different case.
        let badTimestamp = """
        {"lane":"Tandem","number":1,"session":null,"mode":"serial","batch":[],"runs":[],
         "close":null,"status":"open","openedAt":"not-a-date","closedAt":null}
        """
        do {
            _ = try decoder.decode(Sprint.self, from: Data(badTimestamp.utf8))
            Issue.record("expected the sprints file's own decode signal")
        } catch let error as CorruptSprints {
            guard case .invalidTimestamp(let field, let value) = error else {
                Issue.record("expected invalidTimestamp, got \(error)")
                return
            }
            #expect(field == "openedAt")
            #expect(value == "not-a-date")
        } catch {
            Issue.record("expected CorruptSprints, got \(error)")
        }
    }

    @Test("a version this build does not read invalidates the file")
    func unknownVersionInvalidatesTheFile() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(#"{"version":2,"sprints":[]}"#.utf8).write(to: file(dir))

        let listed = try sprints.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix("sprints.json.corrupt-") }
        #expect(kept.count == 1)
    }

    // MARK: - The file's own I/O failures

    @Test("a sprints write on an unwritable home wears the store's vocabulary")
    func sprintsWriteOnUnwritableHomeIsAStoreFailure() throws {
        let (sprints, _, dir) = try tempSprints()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try sprints.open(lane: "Tandem")
            Issue.record("expected the store's own failure")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("Tandem store error"), "got \(text)")
            #expect(text.contains("tandem.db"), "got \(text)")
            #expect(!text.contains("sprints file"), "got \(text)")
        }
    }

    @Test("the sessions file's own failures still wear the sessions file's vocabulary")
    func sessionsFileVocabularyUnchanged() throws {
        // The sprints file is the new file here; the sessions file's errors
        // are untouched by it — one case apiece, spelled as they were.
        let text = tandemErrorText(DomainError.sessionsFileLocked(path: "/tmp/sessions.json"))
        #expect(text.contains("sessions.json"))
        #expect(!text.contains("sprints.json.lock"))
    }
}
