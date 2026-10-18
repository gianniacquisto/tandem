import Foundation
import Darwin

/// The persisted, human-chosen settings Tandem keeps beside its other state:
/// today just the bikeshed, in `<TandemHome.directory>/settings.json`
/// (ADR 0011). One tiny `{"bikeshedPath": "<absolute path>"}`
/// record, deliberately separate from the store schema and the bikeshed itself —
/// a settings file is not domain data, and a new setting is one more key, not
/// a schema version bump.
///
/// The key is **absent when no choice was ever made**, which is the default
/// state: `chosenBikeshed()` returning `nil` means "wherever the default is"
/// (`<home>/bikeshed`; see `Bikeshed.init()`). Reads never create the file and
/// never throw — a missing, unreadable, or malformed file is simply "no
/// choice" (`nil`), because a broken settings file must never take the app
/// down. Writes create the home directory if needed and replace the file
/// atomically (a same-directory temp file + `rename(2)`).
public enum TandemSettings {

    /// The key inside `settings.json` holding the chosen bikeshed's absolute path.
    private static let bikeshedKey = "bikeshedPath"
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

    /// The testable seam: `chosenBikeshed()` against an explicit home directory,
    /// so tests never read or write the process's real home.
    static func chosenBikeshed(in home: URL) -> URL? {
        let file = home.appendingPathComponent(fileName)
        guard let data = try? Data(contentsOf: file),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any],
              let path = object[bikeshedKey] as? String,
              !path.isEmpty
        else { return nil }
        return URL(fileURLWithPath: path).standardizedFileURL
    }

    /// The testable seam: `setChosenBikeshed(_:)` against an explicit home
    /// directory. Creates `home` if needed, then writes a same-directory temp
    /// file and `rename(2)`s it over `settings.json` — a reader ever sees the
    /// complete old or new file, never a torn one.
    static func setChosenBikeshed(_ url: URL, in home: URL) throws {
        let fm = FileManager.default
        try fm.createDirectory(at: home, withIntermediateDirectories: true)
        let data = try JSONSerialization.data(
            withJSONObject: [bikeshedKey: url.standardizedFileURL.path],
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
