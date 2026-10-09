import Foundation
import Observation

// PROTOTYPE — throwaway in-memory model. This is NOT the store (see ADR 0001);
// the prototype is judging layout and interaction, not persistence.

struct Todo: Identifiable, Equatable {
    let id: UUID
    var title: String
    var isDone: Bool
    var createdAt: Date
    var completedAt: Date?

    /// Last touch — used by the recency sort in Variant C.
    var updatedAt: Date { completedAt ?? createdAt }
}

extension Notification.Name {
    /// Posted on any model or icon change; the menu bar stub listens.
    static let tandemChanged = Notification.Name("tandemChanged")
}

@Observable
final class TandemModel {
    private(set) var todos: [Todo] = []

    /// Creation order — the store's order (ADR 0001).
    var openTodos: [Todo] { todos.filter { !$0.isDone } }
    var doneTodos: [Todo] { todos.filter { $0.isDone } }

    init() { Self.seed(into: self) }

    func add(_ rawTitle: String) {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        todos.append(Todo(id: UUID(), title: title, isDone: false, createdAt: .now, completedAt: nil))
        changed()
    }

    func toggle(_ id: UUID) {
        guard let i = todos.firstIndex(where: { $0.id == id }) else { return }
        todos[i].isDone.toggle()
        todos[i].completedAt = todos[i].isDone ? .now : nil
        changed()
    }

    func clearDone() {
        guard !doneTodos.isEmpty else { return }
        todos.removeAll { $0.isDone }
        changed()
    }

    private func changed() {
        NotificationCenter.default.post(name: .tandemChanged, object: nil)
    }

    /// 9 seeded todos: 7 open, 2 done — enough open ones to see the menu bar
    /// truncate at ~5.
    private static func seed(into model: TandemModel) {
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
            ("Write the spec draft", false, 1, nil),
        ]
        model.todos = seed.map {
            Todo(id: UUID(), title: $0.title, isDone: $0.done,
                 createdAt: h($0.created),
                 completedAt: $0.done ? h($0.completed!) : nil)
        }
    }
}
