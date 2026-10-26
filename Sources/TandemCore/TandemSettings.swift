import Foundation
import Darwin

/// The persisted, human-chosen settings Tandem keeps beside its other state:
/// the bikeshed, the first-run setup flag, whether the bikeshed's hidden
/// layer is revealed, and the reassignment roster, in the `settings` table of
/// `<TandemHome.directory>/tandem.db` (ADR 0011, re-backed by ADR 0031).
/// **A setting is a row, not a schema bump** — the old docstring's reason for
/// a separate file ("deliberately separate from the store schema") died with
/// the one-database decision; adding a setting is still one more `key`/`value`
/// row, never a migration. The legacy `settings.json` is the read-fallback and
/// the one-time import's source.
///
/// A key is **absent when it was never set**, which is the default state:
/// `chosenBikeshed()` returning `nil` means "wherever the default is"
/// (`<home>/bikeshed`; see `Bikeshed.init()`), and `setupComplete()` returning
/// `false` means "the setup wizard has not been finished or skipped". Reads never
/// create anything and never throw — an absent row, or a missing, unreadable, or
/// malformed legacy file, is simply the default, because a broken settings record
/// must never take the app down. Writes create the home directory if needed, merge
/// onto the existing keys (so a bikeshed choice and the setup flag never clobber
/// each other), and land in one database transaction.
public enum TandemSettings {

    /// The `settings` table's key holding the chosen bikeshed's absolute path.
    private static let bikeshedKey = "bikeshedPath"
    /// The `settings` table's key holding whether first-run setup finished.
    private static let setupKey = "setupComplete"
    /// The `settings` table's key holding the lane new tickets are filed into: the
    /// human's last choice, cached so a session of filing does not repeat it.
    private static let defaultLaneKey = "defaultLane"
    /// Whether the bikeshed's hidden layer —
    /// the capture saddlebag (`.raw/`) and the consolidation archive (`.raw/.done/`)
    /// — is revealed in the tree. Off by default: the curated layer is the
    /// reading surface, and the hidden layer is revealed only on request.
    private static let showHiddenFilesKey = "showHiddenFiles"
    /// The `settings` table's key holding the roster — the names the ticket
    /// sheet's assignee picker offers (ticket 4B7CD193). The SKILL.md roster
    /// (`skills/tandem-team/SKILL.md`, ADR 0029) stays the agents' source of
    /// truth; this is the human's editable copy for the app, and the two can
    /// drift — the human owns both.
    private static let rosterKey = "roster"
    /// The legacy settings file's name, under `TandemHome.directory`: the
    /// read-fallback's source and the one-time import's, never written again.
    private static let fileName = "settings.json"

    /// The human's chosen bikeshed, or `nil` when none was chosen (or the legacy
    /// file is missing, unreadable, or malformed). Reads the real
    /// `TandemHome.directory`; returns a standardized absolute URL.
    public static func chosenBikeshed() -> URL? {
        chosenBikeshed(in: TandemHome.directory)
    }

    /// Persist `url` as the chosen bikeshed, writing the store's `settings` table
    /// under the real `TandemHome.directory`.
    public static func setChosenBikeshed(_ url: URL) throws {
        try setChosenBikeshed(url, in: TandemHome.directory)
    }

    /// Whether the first-run setup has been finished or skipped, read from the
    /// real `TandemHome.directory`. `false` when the flag was never written —
    /// including an absent row, or a missing, unreadable, or malformed legacy file
    /// (the same leniency as `chosenBikeshed()`: a broken file re-shows the wizard
    /// rather than crashing the app).
    public static func setupComplete() -> Bool {
        setupComplete(in: TandemHome.directory)
    }

    /// Record that first-run setup is done, writing the store's `settings` table
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
    /// the store's `settings` table under the real `TandemHome.directory`.
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

    /// Record whether the hidden layer is revealed, writing the store's `settings`
    /// table under the real `TandemHome.directory`.
    public static func setShowHiddenFiles(_ shown: Bool) throws {
        try setShowHiddenFiles(shown, in: TandemHome.directory)
    }

    /// The roster the assignee picker offers when no roster was ever stored:
    /// the six roles of `skills/tandem-team/SKILL.md` (ADR 0029) at the time the
    /// setting shipped. A stored roster replaces this whole list — it is the
    /// human's list to edit, not a set to merge into.
    public static let defaultRoster: [String] = [
        "human", "builder", "refiner", "reviewer", "consolidator", "sprint",
    ]

    /// The roster — the names the ticket sheet's assignee picker offers. Read
    /// from the real `TandemHome.directory`; an absent (or malformed) key reads
    /// as `defaultRoster`, because a list that was never edited should not
    /// shrink the picker. An explicitly stored empty list reads as empty: the
    /// human removed every name, and the picker's own constants (`human`,
    /// Unassigned) keep it useful anyway.
    public static func roster() -> [String] {
        roster(in: TandemHome.directory)
    }

