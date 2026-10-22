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

    @Test("the first start creates sessions.json in the given directory, and nothing else")
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

        // The file, and the lockfile the mechanism takes beside it — no store
        // file, no backup, no temp file.
        let names = try fm.contentsOfDirectory(atPath: dir.path).sorted()
        #expect(names == ["sessions.json", "sessions.lock"])
    }

    @Test("a second start returns the record unchanged, and the file's bytes do not move")
    func secondStartChangesNothing() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let first = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        let before = try Data(contentsOf: file(dir))

        let second = try sessions.start(id: "ses_1", project: "tandem", brief: "A different brief.")
        #expect(second.value == first.value)
        #expect(second.note == "session ses_1 already recorded")
        #expect(try Data(contentsOf: file(dir)) == before)

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
        #expect(!FileManager.default.fileExists(atPath: file(dir).path))
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
        #expect(!FileManager.default.fileExists(atPath: file(dir).path))
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

    @Test("the file carries the envelope and the whole record shape")
    func fileCarriesTheEnvelopeAndTheRecord() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = UUID()
        _ = try sessions.start(id: "ses_1", project: "tandem", agent: "black",
                               brief: "  Build the core.  ")
        _ = try sessions.link(id: "ses_1", ticket: ticket)
        _ = try sessions.end(id: "ses_1", outcome: .succeeded, summary: "Built it.")

        let raw = try Data(contentsOf: file(dir))
        let object = try #require(JSONSerialization.jsonObject(with: raw) as? [String: Any])
        #expect(object["version"] as? Int == 1)
        let records = try #require(object["sessions"] as? [[String: Any]])
        #expect(records.count == 1)
        let record = try #require(records.first)
        #expect(record["id"] as? String == "ses_1")
        #expect(record["project"] as? String == "tandem")
        #expect(record["agent"] as? String == "black")
        #expect(record["brief"] as? String == "Build the core.")
        #expect(record["summary"] as? String == "Built it.")
        #expect(record["status"] as? String == "ended")
        #expect(record["outcome"] as? String == "succeeded")
        #expect(record["tickets"] as? [String] == [ticket.uuidString])
        // Every stamp is ISO-8601 UTC with a trailing Z, like the store's.
        #expect((record["startedAt"] as? String)?.hasSuffix("Z") == true)
        #expect((record["endedAt"] as? String)?.hasSuffix("Z") == true)
    }

    @Test("an unknown key is ignored on read, and gone on the next write")
    func unknownKeysAreIgnoredOnReadAndDroppedOnWrite() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        try writeFile(dir, [
            record(id: "ses_1", project: "tandem", status: "running",
                   startedAt: "2026-10-03T10:00:00Z",
                   extra: ["aKeyThisBuildDoesNotKnow": 42]),
        ])

        // Lenient read: the unknown key does not make the file invalid.
        let listed = try sessions.list().value
        #expect(listed.map(\.id) == ["ses_1"])

        // Strict write: the next write emits the shape this build knows.
        _ = try sessions.link(id: "ses_1", ticket: UUID())
        let raw = try Data(contentsOf: file(dir))
        let object = try #require(JSONSerialization.jsonObject(with: raw) as? [String: Any])
        let records = try #require(object["sessions"] as? [[String: Any]])
        #expect(records.first?["aKeyThisBuildDoesNotKnow"] == nil)
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

    @Test("a corrupt sessions file is moved aside, and the next read is empty with a note")
    func corruptFileIsMovedAside() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        try Data("{not a sessions file\n".utf8).write(to: file(dir))

        let listed = try sessions.list()
        #expect(listed.value.isEmpty)
        let note = try #require(listed.note)
        #expect(note.contains("corrupt"))

        // The corrupt file is kept, not deleted, and the live file is gone: the
        // list restarted empty, and the next write makes a new file.
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
        // The lockfile first: it must exist before the directory is closed to
        // writes, or the lock itself would be what fails.
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
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

    @Test("a home whose sessions file cannot be locked fails in the sessions file's own words")
    func lockFailureNamesTheSessionsFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        // A directory that cannot be written: the lockfile cannot be created,
        // so the lock fails before the file is ever touched.
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        do {
            _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
            Issue.record("expected the sessions file's own lock failure")
        } catch let DomainError.sessionsFileIO(path, op, detail) {
            #expect(path == file(dir).path)
            #expect(op == "open lockfile")
            #expect(detail.contains("13"))
        }

        // The failure the human sees names the sessions file and the operation,
        // and never calls it a store error: the store is `todos.json`, and it
        // is fine.
        let text = tandemErrorText(DomainError.sessionsFileIO(
            path: file(dir).path, op: "open lockfile", detail: "13: Permission denied"))
        #expect(text.contains("sessions.json"))
        #expect(text.contains(file(dir).path))
        #expect(text.contains("open lockfile"))
        #expect(!text.contains("Tandem store"))
    }

    @Test("a sessions file locked by another process fails in the sessions file's own words")
    func lockContentionNamesTheSessionsFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        // The lockfile first: it must exist before another process holds it.
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
        let lockPath = dir.appendingPathComponent("sessions.lock")
        let fd = open(lockPath.path, O_RDWR | O_CREAT, 0o644)
        #expect(fd >= 0)
        #expect(flock(fd, LOCK_EX) == 0)
        defer { close(fd) }

        do {
            _ = try sessions.start(id: "ses_2", project: "tandem", brief: "Read the fleet.")
            Issue.record("expected the sessions file's own lock failure")
        } catch let DomainError.sessionsFileLocked(path) {
            #expect(path == file(dir).path)
        }

        let text = tandemErrorText(DomainError.sessionsFileLocked(path: file(dir).path))
        #expect(text.contains("sessions.json"))
        #expect(text.contains(file(dir).path))
        #expect(!text.contains("Tandem store"))
    }

    @Test("a write that cannot happen fails in the sessions file's own words")
    func writeFailureNamesTheSessionsFile() throws {
        let (sessions, dir) = try tempSessions()
        defer { try? FileManager.default.removeItem(at: dir) }
        // A directory where the file should be: the atomic write's `rename`
        // cannot land on it, so the write fails where the file is named.
        try FileManager.default.createDirectory(at: file(dir), withIntermediateDirectories: true)

        do {
            _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Build the core.")
            Issue.record("expected the sessions file's own write failure")
        } catch let DomainError.sessionsFileIO(path, op, detail) {
            #expect(path == file(dir).path)
            #expect(op == "rename")
            #expect(!detail.isEmpty)
        }

        let text = tandemErrorText(DomainError.sessionsFileIO(
            path: file(dir).path, op: "rename", detail: "21: Is a directory"))
        #expect(text.contains("sessions.json"))
        #expect(text.contains(file(dir).path))
        #expect(text.contains("rename"))
        #expect(!text.contains("Tandem store"))
    }

    @Test("the store's own lock failure still wears the store's vocabulary")
    func storeLockFailureUnchanged() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-store-lock-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)

        // The store's file is `todos.json`, and its failures say so: this
        // ticket is about the file that is not the store.
        do {
            _ = try Store(directory: dir).add("a ticket")
            Issue.record("expected the store's own lock failure")
        } catch {
            let text = tandemErrorText(error)
            #expect(text.contains("Tandem store I/O failed during open lockfile"))
        }
    }
}
