import Foundation
import Observation

// PROTOTYPE — throwaway in-memory model. This is NOT the store (see ADR 0001);
// the prototype is judging look, not persistence.
//
// v2 settled decision honoured here: the checkbox is ONE-WAY — completing
// works, un-checking does nothing. "Reset demo data" in the floating bar
// restores the seed so the judge can play again.

struct Todo: Identifiable, Equatable {
    let id: UUID
    var title: String
    var isDone: Bool
    var createdAt: Date
    var completedAt: Date?
}

@Observable
final class TandemModel {
    private(set) var todos: [Todo] = []

    /// Creation order — the store's order (ADR 0001).
    var openTodos: [Todo] { todos.filter { !$0.isDone } }
    var doneTodos: [Todo] { todos.filter { $0.isDone } }

    init() { seed() }

    func add(_ rawTitle: String) {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        todos.append(Todo(id: UUID(), title: title, isDone: false, createdAt: .now, completedAt: nil))
    }

    /// One-way (map decision 2026-10-11): no reopen in v2.
    func complete(_ id: UUID) {
        guard let i = todos.firstIndex(where: { $0.id == id }), !todos[i].isDone else { return }
        todos[i].isDone = true
        todos[i].completedAt = .now
    }

    func clearDone() {
        todos.removeAll { $0.isDone }
    }

    /// GUI-only (agents have no delete — v1 contract carries over).
    func delete(_ id: UUID) {
        todos.removeAll { $0.id == id }
    }

    func reset() { seed() }

    /// 8 seeded todos: 6 open (so the popover truncates at 5), 2 done.
    private func seed() {
        let now = Date()
        func h(_ hoursAgo: Int) -> Date { now.addingTimeInterval(TimeInterval(hoursAgo) * -3600) }
        let seed: [(title: String, done: Bool, created: Int, completed: Int?)] = [
            ("Ship the wayfinder map", true, 30, 1),
            ("Fix the CI flake", true, 26, 3),
            ("Book a dentist", false, 24, nil),
            ("Reply to Sam about the offsite", false, 22, nil),
            ("Renew passport", false, 20, nil),
            ("Water the plants", false, 9, nil),
            ("Draft Q4 goals", false, 6, nil),
            ("Order a new keyboard", false, 3, nil),
        ]
        todos = seed.map {
            Todo(id: UUID(), title: $0.title, isDone: $0.done,
                 createdAt: h($0.created),
                 completedAt: $0.done ? h($0.completed!) : nil)
        }
    }
}
