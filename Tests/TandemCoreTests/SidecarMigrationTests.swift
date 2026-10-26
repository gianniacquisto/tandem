import Foundation
import Testing
@testable import TandemCore

/// The sidecar domains' one-time import and read-fallback (ADR 0031, ticket
/// 396287D4): comments, sessions, sprints and settings each move into `tandem.db` on
/// their own first write, copy every source file aside as `<name>.pre-sqlite-<ts>`,
/// and never read the JSON for a write again. A read before the marker lands keeps
/// serving the JSON, so an installed build's first read never lies.
@Suite("Sidecar migration and fallback")
struct SidecarMigrationTests {

    private func tempDir() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-sidecar-migration-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func entries(in dir: URL) -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
    }

    private func copies(of name: String, in dir: URL) -> [String] {
        entries(in: dir).filter { $0.hasPrefix("\(name).pre-sqlite-") }.sorted()
    }

    // MARK: - Comments

    private func legacyCommentLine(author: String, at: String, body: String) throws -> String {
        let object: [String: String] = ["author": author, "at": at, "body": body]
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        return try #require(String(data: data, encoding: .utf8)) + "\n"
    }

    private func commentLog(_ store: Store, _ id: UUID) -> URL {
        store.commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    @Test("a comment log imports on the first append: the legacy and the new comment read back from the database")
    func commentsImportOnFirstAppend() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let ticket = try store.add("A ticket").value
        let log = commentLog(store, ticket.id)
        try FileManager.default.createDirectory(
            at: store.commentsDirectory, withIntermediateDirectories: true)
        try legacyCommentLine(author: "human", at: "2026-01-01T00:00:00Z", body: "legacy")
            .write(to: log, atomically: false, encoding: .utf8)

        let appended = try store.append(
            ticketID: ticket.id.uuidString, author: "builder", body: "new").value
        #expect(appended.body == "new")

        // The import copies the source aside and leaves the log in place: the bytes
        // are the evidence the import is true.
        #expect(FileManager.default.fileExists(atPath: log.path))
        #expect(try copies(of: log.lastPathComponent, in: store.commentsDirectory).count == 1)

        // Reads are the database's now: removing the log changes nothing.
        try FileManager.default.removeItem(at: log)
        let listed = try store.list(ticketID: ticket.id.uuidString).value
        #expect(listed.map(\.body) == ["legacy", "new"])
        #expect(listed.map(\.author) == ["human", "builder"])
    }

    @Test("a first comment append imports the ticket domain it checks against, in the same transaction")
    func commentAppendImportsTheTicketDomain() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let id = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        let fixture = """
            {"version":1,"lanes":[],"tickets":[
            {"id":"\(id.uuidString)","title":"legacy","status":"backlog","archived":false,"description":null,"assignee":null,"origin":"human","lane":null,"link":null,"createdAt":"2026-01-01T00:00:00Z","completedAt":null}]}
            """
        try fixture.write(
            to: dir.appendingPathComponent("tickets.json"), atomically: false, encoding: .utf8)
        let store = Store(directory: dir)
        try FileManager.default.createDirectory(
            at: store.commentsDirectory, withIntermediateDirectories: true)
        try legacyCommentLine(author: "human", at: "2026-01-01T00:00:00Z", body: "legacy")
            .write(to: commentLog(store, id), atomically: false, encoding: .utf8)

        _ = try store.append(ticketID: id.uuidString, author: "builder", body: "new")

        #expect(try store.list(.all).value.map(\.title) == ["legacy"])
        #expect(try copies(of: "tickets.json", in: dir).count == 1)
        #expect(try store.list(ticketID: id.uuidString).value.map(\.body) == ["legacy", "new"])
    }

    @Test("comments read from the JSON log while the database has not imported them")
    func commentsReadFallback() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let ticket = try store.add("A ticket").value
        let log = commentLog(store, ticket.id)
        try FileManager.default.createDirectory(
            at: store.commentsDirectory, withIntermediateDirectories: true)
        try legacyCommentLine(author: "human", at: "2026-01-01T00:00:00Z", body: "legacy")
            .write(to: log, atomically: false, encoding: .utf8)

        // The ticket write created the database, but the comments domain has not
        // been imported yet: the log is still the store's truth.
        #expect(try store.list(ticketID: ticket.id.uuidString).value.map(\.body) == ["legacy"])
        #expect(try copies(of: log.lastPathComponent, in: store.commentsDirectory).isEmpty)
    }

    @Test("a second append never re-imports and never copies the log again")
    func commentsImportOnce() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let ticket = try store.add("A ticket").value
        let log = commentLog(store, ticket.id)
        try FileManager.default.createDirectory(
            at: store.commentsDirectory, withIntermediateDirectories: true)
        try legacyCommentLine(author: "human", at: "2026-01-01T00:00:00Z", body: "legacy")
            .write(to: log, atomically: false, encoding: .utf8)

        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "one")
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "two")

        #expect(try copies(of: log.lastPathComponent, in: store.commentsDirectory).count == 1)
        #expect(try store.list(ticketID: ticket.id.uuidString).value.map(\.body)
                == ["legacy", "one", "two"])
    }

    @Test("a damaged legacy log fails the first append and imports nothing")
    func damagedCommentLogFailsTheImport() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)
        let ticket = try store.add("A ticket").value
        let log = commentLog(store, ticket.id)
        try FileManager.default.createDirectory(
            at: store.commentsDirectory, withIntermediateDirectories: true)
        try Data("{not a comment\n".utf8).write(to: log)

        do {
            _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "one")
            Issue.record("expected a damaged-log failure")
        } catch let StoreIOError.io(detail) {
            #expect(detail.contains("damaged line"))
        }

        // Nothing was marked imported: the log is still the fallback's source,
        // and the failed write left no comment behind.
        try FileManager.default.removeItem(at: log)
        #expect(try store.list(ticketID: ticket.id.uuidString).value.isEmpty)
    }

    // MARK: - Sessions

    private func sessionRecord(
        id: String,
        project: String = "tandem",
        status: String = "ended",
        outcome: String? = "succeeded",
        startedAt: String = "2026-01-01T00:00:00Z",
        endedAt: String? = "2026-01-01T00:01:00Z",
        tokens: [String: Int]? = nil,
        cost: Double? = nil,
        model: String? = nil
    ) throws -> Data {
        var record: [String: Any] = [
            "id": id,
            "project": project,
            "agent": NSNull(),
            "brief": "Work on \(id).",
            "summary": "Done.",
            "status": status,
            "outcome": outcome ?? NSNull(),
            "tickets": [],
            "startedAt": startedAt,
            "endedAt": endedAt ?? NSNull(),
        ]
        if let tokens { record["tokens"] = tokens }
        if let cost { record["cost"] = cost }
        if let model { record["model"] = model }
        let data = try JSONSerialization.data(
            withJSONObject: ["version": 1, "sessions": [record]],
            options: [.prettyPrinted, .sortedKeys])
        return data
    }

    @Test("sessions.json imports on the first write and reads back from the database")
    func sessionsImportOnFirstWrite() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("sessions.json")
        try sessionRecord(
            id: "ses_legacy",
            tokens: ["input": 10, "output": 20, "reasoning": 30, "cacheRead": 40, "cacheWrite": 50],
            cost: 0.25,
            model: "deepseek-v4"
        ).write(to: file)

        let sessions = Sessions(directory: dir)
        let started = try sessions.start(id: "ses_new", project: "tandem", brief: "New.").value
        #expect(started.brief == "New.")

        // The source is copied aside and left where it was.
        #expect(FileManager.default.fileExists(atPath: file.path))
        #expect(try copies(of: "sessions.json", in: dir).count == 1)

        // Removing the file proves the read is the database's — usage included.
        try FileManager.default.removeItem(at: file)
        let listed = try sessions.list().value
        #expect(listed.map(\.id) == ["ses_new", "ses_legacy"])
        let legacy = try #require(listed.first { $0.id == "ses_legacy" })
        #expect(legacy.tokens == SessionTokens(
            input: 10, output: 20, reasoning: 30, cacheRead: 40, cacheWrite: 50))
        #expect(legacy.cost == 0.25)
        #expect(legacy.model == "deepseek-v4")
        #expect(legacy.startedAt == Timestamp.decode("2026-01-01T00:00:00Z"))
        #expect(legacy.endedAt == Timestamp.decode("2026-01-01T00:01:00Z"))
    }

    @Test("sessions read from the JSON file while the database has not imported them")
    func sessionsReadFallback() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try sessionRecord(id: "ses_legacy").write(
            to: dir.appendingPathComponent("sessions.json"))

        let sessions = Sessions(directory: dir)
        #expect(try sessions.list().value.map(\.id) == ["ses_legacy"])
        #expect(try copies(of: "sessions.json", in: dir).isEmpty)
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("tandem.db").path))
    }

    @Test("a second session write never re-imports")
    func sessionsImportOnce() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try sessionRecord(id: "ses_legacy").write(
            to: dir.appendingPathComponent("sessions.json"))
        let sessions = Sessions(directory: dir)
        _ = try sessions.start(id: "ses_a", project: "tandem", brief: "A.")
        _ = try sessions.start(id: "ses_b", project: "tandem", brief: "B.")
        #expect(try copies(of: "sessions.json", in: dir).count == 1)
        #expect(try sessions.list().value.count == 3)
    }

    @Test("a P1 database migrates to v2 and its sidecars round-trip the new columns")
    func v1DatabaseMigratesToV2() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = try SQLiteDatabase(
            url: dir.appendingPathComponent("tandem.db"), mode: .readWrite)
        try StoreSchema.migrate(db, migrations: [StoreSchema.migrations[0]])
        db.close()

        let sessions = Sessions(directory: dir)
        _ = try sessions.start(id: "ses_1", project: "tandem", brief: "Work.")
        _ = try sessions.end(
            id: "ses_1", outcome: .succeeded,
            tokens: SessionTokens(
                input: 1, output: 2, reasoning: 3, cacheRead: 4, cacheWrite: 5),
            cost: 0.5, model: "deepseek-v4")
        let session = try sessions.show(id: "ses_1").value
        #expect(session.tokens?.cacheWrite == 5)
        #expect(session.model == "deepseek-v4")

        let sprints = Sprints(directory: dir)
        _ = try sprints.open(lane: "Tandem")
        _ = try sprints.close(lane: "Tandem", number: 1, note: "Landed.")
        #expect(try sprints.show(lane: "Tandem", number: 1).value.close == "Landed.")

        let check = try SQLiteDatabase(
            url: dir.appendingPathComponent("tandem.db"), mode: .readOnly)
        defer { check.close() }
        #expect(try StoreSchema.version(of: check) == StoreSchema.currentVersion)
    }

    // MARK: - Sprints

    private func sprintsFixture() throws -> Data {
        let record: [String: Any] = [
            "lane": "Tandem",
            "number": 1,
            "session": "ses_open",
            "mode": "parallel",
            "batch": ["00000000-0000-0000-0000-000000000001"],
            "runs": ["2026-01-01T00:00:00Z — one landed"],
            "close": "Landed one.",
            "status": "closed",
            "openedAt": "2026-01-01T00:00:00Z",
            "closedAt": "2026-01-02T00:00:00Z",
        ]
        return try JSONSerialization.data(
            withJSONObject: ["version": 1, "sprints": [record]],
            options: [.prettyPrinted, .sortedKeys])
    }

    @Test("sprints.json imports on the first write and reads back through the database")
    func sprintsImportOnFirstWrite() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("sprints.json")
        try sprintsFixture().write(to: file)

        let sprints = Sprints(directory: dir)
        let opened = try sprints.open(lane: "Tandem").value
        #expect(opened.number == 2, "the imported Sprint 1 is never renumbered over")

        #expect(FileManager.default.fileExists(atPath: file.path))
        #expect(try copies(of: "sprints.json", in: dir).count == 1)

        try FileManager.default.removeItem(at: file)
        let listed = try sprints.list().value
        #expect(listed.map(\.name) == ["Sprint 2 — Tandem", "Sprint 1 — Tandem"])
        let legacy = try #require(listed.first { $0.number == 1 })
        #expect(legacy.session == "ses_open")
        #expect(legacy.mode == .parallel)
        #expect(legacy.batch == [UUID(uuidString: "00000000-0000-0000-0000-000000000001")!])
        #expect(legacy.runs == ["2026-01-01T00:00:00Z — one landed"])
        #expect(legacy.close == "Landed one.")
        #expect(legacy.status == .closed)
        #expect(legacy.closedAt == Timestamp.decode("2026-01-02T00:00:00Z"))
    }

    @Test("sprints read from the JSON file while the database has not imported them")
    func sprintsReadFallback() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try sprintsFixture().write(to: dir.appendingPathComponent("sprints.json"))

        let sprints = Sprints(directory: dir)
        #expect(try sprints.list().value.map(\.name) == ["Sprint 1 — Tandem"])
        #expect(try copies(of: "sprints.json", in: dir).isEmpty)
    }

    @Test("a second sprint write never re-imports")
    func sprintsImportOnce() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try sprintsFixture().write(to: dir.appendingPathComponent("sprints.json"))
        let sprints = Sprints(directory: dir)
        _ = try sprints.open(lane: "Tandem")
        _ = try sprints.open(lane: "Work")
        #expect(try copies(of: "sprints.json", in: dir).count == 1)
        #expect(try sprints.list().value.count == 3)
    }

    // MARK: - Settings

    private func settingsFixture() throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "bikeshedPath": "/tmp/Notes",
            "setupComplete": true,
            "defaultLane": "Board",
            "showHiddenFiles": true,
            "roster": ["human", "poet", "builder"],
        ], options: [.prettyPrinted, .sortedKeys])
    }

    @Test("settings.json imports on the first write and every setting reads back")
    func settingsImportOnFirstWrite() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("settings.json")
        try settingsFixture().write(to: file)

        let notes = dir.appendingPathComponent("Notes")
        try TandemSettings.setChosenBikeshed(notes, in: dir)

        #expect(FileManager.default.fileExists(atPath: file.path))
        #expect(try copies(of: "settings.json", in: dir).count == 1)

        // Removing the file proves the database is the settings store now.
        try FileManager.default.removeItem(at: file)
        #expect(TandemSettings.chosenBikeshed(in: dir) == notes.standardizedFileURL)
        #expect(TandemSettings.setupComplete(in: dir))
        #expect(TandemSettings.defaultLane(in: dir) == "Board")
        #expect(TandemSettings.showHiddenFiles(in: dir))
        #expect(TandemSettings.roster(in: dir) == ["human", "poet", "builder"])
    }

    @Test("a migrated database without the sidecar marker still serves the JSON")
    func unimportedTablesFallBackToJSON() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try sessionRecord(id: "ses_legacy").write(
            to: dir.appendingPathComponent("sessions.json"))

        // The crash shape: schema applied, no sessions marker, no rows.
        let db = try SQLiteDatabase(
            url: dir.appendingPathComponent("tandem.db"), mode: .readWrite)
        try StoreSchema.migrate(db)
        db.close()

        let sessions = Sessions(directory: dir)
        #expect(try sessions.list().value.map(\.id) == ["ses_legacy"])
        let check = try SQLiteDatabase(
            url: dir.appendingPathComponent("tandem.db"), mode: .readOnly)
        defer { check.close() }
        #expect(try !StoreSchema.isImported(StoreSchema.sessionsDomain, in: check))
    }

    @Test("settings read from the JSON file while the database has not imported them")
    func settingsReadFallback() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try settingsFixture().write(to: dir.appendingPathComponent("settings.json"))

        #expect(TandemSettings.chosenBikeshed(in: dir)
                == URL(fileURLWithPath: "/tmp/Notes").standardizedFileURL)
        #expect(TandemSettings.setupComplete(in: dir))
        #expect(TandemSettings.defaultLane(in: dir) == "Board")
        #expect(TandemSettings.showHiddenFiles(in: dir))
        #expect(TandemSettings.roster(in: dir) == ["human", "poet", "builder"])
        #expect(try copies(of: "settings.json", in: dir).isEmpty)
    }

    @Test("a second settings write never re-imports, and untouched keys survive")
    func settingsImportOnce() throws {
        let dir = try tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        try settingsFixture().write(to: dir.appendingPathComponent("settings.json"))
        try TandemSettings.setSetupComplete(false, in: dir)
        try TandemSettings.setDefaultLane(nil, in: dir)
        #expect(try copies(of: "settings.json", in: dir).count == 1)
        #expect(TandemSettings.setupComplete(in: dir) == false)
        #expect(TandemSettings.defaultLane(in: dir) == nil)
        #expect(TandemSettings.roster(in: dir) == ["human", "poet", "builder"])
    }
}
