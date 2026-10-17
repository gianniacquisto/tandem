import Foundation

/// The store's contents: the ordered section names and the todos, in stored order.
///
/// **Stored order is the order.** The `todos` array position is authoritative (new todos
/// land on top by default); it is never recomputed from `createdAt`. It is also kept
/// **grouped by section** — first the default (ungrouped) area, then each section in the
/// declared order — so the GUI can walk it once and emit a header whenever the section
/// changes, and hand-edited files stay legible.
struct StoreContents: Sendable {
    /// Declared section names, in render order after the default area.
    var sections: [String]
    /// Todos in stored order (grouped by section).
    var todos: [Todo]

    /// A section key: `nil` is the default (ungrouped) area, always first.
    typealias Key = String?

    /// Every key in render order: the default first, then each declared section, then any
    /// name that appears on a todo but is not declared (a hand edit), in first-appearance
    /// order — so a hand-edited file loses no grouping.
    var orderedKeys: [Key] {
        var keys: [Key] = [nil]
        for name in sections where !keys.contains(name) { keys.append(name) }
        for todo in todos where todo.section != nil {
            if !keys.contains(todo.section) { keys.append(todo.section) }
        }
        return keys
    }

    /// Every name that is safe to file into: each declared section plus any dangling name
    /// already on a todo.
    var knownSections: [String] { orderedKeys.compactMap { $0 } }

    /// The index at which `key`'s block begins — the cumulative length of every preceding
    /// group. For a key that is neither declared nor present, that is the end of the list.
    func blockStart(of key: Key) -> Int {
        var start = 0
        for k in orderedKeys {
            if k == key { return start }
            start += todos.lazy.filter { $0.section == k }.count
        }
        return start
    }

    /// Re-establishes the grouping invariant, stably: groups in `orderedKeys` order, each
    /// keeping its current relative order. Run before every read-modify-write body and
    /// again before the write, so the invariant holds for every operation.
    mutating func normalize() {
        let keys = orderedKeys
        todos = keys.flatMap { key in todos.filter { $0.section == key } }
    }

    /// Guards a section argument from the GUI: `nil` (the default area) is always valid;
    /// any other name must be a known section.
    func validateSection(_ section: String?) throws {
        guard let section else { return }
        guard knownSections.contains(section) else {
            throw DomainError.unknownSection(section)
        }
    }
}
