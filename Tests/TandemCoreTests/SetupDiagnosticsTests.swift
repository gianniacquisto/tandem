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
        storeHealth: Store.Health = .counts(open: 3, done: 2),
        storeLockHeld: Bool = false,
        bikeshedNoteCount: Int = 4,
        bikeshedIsFile: Bool = false,
        appPresent: Bool = true,
        appVersion: String? = "0.9.0",
        appQuarantined: Bool = false,
        skillHosts: [SetupDiagnostics.SkillHost] = [
            .init(name: "agents", path: "/Users/tester/.agents/skills/tandem", found: true),
            .init(name: "claude", path: "/Users/tester/.claude/skills/tandem", found: false),
        ],
        opencodeListsSkills: Bool = true
    ) -> SetupDiagnostics.Facts {
        SetupDiagnostics.Facts(
            homePath: "/Users/tester/.tandem-dev",
            homeSource: homeSource,
            storePath: "/Users/tester/.tandem-dev/todos.json",
            storeHealth: storeHealth,
            storeLockHeld: storeLockHeld,
            bikeshedPath: "/Users/tester/.tandem-dev/bikeshed",
            bikeshedNoteCount: bikeshedNoteCount,
            bikeshedIsFile: bikeshedIsFile,
            appPresent: appPresent,
            appVersion: appVersion,
            appQuarantined: appQuarantined,
            skillHosts: skillHosts,
            opencodeListsSkills: opencodeListsSkills)
    }

    @Test("a healthy machine passes, with the checks in the pinned order")
    func healthyMachinePasses() {
        let report = SetupDiagnostics.report(facts())
        #expect(report.ok)
        #expect(report.checks.map(\.name) == SetupDiagnostics.checkNames)
        #expect(report.checks.allSatisfy { $0.ok })
        #expect(report.checks[0].detail.contains("dev build"))
        #expect(report.checks[1].detail.contains("3 open, 2 done"))
        #expect(report.checks[2].detail.contains("4 note(s)"))
        #expect(report.checks[4].detail.contains("version 0.9.0"))
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

        try Data("not json".utf8).write(to: dir.appendingPathComponent("todos.json"))
        #expect(store.health() == .unreadable)
    }

    @Test("the lock probe reports free when nothing holds the lock")
    func lockFree() throws {
        let (store, dir) = tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        #expect(store.lockIsHeld() == false)
    }
}
