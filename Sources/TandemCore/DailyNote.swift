import Foundation

/// Daily notes (ticket EE991EF2): the human's own folder at the vault root.
///
/// The human writes one file per day, `daily/YYYY-MM-DD.md`, from the app's
/// **Today's note** action (⌘K or the File menu). The consolidation pass reads
/// them and folds what matters into the curated notes; it **never edits, moves or
/// archives** one — the daily note is the human's, and there is no agent-facing
/// write path for it.
///
/// The deterministic half of the contract lives here so the app, the tests and the
/// protocol's prose agree: the harvest list, the day's path, the fold marker's
/// wording and parse, and the pending set — the files dated after the marker, found
/// by name and **never by mtime** (a touch must not re-fold a note).
public enum DailyNote {

    /// The **harvest list**: the human-owned folders at the vault root, read-only to
    /// every agent. One entry today; a second folder joins by being declared here,
    /// with the same contract, and its marker line carries the folder's name.
    public static let harvestFolders = ["daily"]

    /// The fixed wording of the fold position in `Index.md`:
    /// `Human notes folded through 2026-10-26`.
    public static let markerPrefix = "Human notes folded through"

    /// `daily/<YYYY-MM-DD>.md` for `day` — the calendar decides which day it is.
    public static func path(on day: Date, calendar: Calendar = .current) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateFormat = "yyyy-MM-dd"
        return "\(harvestFolders[0])/\(formatter.string(from: day)).md"
    }

    /// The day the fold marker names (`YYYY-MM-DD`), or `nil` when `Index.md` has no
    /// marker yet or the line names no day.
    public static func foldedThrough(in indexText: String) -> String? {
        for line in indexText.split(separator: "\n", omittingEmptySubsequences: false) {
            guard let marker = line.range(of: markerPrefix) else { continue }
            let day = line[marker.upperBound...]
                .trimmingCharacters(in: CharacterSet(charactersIn: " \t*."))
            if isDay(day) { return day }
        }
        return nil
    }

    /// The day a daily note's file name carries — `2026-10-26` for
    /// `2026-10-26.md`, `nil` for anything else.
    static func day(ofFile name: String) -> String? {
        guard name.lowercased().hasSuffix(".md") else { return nil }
        let day = String(name.dropLast(3))
        return isDay(day) ? day : nil
    }

    /// Whether the text is exactly `YYYY-MM-DD` — the only shape a day takes.
    private static func isDay(_ text: String) -> Bool {
        let characters = Array(text)
        guard characters.count == 10 else { return false }
        for (index, character) in characters.enumerated() {
            if index == 4 || index == 7 {
                guard character == "-" else { return false }
            } else {
                guard character.isASCII, character.isNumber else { return false }
            }
        }
        return true
    }
}

// MARK: - The app's open-or-create, and the pass's pending set

extension Bikeshed {

    /// Open-or-create the day's note: today's `daily/<today>.md` (the app's
    /// **Today's note** action).
    ///
    /// An existing file is returned untouched — the human's words are never
    /// overwritten; a missing one is created empty, as "New note" is. Lock-free on
    /// the existing path; the create takes the bikeshed lock.
    public func openDailyNote(
        on day: Date = .now, calendar: Calendar = .current
    ) throws -> OperationResult<String> {
        let path = DailyNote.path(on: day, calendar: calendar)
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)
        }
        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
            guard !isDirectory.boolValue else {
                throw BikeshedIOError.io("path \"\(path)\" is a folder")
            }
            return OperationResult(path)
        }
        return try create(path: path)
    }

    /// The daily notes the consolidation pass has not folded yet: every file in the
    /// harvest list whose name's day is **after** `Index.md`'s fold marker
    /// (bikeshed-root-relative, sorted).
    ///
    /// Discovery is the list plus the marker date, not mtime: a touch cannot re-fold
    /// a note, and an edited old note stays the human's to re-date. No marker yet
    /// means every daily note is pending. Read-only and lock-free — the pass's
    /// judgment (which curated note holds the fold) is not here, only the set.
    public func pendingDailyNotes() throws -> OperationResult<[String]> {
        let index = (try? String(
            contentsOf: directory.appendingPathComponent("Index.md"), encoding: .utf8)) ?? ""
        let marker = DailyNote.foldedThrough(in: index)
        var pending: [String] = []
        for folder in DailyNote.harvestFolders {
            let directory = self.directory.appendingPathComponent(folder)
            for url in files(under: directory, recursive: false, pruningHidden: true) {
                guard let day = DailyNote.day(ofFile: url.lastPathComponent) else { continue }
                if let marker, day <= marker { continue }
                pending.append("\(folder)/\(url.lastPathComponent)")
            }
        }
        return OperationResult(pending.sorted())
    }
}
