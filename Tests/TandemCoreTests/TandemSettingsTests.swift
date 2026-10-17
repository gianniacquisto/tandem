import Foundation
import Testing
@testable import TandemCore

/// The chosen-vault settings record (ADR 0011, "A chosen vault"):
/// `<home>/settings.json` holds `{"vaultPath": "<absolute path>"}`, the key
/// absent when no choice was made. Every test uses the explicit-home seam so
/// the process's real `TandemHome.directory` is never read or written.
@Suite("Tandem settings — chosen vault")
struct TandemSettingsTests {

    /// A unique temp home directory (not pre-created: creating it is the
    /// writer's job).
    private func tempHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-settings-\(UUID().uuidString)")
    }

    private func settingsFile(in home: URL) -> URL {
        home.appendingPathComponent("settings.json")
    }

    @Test("no settings file → no chosen vault")
    func unsetIsNil() {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        #expect(TandemSettings.chosenVault(in: home) == nil)
    }

    @Test("setting then getting round-trips the standardized absolute path")
    func roundTrip() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenVault(chosen, in: home)
        #expect(TandemSettings.chosenVault(in: home) == chosen.standardizedFileURL)
    }

    @Test("the on-disk file is valid JSON holding the vaultPath key")
    func fileShapeIsJSON() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenVault(chosen, in: home)

        let data = try Data(contentsOf: settingsFile(in: home))
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(json?["vaultPath"] as? String == chosen.standardizedFileURL.path)
    }

    @Test("setting twice overwrites the previous choice")
    func secondSetWins() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setChosenVault(home.appendingPathComponent("First"), in: home)
        let second = home.appendingPathComponent("Second")
        try TandemSettings.setChosenVault(second, in: home)
        #expect(TandemSettings.chosenVault(in: home) == second.standardizedFileURL)
    }

    @Test("a corrupt settings file reads as no choice, never a crash")
    func corruptFileIsNil() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data("not json at all".utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.chosenVault(in: home) == nil)
    }

    @Test("valid JSON without the key reads as no choice")
    func missingKeyIsNil() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data(#"{"somethingElse": 1}"#.utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.chosenVault(in: home) == nil)
    }
}
