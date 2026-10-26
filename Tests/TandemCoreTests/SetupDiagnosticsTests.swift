import Foundation
import Testing
@testable import TandemCore

/// The setup report (`SetupDiagnostics`, ADR 0014): the pure assembly, and the
/// store's health read that feeds it. The impure gathering is deliberately not
/// tested — it only reads the machine; everything with a decision in it is
/// `report(_:)`, which these tests drive with injected facts.
@Suite("Setup diagnostics")
struct SetupDiagnosticsTests {

    /// Facts for a healthy machine, overridable per test.
    private func facts(
        homeSource: TandemHome.Source = .devBuild,
        storePath: String = "/Users/tester/.tandem-dev/tandem.db",
        storeVersion: Int? = 1,
        storeJSONFork: String? = nil,
        storeHealth: Store.Health = .counts(open: 3, done: 2),
        storeBackup: Store.BackupHealth = .present(age: 300),
        storeLockHeld: Bool = false,
        bikeshedNoteCount: Int = 4,
        bikeshedIsFile: Bool = false,
        commentsState: SetupDiagnostics.CommentsState = .absent,
        appPresent: Bool = true,
        appVersion: String? = "0.9.0",
        appQuarantined: Bool = false,
        skillHosts: [SetupDiagnostics.SkillHost] = [
            .init(name: "agents", path: "/Users/tester/.agents/skills/tandem", found: true),
            .init(name: "claude", path: "/Users/tester/.claude/skills/tandem", found: false),
        ],
        opencodeHasSkills: Bool = true
    ) -> SetupDiagnostics.Facts {
        SetupDiagnostics.Facts(
            homePath: "/Users/tester/.tandem-dev",
            homeSource: homeSource,
            storePath: storePath,
            storeVersion: storeVersion,
            storeJSONFork: storeJSONFork,
            storeHealth: storeHealth,
            storeBackup: storeBackup,
            storeLockHeld: storeLockHeld,
            bikeshedPath: "/Users/tester/.tandem-dev/bikeshed",
            bikeshedNoteCount: bikeshedNoteCount,
            bikeshedIsFile: bikeshedIsFile,
            commentsPath: "/Users/tester/.tandem-dev/comments",
            commentsState: commentsState,
            appPresent: appPresent,
            appVersion: appVersion,
            appQuarantined: appQuarantined,
            skillHosts: skillHosts,
            opencodeHasSkills: opencodeHasSkills)
    }

    @Test("a healthy machine passes, with the checks in the pinned order")
    func healthyMachinePasses() {
        let report = SetupDiagnostics.report(facts())
        #expect(report.ok)
        #expect(report.checks.map(\.name) == SetupDiagnostics.checkNames)
        #expect(report.checks.allSatisfy { $0.ok })
        #expect(report.checks[0].detail.contains("dev build"))
        #expect(report.checks[1].detail.contains("3 open, 2 done"))
        #expect(report.checks[1].detail.contains("backup 5m old"))
        #expect(report.checks[2].detail.contains("4 note(s)"))
        #expect(report.checks[3].detail.contains("created on first comment"))
        #expect(report.checks[5].detail.contains("version 0.9.0"))
    }

    @Test("an absent store is a valid empty setup, not a failure")
    func absentStoreIsOK() {
        let report = SetupDiagnostics.report(facts(storeHealth: .absent))
        #expect(report.ok)
        #expect(report.checks.first { $0.name == "store" }?.detail.contains("absent") == true)
    }

    @Test("an unreadable store fails the report")
    func unreadableStoreFails() {
        let report = SetupDiagnostics.report(facts(storeHealth: .unreadable))
        #expect(!report.ok)
        #expect(report.checks.first { $0.name == "store" }?.ok == false)
    }

    @Test("the store check reports the rolling backup's age, or its absence")
    func storeDetailReportsTheBackup() {
        let withBackup = SetupDiagnostics.report(facts(storeBackup: .present(age: 300)))
        #expect(withBackup.checks[1].detail.contains("backup 5m old"))
        #expect(withBackup.ok)

        let without = SetupDiagnostics.report(facts(storeBackup: .absent))
        #expect(without.checks[1].detail.contains("no backup yet"))
        #expect(without.ok, "a store too young to have a backup is not a failure")
    }

    @Test("an old backup on a corrupt store is still reported beside the failure")
    func corruptStoreStillNamesItsBackup() {
        let report = SetupDiagnostics.report(
            facts(storeHealth: .unreadable, storeBackup: .present(age: 7_200)))
        let store = report.checks.first { $0.name == "store" }
        #expect(store?.ok == false)
        #expect(store?.detail.contains("backup 2h old") == true)
    }

    @Test("the store row names the database, its migration version and the JSON fork")
    func storeRowNamesTheDatabaseVersionAndFork() {
        let report = SetupDiagnostics.report(facts(
            storePath: "/Users/tester/.tandem-dev/tandem.db",
            storeVersion: 1,
            storeJSONFork: "/Users/tester/.tandem-dev/tickets.json"))
        let store = report.checks.first { $0.name == "store" }
        #expect(store?.detail.contains("/Users/tester/.tandem-dev/tandem.db") == true)
        #expect(store?.detail.contains("v1;") == true)
        #expect(store?.detail.contains("a fork") == true)
        #expect(store?.detail.contains("tickets.json") == true)
    }

