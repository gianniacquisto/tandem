import Foundation
import Testing
@testable import TandemCore

/// The chosen-bikeshed settings record (ADR 0011):
/// `<home>/settings.json` holds `{"bikeshedPath": "<absolute path>"}`, the key
/// absent when no choice was made. Every test uses the explicit-home seam so
/// the process's real `TandemHome.directory` is never read or written.
@Suite("Tandem settings — chosen bikeshed")
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

    @Test("no settings file → no chosen bikeshed")
    func unsetIsNil() {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        #expect(TandemSettings.chosenBikeshed(in: home) == nil)
    }

    @Test("setting then getting round-trips the standardized absolute path")
    func roundTrip() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenBikeshed(chosen, in: home)
        #expect(TandemSettings.chosenBikeshed(in: home) == chosen.standardizedFileURL)
    }

    @Test("the on-disk file is valid JSON holding the bikeshedPath key")
    func fileShapeIsJSON() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenBikeshed(chosen, in: home)

        let data = try Data(contentsOf: settingsFile(in: home))
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(json?["bikeshedPath"] as? String == chosen.standardizedFileURL.path)
    }

    @Test("setting twice overwrites the previous choice")
    func secondSetWins() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setChosenBikeshed(home.appendingPathComponent("First"), in: home)
        let second = home.appendingPathComponent("Second")
        try TandemSettings.setChosenBikeshed(second, in: home)
        #expect(TandemSettings.chosenBikeshed(in: home) == second.standardizedFileURL)
    }

    @Test("a corrupt settings file reads as no choice, never a crash")
    func corruptFileIsNil() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data("not json at all".utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.chosenBikeshed(in: home) == nil)
    }

    @Test("valid JSON without the key reads as no choice")
    func missingKeyIsNil() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data(#"{"somethingElse": 1}"#.utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.chosenBikeshed(in: home) == nil)
    }

    // MARK: - First-run setup flag

    @Test("no settings file → setup not complete (the default)")
    func setupDefaultIsFalse() {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        #expect(TandemSettings.setupComplete(in: home) == false)
    }

    @Test("setting then getting round-trips the setup flag")
    func setupRoundTrip() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setSetupComplete(true, in: home)
        #expect(TandemSettings.setupComplete(in: home))
    }

    @Test("a false setup flag stays false")
    func setupExplicitFalse() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setSetupComplete(true, in: home)
        try TandemSettings.setSetupComplete(false, in: home)
        #expect(TandemSettings.setupComplete(in: home) == false)
    }

    @Test("the two settings coexist: a bikeshed choice survives the setup write")
    func setupWriteKeepsBikeshed() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenBikeshed(chosen, in: home)
        try TandemSettings.setSetupComplete(true, in: home)
        #expect(TandemSettings.chosenBikeshed(in: home) == chosen.standardizedFileURL)
        #expect(TandemSettings.setupComplete(in: home))
    }

    @Test("the two settings coexist: the setup flag survives a bikeshed write")
    func bikeshedWriteKeepsSetup() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setSetupComplete(true, in: home)
        let chosen = home.appendingPathComponent("Notes")
        try TandemSettings.setChosenBikeshed(chosen, in: home)
        #expect(TandemSettings.setupComplete(in: home))
        #expect(TandemSettings.chosenBikeshed(in: home) == chosen.standardizedFileURL)
    }

    @Test("a corrupt settings file reads as setup not complete, never a crash")
    func corruptFileIsNotComplete() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data("not json at all".utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.setupComplete(in: home) == false)
    }

    @Test("valid JSON without the setup key reads as not complete")
    func missingSetupKeyIsFalse() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data(#"{"bikeshedPath": "/tmp/Notes"}"#.utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.setupComplete(in: home) == false)
    }

    // MARK: - the lane new tickets are filed into (the cached default lane)

    @Test("no default lane → new tickets go to the default area")
    func noDefaultLane() {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        #expect(TandemSettings.defaultLane(in: home) == nil)
    }

    @Test("the default lane round-trips, trimmed")
    func defaultLaneRoundTrip() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setDefaultLane("  Board  ", in: home)
        #expect(TandemSettings.defaultLane(in: home) == "Board")
    }

    @Test("clearing the default lane removes the key; the other settings stay")
    func clearingDefaultLane() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setDefaultLane("Board", in: home)
        try TandemSettings.setSetupComplete(true, in: home)
        try TandemSettings.setDefaultLane(nil, in: home)

        #expect(TandemSettings.defaultLane(in: home) == nil)
        #expect(TandemSettings.setupComplete(in: home))
        let json = try JSONSerialization.jsonObject(with: Data(contentsOf: settingsFile(in: home)))
        let object = try #require(json as? [String: Any])
        #expect(object["defaultLane"] == nil)
    }

    @Test("an empty or whitespace default lane is the default area, not a lane name")
    func blankDefaultLaneIsNil() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setDefaultLane("   ", in: home)
        #expect(TandemSettings.defaultLane(in: home) == nil)
    }

    // MARK: - The hidden layer (the reveal flag)

    @Test("no settings file → the hidden layer stays hidden (the default)")
    func showHiddenFilesDefaultIsFalse() {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        #expect(TandemSettings.showHiddenFiles(in: home) == false)
    }

    @Test("the reveal flag round-trips")
    func showHiddenFilesRoundTrip() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setShowHiddenFiles(true, in: home)
        #expect(TandemSettings.showHiddenFiles(in: home))
        try TandemSettings.setShowHiddenFiles(false, in: home)
        #expect(TandemSettings.showHiddenFiles(in: home) == false)
    }

    @Test("the on-disk file is valid JSON holding the showHiddenFiles key")
    func showHiddenFilesFileShape() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setShowHiddenFiles(true, in: home)

        let data = try Data(contentsOf: settingsFile(in: home))
        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        #expect(json?["showHiddenFiles"] as? Bool == true)
    }

    @Test("the reveal flag coexists with the other settings")
    func showHiddenFilesKeepsOtherSettings() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try TandemSettings.setChosenBikeshed(home.appendingPathComponent("Notes"), in: home)
        try TandemSettings.setSetupComplete(true, in: home)
        try TandemSettings.setDefaultLane("Board", in: home)
        try TandemSettings.setShowHiddenFiles(true, in: home)

        #expect(TandemSettings.chosenBikeshed(in: home)
                == home.appendingPathComponent("Notes").standardizedFileURL)
        #expect(TandemSettings.setupComplete(in: home))
        #expect(TandemSettings.defaultLane(in: home) == "Board")
        #expect(TandemSettings.showHiddenFiles(in: home))
    }

    @Test("a corrupt settings file reads as hidden, never a crash")
    func showHiddenFilesCorruptIsFalse() throws {
        let home = tempHome()
        defer { try? FileManager.default.removeItem(at: home) }
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        try Data("not json at all".utf8).write(to: settingsFile(in: home))
        #expect(TandemSettings.showHiddenFiles(in: home) == false)
    }
}
