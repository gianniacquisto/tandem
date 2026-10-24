import Foundation

/// The ticket sheet's assignee picker rule (ticket 4B7CD193), in the core
/// because the app face has no test harness — the same reason the ⌘K ranking
/// lives here. The rule is pure over (roster, current assignee):
///
/// - **Unassigned** is always offered (the clear), and **`human`** is always
///   offered: the two are constants of the picker, not roster-derived, so no
///   Settings edit can drop them.
/// - The roster's names follow, in the order the human keeps them.
/// - The ticket's current assignee, when it is not already listed, is appended:
///   an existing value is never an un-reselectable stranger.
///
/// An option is `String?` — `nil` is Unassigned, the same shape the store's
/// `assignee` field holds, so a choice writes through `set(id, assignee:)`
/// unchanged and Unassigned is the clear.
public enum AssigneePicker {

    /// The picker's options, in display order: Unassigned, `human`, the roster
    /// (trimmed, blanks dropped, duplicates collapsed), then an off-roster
    /// current assignee.
    public static func options(roster: [String], current: String?) -> [String?] {
        var seen = Set<String>()
        var options: [String?] = [nil]
        func add(_ raw: String?) {
            guard let raw else { return }
            let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty, seen.insert(name).inserted else { return }
            options.append(name)
        }
        add(TicketQuery.human)
        roster.forEach(add)
        add(current)
        return options
    }
}