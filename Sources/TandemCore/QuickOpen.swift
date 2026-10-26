import Foundation

/// The quick-open palette (⌘K): **one ranked list over the bikeshed's notes and
/// the store's tickets** (ticket 3E1F4D23).
///
/// The ranking lives in the core, not the view, for two reasons: the core is the only
/// test seam (a ranked list is a pure function, so it needs no window, no store and no
/// bikeshed to be proved), and a second face — a CLI `open`, a menu-bar palette — must
/// reach the same answer, not a lookalike.
///
/// The rules, in order:
///
/// 1. **Archived tickets are out.** An archived ticket is set aside (GLOSSARY:
///    Archived); the palette reaches live work, not the archive.
/// 2. **The match is a substring**, case- and diacritic-insensitive — the store's own
///    comparison (`TextMatch`, the same one `TicketQuery` uses), not a second one. A
///    ticket is matched on its title, a note on its path.
/// 3. **Open before done.** A done ticket sorts below every open ticket and every
///    note, but it stays reachable: a finished ticket is still a thing to read.
/// 4. **A name hit before a path hit.** The match landed on the item's *name* (a
///    note's name, a ticket's title) rather than somewhere else in its path — so
///    "app" ranks `projects/App.md` with the other "App"s, not with the hits on
///    "projects".
/// 5. **Alphabetically**, case-insensitively, as the last word, so the same query
///    always gives the same list.
///
/// An empty query matches everything, the way an empty filter box is no filter in
/// `TicketQuery` — the palette then lists everything it can open, ranked by the same
/// rules.
public enum QuickOpen {

    /// One candidate in the palette: a note in the bikeshed, or a ticket in the store.
    ///
    /// The item carries the thing itself, never a copy of it: the palette opens what it
    /// shows, so the ticket the sheet opens is the ticket the list holds.
    public enum Item: Sendable, Equatable, Identifiable {

        /// A bikeshed note, by its bikeshed-root-relative path.
        case note(path: String)

        /// A ticket from the store.
        case ticket(Ticket)

        /// A stable identity for the row: a note is its path, a ticket its id.
        public var id: String {
            switch self {
            case .note(let path): return "note:\(path)"
            case .ticket(let todo): return "ticket:\(todo.id.uuidString)"
            }
        }

        /// The name the row shows: a note's name (its file name, without `.md`) or a
        /// ticket's title.
        public var name: String {
            switch self {
            case .note(let path): return BikeshedPath.name(of: path)
            case .ticket(let todo): return todo.title
            }
        }

        /// The text the query is matched against: a note's path, a ticket's title and
        /// ref (ticket 6598E091 — a typed `TAN-42` is how a human names a ticket).
        public var searchText: String {
            switch self {
            case .note(let path): return path
            case .ticket(let todo): return [todo.ref, todo.title].compactMap { $0 }.joined(separator: " ")
            }
        }

        /// Whether this ticket is done. A note is never done.
        public var isDone: Bool {
            if case .ticket(let todo) = self { return todo.done }
            return false
        }

        /// Whether this ticket is archived. A note is never archived.
        public var isArchived: Bool {
            if case .ticket(let todo) = self { return todo.archived }
            return false
        }
    }

    /// The candidates that match `query`, in rank order.
    ///
    /// - Parameters:
    ///   - items: the candidates — the bikeshed's notes and the store's tickets.
    ///   - query: what the human typed. Whitespace is trimmed; an empty query matches
    ///     everything.
    public static func rank(_ items: [Item], query: String) -> [Item] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let candidates = items.enumerated().compactMap { offset, item -> Candidate? in
            guard !item.isArchived else { return nil }
            guard needle.isEmpty || TextMatch.contains(needle, in: item.searchText) else { return nil }
            return Candidate(offset: offset, item: item, key: rankKey(for: item, needle: needle))
        }
        return candidates
            .sorted { lhs, rhs in
                if lhs.key.done != rhs.key.done { return lhs.key.done < rhs.key.done }
                if lhs.key.hit != rhs.key.hit { return lhs.key.hit < rhs.key.hit }
                if lhs.key.name != rhs.key.name { return lhs.key.name < rhs.key.name }
                // The same name twice (two notes with the same basename in different
                // folders): the input order decides, so the list is stable.
                return lhs.offset < rhs.offset
            }
            .map(\.item)
    }

    // MARK: - The rank key

    /// A candidate that matched, with the words that order it.
    private struct Candidate {
        /// Where the item came in — the last word in a tie.
        var offset: Int
        var item: Item
        var key: RankKey
    }

    /// The three words that order a match: done last, a name hit before a path hit, and
    /// the name itself.
    private struct RankKey {
        /// 0 for an open item, 1 for a done one.
        var done: Int
        /// 0 when the match landed on the item's name, 1 when it landed elsewhere in
        /// the searched text.
        var hit: Int
        /// The item's name, lowercased — the last word in the order.
        var name: String
    }

    private static func rankKey(for item: Item, needle: String) -> RankKey {
        RankKey(
            done: item.isDone ? 1 : 0,
            // An empty query hits every name, so there is no name/path distinction
            // to make.
            hit: needle.isEmpty ? 0 : (TextMatch.contains(needle, in: item.name) ? 0 : 1),
            name: item.name.lowercased())
    }
}
