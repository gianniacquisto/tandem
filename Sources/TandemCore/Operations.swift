import Foundation

/// Contract-level todo operations (spec: "Operations (core)"). These are the only
/// public surface the GUI and the MCP server use; they define the shared semantics so
/// both faces behave identically. The MCP server adds only encoding; the GUI adds only
/// presentation.
extension Store {

    // MARK: - list

    /// `list(filter)`: todos in creation order, filtered. `filter` defaults to `.open`.
    /// An empty result is a successful empty list. Display sorting is a UI concern and
    /// never happens here.
    public func list(_ filter: Filter = .open) throws -> OperationResult<[Todo]> {
        let (todos, note) = try readAll()
        return OperationResult(filter.apply(to: todos), note: note)
    }

    /// `list` with the raw wire filter string (the MCP boundary). `nil` (argument absent)
    /// means the default `.open`; any unrecognized value is `DomainError.invalidFilter`.
    public func list(rawFilter raw: String?) throws -> OperationResult<[Todo]> {
        guard let filter = Filter.parse(raw: raw) else {
            throw DomainError.invalidFilter(raw ?? "")
        }
        return try list(filter)
    }

    // MARK: - add

    /// `add(title)`: the title is trimmed; empty/whitespace-only is rejected with an
    /// actionable error. Duplicate titles are allowed. Returns the created todo (fresh
    /// id, `completedAt` null).
    public func add(_ rawTitle: String) throws -> OperationResult<Todo> {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { throw DomainError.emptyTitle }
        let (todo, note) = try mutate { list -> (value: Todo, changed: Bool) in
            let todo = Todo(title: title)
            list.append(todo)
            return (todo, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - complete

    /// `complete(id)`: idempotent — an already-done todo is returned unchanged, no error.
    /// On a transition to done, `completedAt` is set to the current UTC time. An unknown
    /// id throws `DomainError.unknownID` carrying the full current open list. The inverse
    /// is `reopen` (GUI-only).
    public func complete(_ id: String) throws -> OperationResult<Todo> {
        let (todo, note) = try mutate { list -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = list.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: list.filter { !$0.done })
            }
            var changed = false
            if !list[i].done {
                list[i].done = true
                list[i].completedAt = .now
                changed = true
            }
            return (list[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - reopen

    /// `reopen(id)`: the inverse of `complete` — GUI-only, with no MCP tool (the agent
    /// surface stays `list`/`add`/`complete`). Idempotent — an already-open todo is
    /// returned unchanged, no error. On a transition back to open, `completedAt` is
    /// cleared. An unknown id throws `DomainError.unknownID` carrying the full current
    /// open list.
    public func reopen(_ id: String) throws -> OperationResult<Todo> {
        let (todo, note) = try mutate { list -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = list.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: list.filter { !$0.done })
            }
            var changed = false
            if list[i].done {
                list[i].done = false
                list[i].completedAt = nil
                changed = true
            }
            return (list[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - clearDone

    /// `clearDone()`: removes all done todos in one write. Returns the number removed.
    /// A no-op (no write) when there are no done todos.
    public func clearDone() throws -> OperationResult<Int> {
        let (count, note) = try mutate { list -> (value: Int, changed: Bool) in
            let before = list.count
            list.removeAll { $0.done }
            let removed = before - list.count
            return (removed, removed > 0)
        }
        return OperationResult(count, note: note)
    }

    // MARK: - delete

    /// `delete(id)`: removes a single todo. GUI-only — there is no delete tool in the MCP
    /// v1 surface. An unknown id throws `DomainError.unknownID` (with the open list).
    public func delete(_ id: String) throws -> OperationResult<Void> {
        let (_, note) = try mutate { list -> (value: Void, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = list.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: list.filter { !$0.done })
            }
            list.remove(at: i)
            return ((), true)
        }
        return OperationResult((), note: note)
    }
}