    /// Persist the roster, writing the store's `settings` table under the real
    /// `TandemHome.directory`. Names are trimmed and blank ones dropped — the
    /// same edge-trim rule every other setting's write applies.
    public static func setRoster(_ names: [String]) throws {
        try setRoster(names, in: TandemHome.directory)
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
        try mutate(in: home) { settings in
            let trimmed = lane?.trimmingCharacters(in: .whitespacesAndNewlines)
            if let trimmed, !trimmed.isEmpty {
                settings[defaultLaneKey] = trimmed
            } else {
                settings.removeValue(forKey: defaultLaneKey)
            }
        }
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
    /// and the setup flag are preserved).
    static func setShowHiddenFiles(_ shown: Bool, in home: URL) throws {
        try mutate(in: home) { settings in
            settings[showHiddenFilesKey] = shown
        }
    }

    /// The testable seam: `setChosenBikeshed(_:)` against an explicit home
    /// directory. Merges the key onto the existing record (keeping any other
    /// settings).
    static func setChosenBikeshed(_ url: URL, in home: URL) throws {
        try mutate(in: home) { settings in
            settings[bikeshedKey] = url.standardizedFileURL.path
        }
    }

    /// The testable seam: `roster()` against an explicit home directory.
    static func roster(in home: URL) -> [String] {
        guard let raw = object(in: home)[rosterKey] as? [Any] else { return defaultRoster }
        return raw.compactMap { $0 as? String }
    }

    /// The testable seam: `setRoster(_:)` against an explicit home directory.
    /// Merges the key onto the existing record (the bikeshed choice, the setup
    /// flag, and the rest are preserved). An empty list is stored as one: absent
    /// means "never edited", empty means "edited to nothing".
    static func setRoster(_ names: [String], in home: URL) throws {
        try mutate(in: home) { settings in
            let trimmed = names.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            settings[rosterKey] = trimmed.filter { !$0.isEmpty }
        }
    }

    /// The testable seam: `setSetupComplete(_:)` against an explicit home
    /// directory. Merges the key onto the existing record (a bikeshed choice is
    /// preserved).
    static func setSetupComplete(_ done: Bool, in home: URL) throws {
        try mutate(in: home) { settings in
            settings[setupKey] = done
        }
    }

    // MARK: - Storage (the database, with the JSON read-fallback)

    /// Settings as a dictionary: the database once the settings domain has been
    /// imported, else the legacy `settings.json`. A read never creates, imports
    /// or throws — a missing, unreadable or malformed source is simply `[:]`,
    /// because a broken settings store must never take the app down.
    ///
    /// Once the database exists, a read that fails is `[:]` ("no choice"), never
    /// the legacy file: after the import the file is evidence, and serving it
    /// would silently put back the values the human had at import time. The file
    /// is the fallback only while the domain's marker is absent.
    private static func object(in home: URL) -> [String: Any] {
        let database = home.appendingPathComponent("tandem.db")
        guard FileManager.default.fileExists(atPath: database.path) else {
            return fileObject(in: home)
        }
        do {
            if let stored = try StoreMigration.sidecarRead(
                in: home, domain: StoreSchema.settingsDomain,
                { db in try readRows(db) })
            {
                return stored
            }
            // No import marker yet: the legacy file is still the record.
            return fileObject(in: home)
        } catch {
            // The database is there but cannot answer. The marker cannot be read
            // from a broken database, so the only honest non-throwing answer is
            // "no choice" — not the pre-import file, which would be a silent lie
            // about the human's current settings.
            return [:]
        }
    }

    /// A read-modify-write in one transaction: the first write imports
    /// `settings.json` (rows, the copy-aside and the marker commit together),
    /// then the body's key change and the whole row set land together. Every
    /// setter is one setting, not a schema bump — the old docstring's reason for
    /// a separate file died with the database; a new setting is still one key.
    private static func mutate<T>(
        in home: URL, _ body: (inout [String: Any]) throws -> T
    ) throws -> T {
        try Store.translate {
            try StoreMigration.sidecarWrite(
                in: home, domain: StoreSchema.settingsDomain,
                importDomain: { db in try importSettings(in: home, into: db) },
                body: { db in
                    var settings = try readRows(db)
                    let value = try body(&settings)
                    return (value, true, nil, { try writeRows(settings, to: db) })
                }).value
        }
    }

    /// The one-time import: `settings.json` read once, its keys inserted, the
    /// file copied aside as `<name>.pre-sqlite-<ts>` and left where it was.
    private static func importSettings(
        in home: URL, into db: SQLiteDatabase
    ) throws -> (source: String?, note: String?) {
        let file = home.appendingPathComponent(fileName)
        guard FileManager.default.fileExists(atPath: file.path) else { return (nil, nil) }
        let settings = fileObject(in: home)
        try StoreMigration.copyAside(file)
        try writeRows(settings, to: db)
        return (fileName, nil)
    }

    private static func readRows(_ db: SQLiteDatabase) throws -> [String: Any] {
        var object: [String: Any] = [:]
        let rows = try db.rows("SELECT key, value FROM settings ORDER BY key") {
            ($0.text(0), $0.text(1))
        }
        for (key, raw) in rows {
            guard let data = raw.data(using: .utf8),
                  let value = try? JSONSerialization.jsonObject(
                    with: data, options: [.fragmentsAllowed])
            else { continue }
            object[key] = value
        }
        return object
    }

    private static func writeRows(
        _ settings: [String: Any], to db: SQLiteDatabase
    ) throws {
        try db.run("DELETE FROM settings")
        for (key, value) in settings.sorted(by: { $0.key < $1.key }) {
            let data = try JSONSerialization.data(
                withJSONObject: value, options: [.fragmentsAllowed, .sortedKeys])
            try db.run(
                "INSERT INTO settings (key, value) VALUES (?, ?)",
                [.text(key), .text(String(decoding: data, as: UTF8.self))])
        }
    }

    /// The legacy file parsed as a JSON object, or `[:]` when it is missing,
    /// unreadable, or malformed — the read leniency, shared by every getter and
    /// by the one-time import.
    private static func fileObject(in home: URL) -> [String: Any] {
        let file = home.appendingPathComponent(fileName)
        guard let data = try? Data(contentsOf: file),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any]
        else { return [:] }
        return object
    }
}