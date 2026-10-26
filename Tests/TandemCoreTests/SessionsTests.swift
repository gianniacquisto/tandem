import Darwin
import Foundation
import Testing
@testable import TandemCore

/// The sessions file (spec `FC770B8A`, S1): `sessions.json` beside
/// `todos.json`, its record, and its operations — `start`, `end`, `link`,
/// `list`, `show`. A record is created once, stamped once and linked to; it is
/// a fact about a session that ran, not a log line.
///
/// The suite runs against an explicit directory, never `TandemHome`: a test
/// that reached the human's real `~/.tandem` would write a session record into
/// their home.
@Suite("Sessions file")
struct SessionsTests {

    private func tempSessions() throws -> (Sessions, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sessions-tests-\(UUID().uuidString)")
        return (Sessions(directory: dir), dir)
    }

    private func file(_ dir: URL) -> URL {
        dir.appendingPathComponent("sessions.json")
    }

    /// Write a sessions file by hand: the envelope and the records, exactly as
    /// the core writes them. The timestamps are the file's own, so the list's
    /// order is the file's truth rather than the clock's.
    private func writeFile(_ dir: URL, _ records: [[String: Any]]) throws {
        let envelope: [String: Any] = ["version": 1, "sessions": records]
        let data = try JSONSerialization.data(
            withJSONObject: envelope, options: [.prettyPrinted, .sortedKeys])
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try data.write(to: file(dir))
    }

    /// One hand-written record. `startedAt` and `endedAt` are the file's own
    /// ISO-8601 UTC stamps.
    private func record(
        id: String,
        project: String,
        status: String,
        outcome: String? = nil,
        startedAt: String,
        endedAt: String? = nil,
        extra: [String: Any] = [:]
    ) -> [String: Any] {
        var record: [String: Any] = [
            "id": id,
            "project": project,
            "agent": NSNull(),
            "brief": "Work on \(id).",
            "summary": NSNull(),
            "status": status,
            "outcome": outcome ?? NSNull(),
            "tickets": [],
            "startedAt": startedAt,
            "endedAt": endedAt ?? NSNull(),
        ]
        for (key, value) in extra { record[key] = value }
        return record
    }

    // MARK: - start

    @Test("the first start creates the database, and never a sessions.json")
    func firstStartCreatesTheFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let fm = FileManager.default
        #expect(!fm.fileExists(atPath: file(dir).path))

        let result = try sessions.start(
            id: "ses_1", project: "tandem", agent: "black", brief: "Build the sessions core.")
        let record = result.value
        #expect(record.id == "ses_1")
        #expect(record.project == "tandem")
        #expect(record.agent == "black")
        #expect(record.brief == "Build the sessions core.")
        #expect(record.status == .running)
        #expect(record.outcome == nil)
        #expect(record.endedAt == nil)
        #expect(record.tickets.isEmpty)
        #expect(result.note == nil)

