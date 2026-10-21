import Foundation

/// One broken wikilink: the note that holds it, the target as written, and
/// the line it sits on. Derived state — recomputed from the file set, never
/// stored.
public struct BrokenLink: Sendable, Equatable, Hashable {
    /// The holding note's bikeshed-root-relative path.
    public let note: String
    /// The link target as written — no `#fragment`, no `|alias`: the part
    /// that resolves to nothing.
    public let link: String
    /// The 1-based line number of the link in its note.
    public let line: Int
}

/// The bikeshed's link-graph health: the wikilinks that resolve to nothing,
/// the notes nothing links to, and the totals. What `note links` reports and
/// what the app's Health panel lists — one core call, so the two surfaces
/// cannot drift.
public struct BikeshedHealth: Sendable, Equatable {
    /// The empty report — a bikeshed with nothing in it, and the state the
    /// app's Health panel holds until its first scan lands.
    public static let empty = BikeshedHealth(
        broken: [], orphans: [],
        counts: BikeshedHealthCounts(notes: 0, links: 0, broken: 0, orphans: 0))

    /// Every wikilink that resolves to no note (or to more than one — the
    /// exactly-one rule makes an ambiguous basename a visible broken link,
    /// ADR 0005), sorted by note, then line, then target.
    public let broken: [BrokenLink]
    /// The notes with no incoming wikilink, minus the entry points (the Hub
    /// and a project's anchor note), sorted by path.
    public let orphans: [String]
    /// The totals.
    public let counts: BikeshedHealthCounts
}

/// The totals of a `BikeshedHealth` readout.
public struct BikeshedHealthCounts: Sendable, Equatable {
    /// The notes scanned (the whole bikeshed, the hidden saddlebag pruned).
    public let notes: Int
    /// The wikilinks found in them, resolved and broken together.
    public let links: Int
    /// The broken ones.
    public let broken: Int
    /// The orphan notes.
    public let orphans: Int
}

extension Bikeshed {

    /// `health()`: the bikeshed's link-graph health — the wikilinks that
    /// resolve to nothing, the notes nothing links to, and the totals.
    /// Read-only and lock-free: the file set is read once and every link
    /// resolved against it, with the same resolver the backlinks and the
    /// rename-rewrite use.
    ///
    /// The rules, in full:
    /// - **Broken** — a wikilink occurrence whose target resolves to no note,
    ///   or to more than one (an ambiguous basename is a visible broken link,
    ///   never a silent tie-break — ADR 0005). The scanner's own exemptions
    ///   apply: code spans, embeds, and unclosed tokens are not links.
    /// - **Orphans** — the notes with no *incoming* wikilink, minus the
    ///   entry points: the Hub (`Index.md` at the bikeshed root) and a
    ///   project's anchor note (`projects/<project>/<project>.md` — the
    ///   project's orientation surface, the note a project starts as). A note
    ///   that links out but is linked to by nothing is still an orphan.
    /// - The hidden `.raw/` saddlebag is out of the graph (ADR 0004: the
    ///   linkable set prunes the hidden layer, whatever the reveal flag says
    ///   — ADR 0023), so a capture's unresolvable links are never reported,
    ///   and a capture is never an orphan.
    public func health() throws -> OperationResult<BikeshedHealth> {
        let notes = notePaths()
        var broken: [BrokenLink] = []
        var inbound: Set<String> = []
        var linkCount = 0
        for path in notes {
            guard let content = try? readNote(path) else { continue }
            let occurrences = Self.linkOccurrences(in: content.text, against: notes)
            linkCount += occurrences.count
            for occurrence in occurrences {
                switch occurrence.resolution {
                case .broken:
                    broken.append(BrokenLink(
                        note: path,
                        link: occurrence.link.target,
                        line: Self.lineNumber(of: occurrence.range.lowerBound, in: content.text)))
                case .resolved(let target):
                    inbound.insert(target)
                }
            }
        }
        broken.sort {
            ($0.note.lowercased(), $0.note, $0.line, $0.link)
                < ($1.note.lowercased(), $1.note, $1.line, $1.link)
        }
        let orphans = notes
            .filter { !inbound.contains($0) && !Self.isAnchorNote($0) }
            .sorted { ($0.lowercased(), $0) < ($1.lowercased(), $1) }
        return OperationResult(BikeshedHealth(
            broken: broken,
            orphans: orphans,
            counts: BikeshedHealthCounts(
                notes: notes.count,
                links: linkCount,
                broken: broken.count,
                orphans: orphans.count)))
    }

    // MARK: - Rules (internal)

    /// The entry points that are never orphans: the Hub (`Index.md` at the
    /// bikeshed root) and a project's anchor note — the note named after its
    /// project folder (`projects/<project>/<project>.md`), the orientation
    /// surface a project starts as. A project with no such note has no
    /// anchor, and its notes are all orphan candidates.
    static func isAnchorNote(_ path: String) -> Bool {
        if path.lowercased() == "index.md" { return true }
        let components = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard components.count == 3, components[0].lowercased() == "projects" else { return false }
        let project = components[1].lowercased()
        var name = components[2].lowercased()
        if name.hasSuffix(".md") { name.removeLast(3) }
        return name == project
    }

    /// The 1-based line number of a position in `text` — the count of the
    /// line starts at or before it.
    static func lineNumber(of index: String.Index, in text: String) -> Int {
        let starts = lineStartByteOffsets(of: text)
        let offset = text.utf8.distance(from: text.utf8.startIndex, to: index)
        var low = 0
        var high = starts.count
        while low < high {
            let mid = (low + high) / 2
            if starts[mid] <= offset { low = mid + 1 } else { high = mid }
        }
        return low
    }
}
