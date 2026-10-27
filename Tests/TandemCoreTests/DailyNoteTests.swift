import Foundation
import Testing
@testable import TandemCore

/// Daily notes (ticket EE991EF2): a human-owned `daily/` folder at the vault
/// root the consolidation pass harvests and never edits.
///
/// The contract has two halves. The **folder** is the app's — one file per day,
/// `daily/YYYY-MM-DD.md`, open-or-create, an ordinary note the human edits. The
/// **fold position** is the pass's — a dated marker line in `Index.md`, with the
/// pending set being the files dated after it, found by name and never by mtime.
@Suite("Daily notes")
struct DailyNoteTests {

    /// A unique temp directory (not pre-created: a missing bikeshed folder is a
    /// valid empty bikeshed). Nested one level below a unique parent so the
    /// bikeshed's sibling lockfile is never shared between concurrent tests.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-daily-notes-\(UUID().uuidString)")
            .appendingPathComponent("bikeshed")
    }

    private func write(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    /// A UTC calendar, so the day a note belongs to never depends on the Mac's zone.
    private var utc: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }

    private func day(_ year: Int, _ month: Int, _ day: Int) -> Date {
        utc.date(from: DateComponents(year: year, month: month, day: day, hour: 12))!
    }

    // MARK: - The folder and the app's open-or-create

    @Test("a day's note is daily/YYYY-MM-DD.md at the vault root, zero-padded")
    func pathIsTheDay() {
        #expect(DailyNote.harvestFolders == ["daily"],
                "the harvest list is the one human-owned folder today")
        #expect(DailyNote.path(on: day(2026, 10, 26), calendar: utc) == "daily/2026-10-26.md")
        #expect(DailyNote.path(on: day(2026, 1, 5), calendar: utc) == "daily/2026-01-05.md")
    }

    @Test("opening a day's note creates it once, and a second open leaves the human's bytes alone")
    func openOrCreate() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let today = day(2026, 10, 26)

        let path = try bikeshed.openDailyNote(on: today, calendar: utc).value
        #expect(path == "daily/2026-10-26.md")
        let url = dir.appendingPathComponent(path)
        #expect(FileManager.default.fileExists(atPath: url.path),
                "the action creates today's file when it is missing")
        // The tree draws the folder like any other visible folder, with the file in it.
        let daily = try bikeshed.list().value.first { $0.path == "daily" }
        #expect(daily?.kind == .folder)
        #expect(daily?.children.map(\.path) == ["daily/2026-10-26.md"])

        // The human writes. Opening again is not a write.
        try "# Today\n\nwhat the day threw at me\n".write(to: url, atomically: false, encoding: .utf8)
        let again = try bikeshed.openDailyNote(on: today, calendar: utc).value
        #expect(again == path)
        #expect(try String(contentsOf: url, encoding: .utf8)
                == "# Today\n\nwhat the day threw at me\n",
                "a daily note is the human's: opening it never overwrites it")
    }

    // MARK: - The fold marker

    @Test("the fold marker names the day the pass advanced through")
    func markerParses() {
        #expect(DailyNote.foldedThrough(in: "# Index\n\nHuman notes folded through 2026-10-26\n")
                == "2026-10-26")
        #expect(DailyNote.foldedThrough(in: "\n- Human notes folded through 2026-01-05\n") == "2026-01-05")
        // No marker yet, and a line that names no day, fold nothing.
        #expect(DailyNote.foldedThrough(in: "# Index\n") == nil)
        #expect(DailyNote.foldedThrough(in: "Human notes folded through soon") == nil)
    }

    // MARK: - Discovery: the list plus the marker date, never mtime

    @Test("the pending set is the daily notes dated after the marker")
    func pendingIsAfterTheMarker() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        for day in ["2026-10-24", "2026-10-25", "2026-10-26", "2026-10-27"] {
            try write("daily/\(day).md", "# \(day)\n", in: dir)
        }
        try write("Index.md", "# Index\n\nHuman notes folded through 2026-10-25\n", in: dir)
        let bikeshed = Bikeshed(directory: dir)

        let pending = try bikeshed.pendingDailyNotes().value
        #expect(pending == ["daily/2026-10-26.md", "daily/2026-10-27.md"],
                "the day the marker names is folded; the days after it are not")
    }

    @Test("without a marker every daily note is pending, and nothing else is")
    func pendingWithoutAMarker() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try write("daily/2026-10-24.md", "# a\n", in: dir)
        try write("daily/2026-10-25.md", "# b\n", in: dir)
        // Not daily notes: an attachment, a nested note, an undated file.
        try write("daily/2026-10-24.png", "png", in: dir)
        try write("daily/notes/2026-10-23.md", "# nested\n", in: dir)
        try write("daily/loose.md", "# undated\n", in: dir)
        // A folder outside the harvest list is not read at all.
        try write("other/2026-10-26.md", "# elsewhere\n", in: dir)
        let bikeshed = Bikeshed(directory: dir)

        let pending = try bikeshed.pendingDailyNotes().value
        #expect(pending == ["daily/2026-10-24.md", "daily/2026-10-25.md"])
    }

    @Test("a note dated before the marker stays folded when its mtime moves")
    func discoveryIgnoresMtime() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try write("daily/2026-10-20.md", "# old\n", in: dir)
        try write("daily/2026-10-27.md", "# new\n", in: dir)
        try write("Index.md", "Human notes folded through 2026-10-21\n", in: dir)
        // A touch (or the human editing an old note) makes the old file the newest
        // on disk. Discovery must still exclude it: the date is in the name.
        try FileManager.default.setAttributes(
            [.modificationDate: Date()],
            ofItemAtPath: dir.appendingPathComponent("daily/2026-10-20.md").path)
        let bikeshed = Bikeshed(directory: dir)

        #expect(try bikeshed.pendingDailyNotes().value == ["daily/2026-10-27.md"],
                "an mtime never folds a note the marker already covers")
    }

    @Test("reading a pass's inputs writes nothing, and one advance leaves nothing pending")
    func onePassThenNothing() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        try write("daily/2026-10-26.md", "# Today\n\nthe forum breaks on Safari\n", in: dir)
        try write("Index.md", "# Index\n\nHuman notes folded through 2026-10-25\n", in: dir)
        let bikeshed = Bikeshed(directory: dir)
        let daily = dir.appendingPathComponent("daily/2026-10-26.md")
        let before = try Data(contentsOf: daily)
        let indexBefore = try String(contentsOf: dir.appendingPathComponent("Index.md"), encoding: .utf8)

        // The deterministic half of a pass: read the fold position, read the pending
        // notes, fold them, advance the marker. The judgment (which curated note holds
        // the fold) is the consolidator's; the rule it runs is this.
        let marker = DailyNote.foldedThrough(in: indexBefore) ?? ""
        let pending = try bikeshed.pendingDailyNotes().value
        #expect(marker == "2026-10-25")
        #expect(pending == ["daily/2026-10-26.md"])
        let folded = try String(contentsOf: daily, encoding: .utf8)
        try write("projects/tandem/tandem.md", "# Tandem\n\nwhat the day held: \(folded)", in: dir)
        try write(
            "Index.md",
            indexBefore.replacingOccurrences(
                of: "Human notes folded through 2026-10-25",
                with: "Human notes folded through 2026-10-26"),
            in: dir)

        // The daily file is the human's: byte-identical after the fold.
        #expect(try Data(contentsOf: daily) == before)
        // One advance folds exactly once; the same fixture now has nothing pending.
        #expect(try bikeshed.pendingDailyNotes().value == [])
        let indexAfter = try String(contentsOf: dir.appendingPathComponent("Index.md"), encoding: .utf8)
        #expect(indexAfter.components(separatedBy: "Human notes folded through 2026-10-26").count == 2,
                "the marker names one day, once")
    }
}