        // The database, and no legacy file, no lockfile, no temp file.
        let names = try fm.contentsOfDirectory(atPath: dir.path).sorted()
        #expect(names.contains("tandem.db"))
        #expect(!names.contains("sessions.json"))
        #expect(!names.contains("sessions.lock"))
    }

    @Test("a second start returns the record unchanged, and the file's bytes do not move")
    func secondStartChangesNothing() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let first = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        let before = try sessions.list().value

        let second = try sessions.start(id: "ses_1", project: "tandem", brief: "A different brief.")
        #expect(second.value == first.value)
        #expect(second.note == "session ses_1 already recorded")
        #expect(try sessions.list().value == before)

        // One record, and it is the first one — the second brief changed nothing.
        let listed = try sessions.list().value
        #expect(listed.count == 1)
        #expect(listed.first?.brief == "Build the core.")
    }

    @Test("a start on a session that has ended does not restart it")
    func startDoesNotRestartAnEndedSession() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        _ = try sessions.end(id: "ses_1", outcome: .succeeded, summary: "Built it.")

        let again = try sessions.start(id: "ses_1", project: "tandem", brief: "Build it again.")
        #expect(again.value.status == .ended)
        #expect(again.value.outcome == .succeeded)
        #expect(again.value.summary == "Built it.")
        #expect(again.note == "session ses_1 already recorded")
    }

    @Test("an empty or whitespace-only brief is refused, and nothing is written")
    func emptyBriefRefused() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }

        for brief in ["", "   ", "\n  \n"] {
            do {
                _ = try sessions.start(id: "ses_1", project: "tandem", brief: brief)
                Issue.record("expected emptySessionBrief for brief \(brief.debugDescription)")
            } catch DomainError.emptySessionBrief {
                // The domain failure, not a usage one.
            }
        }
        #expect(!FileManager.default.fileExists(
            atPath: dir.appendingPathComponent("tandem.db").path))
        #expect(try sessions.list().value.isEmpty)
    }

    @Test("an empty or whitespace-only session id is refused, and nothing is written")
    func emptySessionIDRefused() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }

        for id in ["", "   ", "\n  \n"] {
            do {
                _ = try sessions.start(id: id, project: "tandem", brief: "Build the core.")
                Issue.record("expected emptySessionID for id \(id.debugDescription)")
            } catch DomainError.emptySessionID {
                // The domain failure, not a usage one.
            }
        }
        #expect(!FileManager.default.fileExists(
            atPath: dir.appendingPathComponent("tandem.db").path))
        #expect(try sessions.list().value.isEmpty)

        // The other id-taking operations refuse it too, rather than reporting
        // an empty id as an unknown session.
        do {
            _ = try sessions.show(id: "")
            Issue.record("expected emptySessionID")
        } catch DomainError.emptySessionID {
        }
        do {
            _ = try sessions.end(id: "  ", outcome: .succeeded)
            Issue.record("expected emptySessionID")
        } catch DomainError.emptySessionID {
        }
        do {
            _ = try sessions.link(id: "", ticket: UUID())
            Issue.record("expected emptySessionID")
        } catch DomainError.emptySessionID {
        }
    }

    @Test("the session id is trimmed, and the trimmed id is the one stored")
    func sessionIDIsTrimmed() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }

        let started = try sessions.start(id: "  ses_1  ", project: "tandem", brief: "Build it.")
        #expect(started.value.id == "ses_1")
        // The same id, however it is spelled, reaches the same record.
        #expect(try sessions.show(id: " ses_1 ").value.id == "ses_1")
        // And a second start under the padded spelling is the same record.
        let again = try sessions.start(id: " ses_1 ", project: "tandem", brief: "Build it.")
        #expect(again.note == "session ses_1 already recorded")
        #expect(try sessions.list().value.count == 1)
    }

    // MARK: - end

    @Test("end stamps the record once; a second end keeps the first outcome and summary")
    func endStampsOnce() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")

        let ended = try sessions.end(
            id: "ses_1", outcome: .succeeded, summary: "  Built the core.  ")
        #expect(ended.value.status == .ended)
        #expect(ended.value.outcome == .succeeded)
        #expect(ended.value.summary == "Built the core.")
        #expect(ended.value.endedAt != nil)
        #expect(ended.note == nil)

        let again = try sessions.end(
            id: "ses_1", outcome: .failed, summary: "A different summary.")
        #expect(again.value.outcome == .succeeded)
        #expect(again.value.summary == "Built the core.")
        #expect(again.value.endedAt == ended.value.endedAt)
        #expect(again.note == "session ses_1 already ended")

        // And the file carries the first outcome, not the second.
        let shown = try sessions.show(id: "ses_1").value
        #expect(shown.outcome == .succeeded)
        #expect(shown.summary == "Built the core.")
        #expect(shown.status == .ended)
    }

    // MARK: - the caller's own clock

    @Test("start(at:) keeps the caller's timestamp: a record can stand for a session that started earlier")
    func startKeepsTheGivenStart() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }

        let started = Date(timeIntervalSince1970: 1_761_123_456)
        let result = try sessions.start(
            id: "ses_1", project: "tandem",
            brief: "A session that started earlier.", at: started)
        #expect(result.value.startedAt == started)

        // The file carries the same stamp: a fresh read gives the same record.
        let shown = try sessions.show(id: "ses_1")
        #expect(shown.value.startedAt == started)
    }

    @Test("end(at:) keeps the caller's timestamp: a record can stand for a session that ended earlier")
    func endKeepsTheGivenEnd() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "A session that ended earlier.")

        let ended = Date(timeIntervalSince1970: 1_761_234_567)
        let result = try sessions.end(
            id: "ses_1", outcome: .unknown,
            summary: "It ended earlier.", at: ended)
        #expect(result.value.status == .ended)
        #expect(result.value.outcome == .unknown)
        #expect(result.value.endedAt == ended)

        let shown = try sessions.show(id: "ses_1")
        #expect(shown.value.endedAt == ended)
    }

    // MARK: - usage

    /// The usage a reported session carries — the five counts as opencode's
    /// row holds them, the cost in dollars, the model that spent them.
    private func usage(
        input: Int = 12345,
        output: Int = 4567,
        reasoning: Int = 567,
        cacheRead: Int = 89012,
        cacheWrite: Int = 1234,
        cost: Double = 0.42,
        model: String = "z-ai/glm-5.3-flash"
    ) -> (tokens: SessionTokens, cost: Double, model: String) {
        (SessionTokens(
            input: input, output: output, reasoning: reasoning,
            cacheRead: cacheRead, cacheWrite: cacheWrite), cost, model)
    }

    @Test("end writes the usage once, beside the outcome; a second end cannot restate it")
    func endWritesTheUsageOnce() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        let reported = usage()

        let ended = try sessions.end(
            id: "ses_1", outcome: .succeeded, summary: "Built it.",
            tokens: reported.tokens, cost: reported.cost, model: reported.model)
        #expect(ended.value.tokens == reported.tokens)
        #expect(ended.value.cost == reported.cost)
        #expect(ended.value.model == reported.model)

        // The first usage is the fact, like the first outcome.
        let other = usage(input: 1, output: 2, reasoning: 3, cacheRead: 4, cacheWrite: 5,
                          cost: 9.99, model: "other/model")
        let again = try sessions.end(
            id: "ses_1", outcome: .failed, summary: "Again.",
            tokens: other.tokens, cost: other.cost, model: other.model)
        #expect(again.value.tokens == reported.tokens)
        #expect(again.value.cost == reported.cost)
        #expect(again.value.model == reported.model)
        #expect(again.note == "session ses_1 already ended")
    }

    @Test("a record ended without usage carries none, and the file omits the keys")
    func absentUsageIsAbsentOnTheWire() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        _ = try sessions.end(id: "ses_1", outcome: .succeeded, summary: "Built it.")

        let shown = try sessions.show(id: "ses_1").value
        #expect(shown.tokens == nil)
        #expect(shown.cost == nil)
        #expect(shown.model == nil)

        // The database row carries none of the three: a record without usage
        // stays without usage across the round trip.
        let again = try sessions.list().value
        #expect(again.first?.tokens == nil)
        #expect(again.first?.cost == nil)
        #expect(again.first?.model == nil)
    }

    @Test("the usage survives the round trip through the file")
    func usageSurvivesTheFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let reported = usage()
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "ended",
                   outcome: "succeeded", startedAt: "2026-10-23T10:00:00Z",
                   endedAt: "2026-10-23T11:00:00Z",
                   extra: [
                       "tokens": [
                           "input": reported.tokens.input,
                           "output": reported.tokens.output,
                           "reasoning": reported.tokens.reasoning,
                           "cacheRead": reported.tokens.cacheRead,
                           "cacheWrite": reported.tokens.cacheWrite,
                       ],
                       "cost": reported.cost,
                       "model": reported.model,
                   ]),
        ])

        let shown = try sessions.show(id: "ses_1").value
        #expect(shown.tokens == reported.tokens)
        #expect(shown.cost == reported.cost)
        #expect(shown.model == reported.model)
    }

    @Test("onStamp runs once, and only when the stamp lands")
    func onStampRunsOncePerStamp() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        var stamped: [Session] = []

        let reported = usage()
        _ = try sessions.end(
            id: "ses_1", outcome: .succeeded, summary: "Built it.",
            tokens: reported.tokens, cost: reported.cost, model: reported.model,
            onStamp: { stamped.append($0) })
        #expect(stamped.count == 1)
        #expect(stamped.first?.id == "ses_1")
        #expect(stamped.first?.status == .ended)

        // The second end stamps nothing, so it calls nothing — the hook cannot
        // post twice.
        _ = try sessions.end(
            id: "ses_1", outcome: .failed, summary: "Again.",
            tokens: reported.tokens, cost: reported.cost, model: reported.model,
            onStamp: { stamped.append($0) })
        #expect(stamped.count == 1)
    }

    // MARK: - link

    @Test("link appends a ticket once, and keeps the link order")
    func linkDedupesAndKeepsOrder() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        let first = UUID()
        let second = UUID()
        let third = UUID()

        _ = try sessions.link(id: "ses_1", ticket: first)
        _ = try sessions.link(id: "ses_1", ticket: second)
        let dupe = try sessions.link(id: "ses_1", ticket: first)
        #expect(dupe.value.tickets == [first, second])

        let linked = try sessions.link(id: "ses_1", ticket: third)
        #expect(linked.value.tickets == [first, second, third])

        // And the order survives the round trip through the file.
        let shown = try sessions.show(id: "ses_1").value
        #expect(shown.tickets == [first, second, third])
    }

    // MARK: - list

    @Test("list is newest-first, and filters by project and status")
    func listIsNewestFirstAndFilters() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_oldest", project: "tandem", status: "ended",
                   outcome: "succeeded", startedAt: "2026-10-01T10:00:00Z",
                   endedAt: "2026-10-01T11:00:00Z"),
            record(id: "ses_middle", project: "fleet", status: "running",
                   startedAt: "2026-10-02T10:00:00Z"),
            record(id: "ses_newest", project: "tandem", status: "ended",
                   outcome: "failed", startedAt: "2026-10-03T10:00:00Z",
                   endedAt: "2026-10-03T12:00:00Z"),
        ])

        let all = try sessions.list().value
        #expect(all.map(\.id) == ["ses_newest", "ses_middle", "ses_oldest"])

        let tandem = try sessions.list(project: "tandem").value
        #expect(tandem.map(\.id) == ["ses_newest", "ses_oldest"])
        let fleet = try sessions.list(project: "fleet").value
        #expect(fleet.map(\.id) == ["ses_middle"])

        let ended = try sessions.list(status: .ended).value
        #expect(ended.map(\.id) == ["ses_newest", "ses_oldest"])
        let running = try sessions.list(status: .running).value
        #expect(running.map(\.id) == ["ses_middle"])

        // Both filters together.
        let both = try sessions.list(project: "tandem", status: .running).value
        #expect(both.isEmpty)
    }

    @Test("a record read from the file keeps every field it was written with")
    func readBackKeepsTheFields() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "running",
                   startedAt: "2026-10-03T10:00:00Z"),
        ])

        let shown = try sessions.show(id: "ses_1").value
        #expect(shown.id == "ses_1")
        #expect(shown.project == "tandem")
        #expect(shown.agent == nil)
        #expect(shown.brief == "Work on ses_1.")
        #expect(shown.summary == nil)
        #expect(shown.status == .running)
        #expect(shown.outcome == nil)
        #expect(shown.tickets.isEmpty)
        #expect(shown.endedAt == nil)
        #expect(Timestamp.encode(shown.startedAt) == "2026-10-03T10:00:00Z")
    }

    // MARK: - The file's shape

    @Test("the record round-trips through the database with every field")
    func recordRoundTripsThroughTheDatabase() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = UUID()
        _ = try sessions.start(id: "ses_1", project: "tandem", agent: "black",
                               brief: "  Build the core.  ")
        _ = try sessions.link(id: "ses_1", ticket: ticket)
        _ = try sessions.end(id: "ses_1", outcome: .succeeded, summary: "Built it.")

        let record = try sessions.show(id: "ses_1").value
        #expect(record.id == "ses_1")
        #expect(record.project == "tandem")
        #expect(record.agent == "black")
        #expect(record.brief == "Build the core.")
        #expect(record.summary == "Built it.")
        #expect(record.status == .ended)
        #expect(record.outcome == .succeeded)
        #expect(record.tickets == [ticket])
        #expect(record.endedAt != nil)
    }

    @Test("an unknown key is ignored on import and never reaches the database row")
    func unknownKeysAreIgnoredOnImport() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "running",
                   startedAt: "2026-10-03T10:00:00Z",
                   extra: ["aKeyThisBuildDoesNotKnow": 42]),
        ])

        // Lenient read: the unknown key does not make the file invalid.
        #expect(try sessions.list().value.map(\.id) == ["ses_1"])

        // The first write imports the record this build knows; the unknown key
        // has no column and is not carried.
        _ = try sessions.link(id: "ses_1", ticket: UUID())
        #expect(try sessions.show(id: "ses_1").value.tickets.count == 1)
    }

    @Test("a missing required key invalidates the file")
    func missingRequiredKeyInvalidatesTheFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            ["id": "ses_1", "project": "tandem", "status": "running",
             "tickets": [], "startedAt": "2026-10-03T10:00:00Z"],  // no brief
        ])

        let listed = try sessions.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
    }

    @Test("a state this build does not know invalidates the file")
    func unknownStatusInvalidatesTheFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "halfway",
                   startedAt: "2026-10-03T10:00:00Z"),
        ])

        let listed = try sessions.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
    }

    @Test("an outcome this build does not know reads as unknown")
    func unknownOutcomeReadsAsUnknown() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "ended",
                   outcome: "cancelled", startedAt: "2026-10-03T10:00:00Z",
                   endedAt: "2026-10-03T11:00:00Z"),
        ])

        // One record's outcome is flattened; the file is not thrown away.
        let listed = try sessions.list()
        #expect(listed.value.map(\.id) == ["ses_1"])
        #expect(listed.value.first?.outcome == .unknown)
        #expect(listed.note == nil)
    }

    // MARK: - Unknown ids

    @Test("an unknown session id fails with the recorded ids")
    func unknownSessionThrows() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        _ = try sessions.start(id: "ses_2", project: "fleet", brief: "Read the fleet.")
        let unknown = "ses_nope"

        do {
            _ = try sessions.show(id: unknown)
            Issue.record("expected unknownSession")
        } catch let DomainError.unknownSession(id, known) {
            #expect(id == unknown)
            #expect(known == ["ses_1", "ses_2"])
        }
        // `end` and `link` refuse it too.
        do {
            _ = try sessions.end(id: unknown, outcome: .succeeded)
            Issue.record("expected unknownSession")
        } catch let DomainError.unknownSession(id, known) {
            #expect(id == unknown)
            #expect(known == ["ses_1", "ses_2"])
        }
        do {
            _ = try sessions.link(id: unknown, ticket: UUID())
            Issue.record("expected unknownSession")
        } catch let DomainError.unknownSession(id, known) {
            #expect(id == unknown)
            #expect(known == ["ses_1", "ses_2"])
        }
    }

    // MARK: - Recovery

    @Test("a corrupt legacy sessions file is moved aside before the first import, with a note")
    func corruptFileIsMovedAside() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("{not a sessions file\n".utf8).write(to: file(dir))

        let listed = try sessions.list()
        #expect(listed.value.isEmpty)
        let note = try #require(listed.note)
        #expect(note.contains("corrupt"))

        // The corrupt file is kept, not deleted, and the live name is gone: the
        // list restarted empty.
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix("sessions.json.corrupt-") }
        #expect(kept.count == 1)
        #expect(note.contains(kept[0]))
        #expect(!FileManager.default.fileExists(atPath: file(dir).path))
    }

    // MARK: - The file's own errors

    @Test("a corrupt sessions file that cannot be moved aside fails in the sessions file's own words")
    func corruptFileThatCannotBeMovedAsideFails() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("{not a sessions file\n".utf8).write(to: file(dir))
        // Read-only directory: the corrupt file cannot be renamed aside, so the
        // recovery fails and the file stays where it is.
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try sessions.list()
            Issue.record("expected corruptSessionsFile")
        } catch let DomainError.corruptSessionsFile(path, detail) {
            #expect(path == file(dir).path)
            #expect(!detail.isEmpty)
        }

        // The failure the human sees names the sessions file, and never calls
        // it a store error: the store is `todos.json`, and it is fine.
        let text = tandemErrorText(DomainError.corruptSessionsFile(
            path: file(dir).path, detail: "Permission denied"))
        #expect(text.contains("sessions.json"))
        #expect(text.contains(file(dir).path))
        #expect(!text.contains("Tandem store error"))
    }

    @Test("the sessions file's decode failures are its own signal, not the store's")
    func decodeFailuresAreTheSessionsFileOwnSignal() throws {
        let decoder = JSONDecoder()

        // A status this build does not know: the file is invalid, and the
        // signal says so in the sessions file's own nouns.
        let unknownStatus = """
        {"id":"ses_1","project":"tandem","agent":null,"brief":"b","summary":null,
         "status":"halfway","outcome":null,"tickets":[],
         "startedAt":"2026-10-03T10:00:00Z","endedAt":null}
        """
        do {
            _ = try decoder.decode(Session.self, from: Data(unknownStatus.utf8))
            Issue.record("expected the sessions file's own decode signal")
        } catch let error as CorruptSessions {
            guard case .unknownStatus(let status) = error else {
                Issue.record("expected unknownStatus, got \(error)")
                return
            }
            #expect(status == "halfway")
        } catch {
            Issue.record("expected CorruptSessions, got \(error)")
        }

        // An unreadable timestamp: the same signal, a different case.
        let badTimestamp = """
        {"id":"ses_1","project":"tandem","agent":null,"brief":"b","summary":null,
         "status":"running","outcome":null,"tickets":[],
         "startedAt":"not-a-date","endedAt":null}
        """
        do {
            _ = try decoder.decode(Session.self, from: Data(badTimestamp.utf8))
            Issue.record("expected the sessions file's own decode signal")
        } catch let error as CorruptSessions {
            guard case .invalidTimestamp(let field, let value) = error else {
                Issue.record("expected invalidTimestamp, got \(error)")
                return
            }
            #expect(field == "startedAt")
            #expect(value == "not-a-date")
        } catch {
            Issue.record("expected CorruptSessions, got \(error)")
        }
    }

    @Test("a version this build does not read invalidates the file")
    func unknownVersionInvalidatesTheFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(#"{"version":2,"sessions":[]}"#.utf8).write(to: file(dir))

        let listed = try sessions.list()
        #expect(listed.value.isEmpty)
        #expect(listed.note != nil)
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { $0.hasPrefix("sessions.json.corrupt-") }
        #expect(kept.count == 1)
    }

    // MARK: - The file's own I/O failures

    @Test("a sessions write on an unwritable home wears the store's vocabulary")
    func sessionsWriteOnUnwritableHomeIsAStoreFailure() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
            Issue.record("expected the store's own failure")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("Tandem store error"), "got \(text)")
            #expect(text.contains("tandem.db"), "got \(text)")
            #expect(!text.contains("sessions file"), "got \(text)")
        }
    }

    @Test("the store's own failure still wears the store's vocabulary")
    func storeLockFailureUnchanged() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-store-lock-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        // The store cannot create its database there, and its failure says so in its
        // own words and names the file: this ticket is about the file that is not the
        // store. (`StoreLockTests` owned this wording before the database took over.)
        do {
            _ = try Store(directory: dir).add("a ticket")
            Issue.record("expected the store's own failure")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("Tandem store error: could not open"), "got \(text)")
            #expect(text.contains("tandem.db"), "got \(text)")
            #expect(!text.contains("sessions file"))
        }
    }
}
