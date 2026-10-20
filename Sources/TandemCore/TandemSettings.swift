import Foundation
import Darwin

/// The persisted, human-chosen settings Tandem keeps beside its other state:
/// the bikeshed, the first-run setup flag, and whether the bikeshed's hidden
/// layer is revealed, in `<TandemHome.directory>/settings.json` (ADR 0011). One
/// tiny `{"bikeshedPath": "<absolute path>", "setupComplete": true,
/// "showHiddenFiles": false}` record, deliberately separate from the store
/// schema and the bikeshed itself — a settings file is not domain data, and a
/// new setting is one more key, not a schema version bump.
///
/// A key is **absent when it was never set**, which is the default state:
/// `chosenBikeshed()` returning `nil` means "wherever the default is"
/// (`<home>/bikeshed`; see `Bikeshed.init()`), and `setupComplete()` returning
/// `false` means "the setup wizard has not been finished or skipped". Reads
/// never create the file and never throw — a missing, unreadable, or malformed
/// file is simply the default, because a broken settings file must never take
/// the app down. Writes create the home directory if needed, merge onto the
/// existing record (so a bikeshed choice and the setup flag never clobber each
/// other), and replace the file atomically (a same-directory temp file +
/// `rename(2)`).
public enum TandemSettings {

    /// The key inside `settings.json` holding the chosen bikeshed's absolute path.
    private static let bikeshedKey = "bikeshedPath"
    /// The key inside `settings.json` holding whether first-run setup finished.
    private static let setupKey = "setupComplete"
    /// The key inside `settings.json` holding the lane new tickets are filed into: the
    /// human's last choice, cached so a session of filing does not repeat it.
    private static let defaultLaneKey = "defaultLane"
    /// The key inside `settings.json` holding whether the bikeshed's hidden layer —
    /// the capture saddlebag (`.raw/`) and the consolidation archive (`.raw/.done/`)
    /// — is revealed in the tree. Off by default: the curated layer is the
    /// reading surface, and the hidden layer is revealed only on request.
    private static let showHiddenFilesKey = "showHiddenFiles"
    /// The settings file's name, under `TandemHome.directory`.
    private static let fileName = "settings.json"

    /// The human's chosen bikeshed, or `nil` when none was chosen (or the file is
    /// missing, unreadable, or malformed). Reads the real
    /// `TandemHome.directory`; returns a standardized absolute URL.
    public static func chosenBikeshed() -> URL? {
        chosenBikeshed(in: TandemHome.directory)
    }

    /// Persist `url` as the chosen bikeshed, writing `settings.json` atomically
    /// under the real `TandemHome.directory`.
    public static func setChosenBikeshed(_ url: URL) throws {
        try setChosenBikeshed(url, in: TandemHome.directory)
    }

    /// Whether the first-run setup has been finished or skipped, read from the
    /// real `TandemHome.directory`. `false` when the flag was never written —
    /// including a missing, unreadable, or malformed settings file (the same
    /// leniency as `chosenBikeshed()`: a broken file re-shows the wizard rather
    /// than crashing the app).
    public static func setupComplete() -> Bool {
        setupComplete(in: TandemHome.directory)
    }

    /// Record that first-run setup is done, writing `settings.json` atomically
    /// under the real `TandemHome.directory`.
    public static func setSetupComplete(_ done: Bool) throws {
        try setSetupComplete(done, in: TandemHome.directory)
    }

    /// The lane new tickets are filed into, or `nil` for the default (unnamed) area.
    /// A cache of the human's last choice, not a store field: an agent has `--lane`, and
    /// a lane that no longer exists is the GUI's to clamp — the settings layer only
    /// remembers a name.
    public static func defaultLane() -> String? {
        defaultLane(in: TandemHome.directory)
    }

    /// Remember (or, with `nil`, forget) the lane new tickets are filed into. Writes
    /// `settings.json` atomically under the real `TandemHome.directory`.
    public static func setDefaultLane(_ lane: String?) throws {
        try setDefaultLane(lane, in: TandemHome.directory)
    }

    /// Whether the bikeshed tree reveals the hidden layer — the capture
    /// saddlebag (`projects/<project>/.raw/`) and the consolidation archive
    /// (`projects/<project>/.raw/.done/`). Read from the real
    /// `TandemHome.directory`; `false` when the flag was never written (the
    /// default: the curated layer is the reading surface).
    public static func showHiddenFiles() -> Bool {
        showHiddenFiles(in: TandemHome.directory)
    }

