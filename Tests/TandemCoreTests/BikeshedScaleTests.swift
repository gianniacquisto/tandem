import Foundation
import Testing
@testable import TandemCore

/// Scale sentinel (spec: "Scale sentinel"): one generous, non-flaky bound —
/// a full bikeshed-wide recompute (every note read, scanned, and its wikilinks
/// resolved — the heaviest derived operation: `backlinks` over a hub note)
/// over a synthetic bikeshed of hundreds of notes and ~100 MB total completes
/// in seconds, keeping the sub-second feel honest. The bound is measured on
/// the reference hardware and set far above it (see the test's comment) so
/// it catches order-of-magnitude regressions, not scheduler noise.
@Suite("Bikeshed scale sentinel")
struct BikeshedScaleTests {

    private func tempDir() -> URL {
        /// Nested one level below a unique parent so the bikeshed's sibling
        /// lockfile (`bikeshed.lock`) is never shared between concurrent tests.
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-scale-\(UUID().uuidString)")
        let dir = parent.appendingPathComponent("bikeshed")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// ~0.34 MB of deterministic prose: a heading, bulk paragraphs, and a
    /// realistic link load (~200 wikilinks per note, to the hub and a few
    /// siblings) so the scan and resolver do real work on every note.
    private func syntheticNote(_ index: Int) -> String {
        var out = "# Note \(index)\n\n"
        out.reserveCapacity(360_000)
        for p in 0..<2500 {
            out += "Paragraph \(p) of note \(index): the quick brown fox jumps over the lazy dog. "
            out += "Bulk text to keep the note at a realistic size for the sentinel. "
            if p % 20 == 0 {
                out += "It links to [[Hub]] and to [[Note \(index % 50)]]. "
            }
            out += "\n\n"
        }
        return out
    }

    @Test("a full bikeshed-wide recompute over ~100 MB of hundreds of notes completes in seconds")
    func fullRecomputeStaysFast() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // 280 notes × ~0.34 MB ≈ 95 MB, in 14 folders, all linking a hub.
        let noteCount = 280
        let folders = 14
        var staged = 0
        for index in 0..<noteCount {
            let folder = "folder-\(index / (noteCount / folders))"
            let text = syntheticNote(index)
            let url = dir.appendingPathComponent("\(folder)/note-\(index).md")
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try text.write(to: url, atomically: false, encoding: .utf8)
            staged += text.utf8.count
        }
        try "The hub note.\n".write(
            to: dir.appendingPathComponent("Hub.md"), atomically: false, encoding: .utf8)

        let totalBytes = staged + 14
        // Every note links the hub; the [[Note …]] links are deliberately
        // broken (no note carries that name) and must contribute nothing.
        let expected = noteCount

        // The heaviest derived op: read, scan, and resolve every wikilink in
        // the whole bikeshed. Measured on Apple silicon (2026-10): ~11 s for
        // this bikeshed. The bound below carries ~4× headroom — generous and
        // non-flaky, yet it still catches order-of-magnitude regressions
        // (an unindexed per-link resolution walk measured 474 s here).
        let start = Date()
        let rows = try bikeshed.backlinks(path: "Hub.md").value
        let elapsed = Date().timeIntervalSince(start)

        FileHandle.standardError.write(
            "SCALE \(totalBytes / (1024 * 1024)) MB, \(noteCount) notes: \(String(format: "%.2f", elapsed)) s\n"
                .data(using: .utf8)!)
        #expect(totalBytes >= 90 * 1024 * 1024, "bikeshed smaller than intended: \(totalBytes) bytes")
        #expect(rows.count == expected, "expected \(expected) backlink rows, got \(rows.count)")
        #expect(elapsed < 45, "full recompute took \(elapsed) s — should stay in seconds")
    }
}