    @Test("backup ages read as seconds, minutes, hours, then days")
    func backupAgeUnits() {
        #expect(SetupDiagnostics.ageText(0) == "0s")
        #expect(SetupDiagnostics.ageText(59) == "59s")
        #expect(SetupDiagnostics.ageText(60) == "1m")
        #expect(SetupDiagnostics.ageText(3_599) == "59m")
        #expect(SetupDiagnostics.ageText(3_600) == "1h")
        #expect(SetupDiagnostics.ageText(86_399) == "23h")
        #expect(SetupDiagnostics.ageText(86_400) == "1d")
    }

    @Test("a held lock fails the report, with a retry hint")
    func heldLockFails() {
        let report = SetupDiagnostics.report(facts(storeLockHeld: true))
        #expect(!report.ok)
        let lock = report.checks.first { $0.name == "lock" }
        #expect(lock?.ok == false)
        #expect(lock?.detail.contains("retry") == true)
    }

    @Test("a bikeshed path that is a file fails the report")
    func bikeshedFileFails() {
        let report = SetupDiagnostics.report(facts(bikeshedIsFile: true))
        #expect(!report.ok)
        #expect(report.checks.first { $0.name == "bikeshed" }?.ok == false)
    }

    @Test("an absent comments directory is a valid setup, created on first comment")
    func absentCommentsIsOK() {
        let report = SetupDiagnostics.report(facts(commentsState: .absent))
        let comments = report.checks.first { $0.name == "comments" }
        #expect(report.ok)
        #expect(comments?.ok == true)
        #expect(comments?.detail.contains("/Users/tester/.tandem-dev/comments") == true)
        #expect(comments?.detail.contains("created on first comment") == true)
    }

    @Test("an unreadable comments path fails the report, naming the directory")
    func unreadableCommentsFails() {
        let report = SetupDiagnostics.report(facts(commentsState: .unreadable))
        let comments = report.checks.first { $0.name == "comments" }
        #expect(!report.ok)
        #expect(comments?.ok == false)
        #expect(comments?.detail.contains("/Users/tester/.tandem-dev/comments") == true)
        #expect(comments?.detail.contains("unreadable") == true)
    }

    @Test("a missing app is informational — not installed, still ok")
    func missingAppIsInformational() {
        let report = SetupDiagnostics.report(facts(appPresent: false, appVersion: nil))
        let app = report.checks.first { $0.name == "app" }
        #expect(app?.ok == true)
        #expect(app?.detail.contains("not installed") == true)
        #expect(report.ok)
    }

    @Test("a quarantined app names the quarantine strip")
    func quarantinedAppHints() {
        let report = SetupDiagnostics.report(facts(appQuarantined: true))
        #expect(report.checks.first { $0.name == "app" }?.detail.contains("xattr -dr") == true)
    }

    @Test("the skills check reports each host and the opencode config")
    func skillsDetail() {
        let report = SetupDiagnostics.report(facts())
        let skills = report.checks.first { $0.name == "skills" }?.detail ?? ""
        #expect(skills.contains("agents found"))
        #expect(skills.contains("claude not found"))
        #expect(skills.contains("opencode configured"))
    }

    @Test("the home check names which branch of the rule this process takes")
    func homeSources() {
        let shipped = SetupDiagnostics.report(facts(homeSource: .shippedApp))
        #expect(shipped.checks[0].detail.contains("shipped app"))
        let override = SetupDiagnostics.report(facts(homeSource: .environmentOverride))
        #expect(override.checks[0].detail.contains("TANDEM_HOME"))
    }

    // MARK: - Store.health / lock probe (the impure read's decisions)

    private func tempStore() -> (Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-doctor-tests-\(UUID().uuidString)")
        return (Store(directory: dir), dir)
    }

    @Test("health: no file is absent, a write is counts, rubbish is unreadable")
    func storeHealth() throws {
        let (store, dir) = tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        #expect(store.health() == .absent)
        _ = try store.add("Doctor probe")
        #expect(store.health() == .counts(open: 1, done: 0))

        // A file that is not a database where the store's database belongs.
        try Data("not a database".utf8).write(to: store.databaseURL)
        #expect(store.health() == .unreadable)
    }

    @Test("the lock probe reports free when nothing holds the lock")
    func lockFree() throws {
        let (store, dir) = tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        #expect(store.lockIsHeld() == false)
    }

    @Test("backupHealth: no file is absent, one write is still absent, two writes age it")
    func storeBackupHealth() throws {
        let (store, dir) = tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }

        #expect(store.backupHealth() == .absent)
        _ = try store.add("First probe")
        #expect(store.backupHealth() == .absent, "a first write has no file to roll")
        _ = try store.add("Second probe")

        guard case .present(let age) = store.backupHealth() else {
            Issue.record("two writes should leave a backup")
            return
        }
        #expect(age < 60)
    }
}