    /// Record whether the hidden layer is revealed, writing `settings.json`
    /// atomically under the real `TandemHome.directory`.
    public static func setShowHiddenFiles(_ shown: Bool) throws {
        try setShowHiddenFiles(shown, in: TandemHome.directory)
    }

    /// The testable seam: `chosenBikeshed()` against an explicit home directory,
    /// so tests never read or write the process's real home.
    static func chosenBikeshed(in home: URL) -> URL? {
        let object = object(in: home)
        guard let path = object[bikeshedKey] as? String, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path).standardizedFileURL
    }

    /// The testable seam: `defaultLane()` against an explicit home directory.
    static func defaultLane(in home: URL) -> String? {
        guard let name = object(in: home)[defaultLaneKey] as? String else { return nil }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// The testable seam: `setDefaultLane(_:)` against an explicit home directory. An
    /// empty or `nil` lane removes the key — absent means the default area, the same
    /// shape as every other setting that was never set.
    static func setDefaultLane(_ lane: String?, in home: URL) throws {
        var settings = object(in: home)
        let trimmed = lane?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let trimmed, !trimmed.isEmpty {
            settings[defaultLaneKey] = trimmed
        } else {
            settings.removeValue(forKey: defaultLaneKey)
        }
        try write(settings, in: home)
    }

    /// The testable seam: `setupComplete()` against an explicit home directory.
    static func setupComplete(in home: URL) -> Bool {
        object(in: home)[setupKey] as? Bool ?? false
    }

    /// The testable seam: `showHiddenFiles()` against an explicit home directory.
    /// The key absent means the default — the hidden layer stays hidden.
    static func showHiddenFiles(in home: URL) -> Bool {
        object(in: home)[showHiddenFilesKey] as? Bool ?? false
    }

    /// The testable seam: `setShowHiddenFiles(_:)` against an explicit home
    /// directory. Merges the key onto the existing record (the bikeshed choice
    /// and the setup flag are preserved), then writes atomically.
    static func setShowHiddenFiles(_ shown: Bool, in home: URL) throws {
        var settings = object(in: home)
        settings[showHiddenFilesKey] = shown
        try write(settings, in: home)
    }

    /// The testable seam: `setChosenBikeshed(_:)` against an explicit home
    /// directory. Merges the key onto the existing record (keeping any other
    /// settings), then writes atomically.
    static func setChosenBikeshed(_ url: URL, in home: URL) throws {
        var settings = object(in: home)
        settings[bikeshedKey] = url.standardizedFileURL.path
        try write(settings, in: home)
    }

    /// The testable seam: `setSetupComplete(_:)` against an explicit home
    /// directory. Merges the key onto the existing record (a bikeshed choice is
    /// preserved), then writes atomically.
    static func setSetupComplete(_ done: Bool, in home: URL) throws {
        var settings = object(in: home)
        settings[setupKey] = done
        try write(settings, in: home)
    }

    /// The settings file parsed as a JSON object, or `[:]` when it is missing,
    /// unreadable, or malformed — the read leniency, shared by every getter.
    private static func object(in home: URL) -> [String: Any] {
        let file = home.appendingPathComponent(fileName)
        guard let data = try? Data(contentsOf: file),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any]
        else { return [:] }
        return object
    }

    /// Write `settings` atomically: create `home` if needed, write a
    /// same-directory temp file, then `rename(2)` it over `settings.json` — a
    /// reader ever sees the complete old or new file, never a torn one.
    private static func write(_ settings: [String: Any], in home: URL) throws {
        let fm = FileManager.default
        try fm.createDirectory(at: home, withIntermediateDirectories: true)
        let data = try JSONSerialization.data(withJSONObject: settings,
                                              options: [.sortedKeys])
        let destination = home.appendingPathComponent(fileName)
        let temp = home.appendingPathComponent(".settings-\(UUID().uuidString).tmp")
        do {
            try data.write(to: temp, options: [])
            if Darwin.rename(temp.path, destination.path) != 0 {
                throw TandemSettingsError.write(op: "rename", code: errno)
            }
        } catch {
            try? fm.removeItem(at: temp)
            throw error
        }
    }
}

/// The settings file's write failure — short, actionable, the `TandemError`
/// convention, so the GUI surfaces the same text it does for every op. Reads
/// never fail (a broken file is "no choice"), so this is write-only.
enum TandemSettingsError: Error, Sendable {
    case write(op: String, code: Int32)

    var message: String {
        switch self {
        case .write(let op, let code):
            let detail = String(cString: strerror(code))
            return "Could not save Tandem settings during \(op) (\(code): \(detail)). Try again."
        }
    }
}

extension TandemSettingsError: TandemError {}
