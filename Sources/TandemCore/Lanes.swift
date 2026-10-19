import Foundation

/// The store's contents: the ordered lane names and the todos, in stored order.
///
/// **Stored order is the order.** The `todos` array position is authoritative (new todos
/// land on top by default); it is never recomputed from `createdAt`. It is also kept
/// **grouped by lane** — first the default (ungrouped) area, then each lane in the
/// declared order — so the GUI can walk it once and emit a header whenever the lane
/// changes, and hand-edited files stay legible.
struct StoreContents: Sendable {
    /// Declared lane names, in render order after the default area.
    var lanes: [String]
    /// Todos in stored order (grouped by lane).
    var todos: [Todo]

    /// A lane key: `nil` is the default (ungrouped) area, always first.
    typealias Key = String?

    /// Every key in render order: the default first, then each declared lane, then any
    /// name that appears on a todo but is not declared (a hand edit), in first-appearance
    /// order — so a hand-edited file loses no grouping.
    var orderedKeys: [Key] {
        var keys: [Key] = [nil]
        for name in lanes where !keys.contains(name) { keys.append(name) }
        for todo in todos where todo.lane != nil {
            if !keys.contains(todo.lane) { keys.append(todo.lane) }
        }
        return keys
    }

    /// Every name that is safe to file into: each declared lane plus any dangling name
    /// already on a todo.
    var knownLanes: [String] { orderedKeys.compactMap { $0 } }

    /// The index at which `key`'s block begins — the cumulative length of every preceding
    /// group. For a key that is neither declared nor present, that is the end of the list.
    func blockStart(of key: Key) -> Int {
        var start = 0
        for k in orderedKeys {
            if k == key { return start }
            start += todos.lazy.filter { $0.lane == k }.count
        }
        return start
    }

    /// Re-establishes the grouping invariant, stably: groups in `orderedKeys` order, each
    /// keeping its current relative order. Run before every read-modify-write body and
    /// again before the write, so the invariant holds for every operation.
    mutating func normalize() {
        let keys = orderedKeys
        todos = keys.flatMap { key in todos.filter { $0.lane == key } }
    }

    /// Guards a lane argument from the GUI: `nil` (the default area) is always valid;
    /// any other name must be a known lane.
    func validateLane(_ lane: String?) throws {
        guard let lane else { return }
        guard knownLanes.contains(lane) else {
            throw DomainError.unknownLane(lane)
        }
    }
}
