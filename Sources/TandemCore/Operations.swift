import Foundation

/// Contract-level todo operations (spec: "Operations (core)"). These are the only
/// public surface the GUI and the CLI use; they define the shared semantics so
/// both faces behave identically. The CLI adds only encoding; the GUI adds only
/// presentation.
///
/// Ordering: the **stored order** is authoritative — the position of a todo in `todos.json`
/// is its position in the list, and `add` puts a new todo on **top** by default. Order is
/// never recomputed from `createdAt`. Sections group the list; the array is kept grouped by
/// section (default first, then each declared section) so a reader can walk it straight
/// through (see `StoreContents`).
extension Store {

    // MARK: - list

    /// `list(filter)`: todos in **stored order**, filtered. `filter` defaults to `.open`.
    /// An empty result is a successful empty list. Display *sorting* is a UI concern and
    /// never happens here; the stored order is what it is.
    public func list(_ filter: Filter = .open) throws -> OperationResult<[Todo]> {
        let (contents, note) = try readAll()
        return OperationResult(filter.apply(to: contents.todos), note: note)
    }

    /// `list` with the raw wire filter string (the CLI boundary). `nil` (argument absent)
    /// means the default `.open`; any unrecognized value is `DomainError.invalidFilter`.
    public func list(rawFilter raw: String?) throws -> OperationResult<[Todo]> {
        guard let filter = Filter.parse(raw: raw) else {
            throw DomainError.invalidFilter(raw ?? "")
        }
        return try list(filter)
    }

    // MARK: - sections (GUI-only — no CLI verbs)

    /// The section names in render order (the default area is implicit and unnamed).
    /// Includes any name that appears only on a todo (a hand edit), so the GUI can draw it.
    public func sections() throws -> OperationResult<[String]> {
        let (contents, note) = try readAll()
        return OperationResult(contents.knownSections, note: note)
    }

    /// `addSection(name)`: append a new, empty section. The name is trimmed; an empty or
    /// already-used name is rejected. GUI-only.
    public func addSection(_ rawName: String) throws -> OperationResult<Void> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { throw DomainError.emptySectionName }
        let (_, note) = try mutate { contents -> (value: Void, changed: Bool) in
            guard !contents.knownSections.contains(name) else {
                throw DomainError.duplicateSection(name)
            }
            contents.sections.append(name)
            return ((), true)
        }
        return OperationResult((), note: note)
    }

    /// `renameSection(from, to)`: rename a section in place — the section list and every
    /// todo that belongs to it, in one write. An unknown `from`, an empty `to`, or a `to`
    /// already in use is rejected. Renaming to the same name is a no-op. GUI-only.
    public func renameSection(_ rawFrom: String, to rawTo: String) throws -> OperationResult<Void> {
        let from = rawFrom.trimmingCharacters(in: .whitespacesAndNewlines)
        let to = rawTo.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !to.isEmpty else { throw DomainError.emptySectionName }
        let (_, note) = try mutate { contents -> (value: Void, changed: Bool) in
            guard let i = contents.sections.firstIndex(of: from) else {
                throw DomainError.unknownSection(from)
            }
            if from == to { return ((), false) }
            guard !contents.knownSections.contains(to) else {
                throw DomainError.duplicateSection(to)
            }
            contents.sections[i] = to
            for j in contents.todos.indices where contents.todos[j].section == from {
                contents.todos[j].section = to
            }
            return ((), true)
        }
        return OperationResult((), note: note)
    }

    /// `deleteSection(name)`: remove a section and move its todos to the default area —
    /// **a section is never a way to delete todos**. Returns the number of todos moved.
    /// An unknown name is rejected. GUI-only.
    public func deleteSection(_ rawName: String) throws -> OperationResult<Int> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        let (moved, note) = try mutate { contents -> (value: Int, changed: Bool) in
            guard let i = contents.sections.firstIndex(of: name) else {
                throw DomainError.unknownSection(name)
            }
            contents.sections.remove(at: i)
            var moved = 0
            for j in contents.todos.indices where contents.todos[j].section == name {
                contents.todos[j].section = nil
                moved += 1
            }
            return (moved, true)
        }
        return OperationResult(moved, note: note)
    }

    /// `moveSection(name, to:)`: reorder the declared sections. `index` is 0-based in the
    /// section list and is clamped (so `Int.max` moves it to the end). An unknown name is
    /// rejected. Only the section list changes — the groups travel whole, so the order of
    /// todos *within* every section is untouched. Returns the new section order.
    /// GUI-only.
    public func moveSection(_ rawName: String, to index: Int) throws -> OperationResult<[String]> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        let (order, note) = try mutate { contents -> (value: [String], changed: Bool) in
            guard let from = contents.sections.firstIndex(of: name) else {
                throw DomainError.unknownSection(name)
            }
            contents.sections.remove(at: from)
            let to = max(0, min(index, contents.sections.count))
            contents.sections.insert(name, at: to)
            return (contents.sections, to != from)
        }
        return OperationResult(order, note: note)
    }

    // MARK: - add

    /// `add(title)`: adds to the default (ungrouped) area, on top.
    public func add(_ rawTitle: String) throws -> OperationResult<Todo> {
        try add(rawTitle, section: nil)
    }

    /// `add(title, section)`: the title is trimmed; empty/whitespace-only is rejected with
    /// an actionable error. Duplicate titles are allowed. The new todo lands on the **top of
    /// its section** (optionally filing it under `section`; `nil` is the default area). A
    /// `section` that is not a known section is rejected. Returns the created todo (fresh id,
    /// `completedAt` null).
    public func add(_ rawTitle: String, section: String?) throws -> OperationResult<Todo> {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { throw DomainError.emptyTitle }
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            try contents.validateSection(section)
            let todo = Todo(title: title, section: section)
            contents.todos.insert(todo, at: contents.blockStart(of: section))
            return (todo, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - move (GUI-only — no CLI verb)

    /// `move(id, toSection:at:)`: place a todo at a 0-based `index` within `toSection`
    /// (`nil` is the default area), moving it between sections when needed. `index` is
    /// clamped into range, so `Int.max` appends. Covers both reorder-in-place and
    /// move-between-sections. An unknown id throws `DomainError.unknownID`; an unknown
    /// `section` is rejected. GUI-only.
    public func move(_ id: String, toSection section: String?, at index: Int) throws -> OperationResult<Todo> {
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let from = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            try contents.validateSection(section)
            var moved = contents.todos.remove(at: from)
            moved.section = section
            let start = contents.blockStart(of: section)
            let count = contents.todos.lazy.filter { $0.section == section }.count
            contents.todos.insert(moved, at: start + max(0, min(index, count)))
            return (moved, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - complete

    /// `complete(id)`: idempotent — an already-done todo is returned unchanged, no error.
    /// On a transition to done, `completedAt` is set to the current UTC time. An unknown
    /// id throws `DomainError.unknownID` carrying the full current open list. The inverse
    /// is `reopen` (GUI-only). The stored order is never changed.
    public func complete(_ id: String) throws -> OperationResult<Todo> {
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            var changed = false
            if !contents.todos[i].done {
                contents.todos[i].done = true
                contents.todos[i].completedAt = .now
                changed = true
            }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - update

    /// `update(id, title)`: rewrites a todo's title in place. GUI-only, with no CLI
    /// verb (the agent surface stays `list`/`add`/`complete`). The title is trimmed;
    /// empty/whitespace-only is rejected with `DomainError.emptyTitle`. An unchanged
    /// title is a no-op (nothing written). `done`, `completedAt` and the section are
    /// untouched, and the stored order is never changed. An unknown id throws
    /// `DomainError.unknownID` carrying the full current open list.
    public func update(_ id: String, title rawTitle: String) throws -> OperationResult<Todo> {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { throw DomainError.emptyTitle }
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            let changed = contents.todos[i].title != title
            if changed { contents.todos[i].title = title }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - setLink (GUI-only — no CLI verb)

    /// `setLink(id, to:)`: points a todo at a Bikeshed note (a bikeshed-root-relative path
    /// like `projects/App.md`), or clears the link with `nil`. The link is trimmed; an
    /// empty/whitespace-only value unlinks. GUI-only, with no CLI verb (the agent surface
    /// stays `list`/`add`/`complete`). An unchanged link is a no-op (nothing written).
    /// `title`, `done`, `completedAt` and the section are untouched, and the stored order
    /// is never changed. An unknown id throws `DomainError.unknownID` carrying the full
    /// current open list.
    public func setLink(_ id: String, to rawLink: String?) throws -> OperationResult<Todo> {
        let trimmed = rawLink?.trimmingCharacters(in: .whitespacesAndNewlines)
        let link = (trimmed?.isEmpty ?? true) ? nil : trimmed
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            let changed = contents.todos[i].link != link
            if changed { contents.todos[i].link = link }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - reopen

    /// `reopen(id)`: the inverse of `complete` — GUI-only, with no CLI verb (the agent
    /// surface stays `list`/`add`/`complete`). Idempotent — an already-open todo is
    /// returned unchanged, no error. On a transition back to open, `completedAt` is
    /// cleared. An unknown id throws `DomainError.unknownID` carrying the full current
    /// open list.
    public func reopen(_ id: String) throws -> OperationResult<Todo> {
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            var changed = false
            if contents.todos[i].done {
                contents.todos[i].done = false
                contents.todos[i].completedAt = nil
                changed = true
            }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - clearDone

    /// `clearDone()`: removes all done todos in one write. Returns the number removed.
    /// A no-op (no write) when there are no done todos.
    public func clearDone() throws -> OperationResult<Int> {
        let (count, note) = try mutate { contents -> (value: Int, changed: Bool) in
            let before = contents.todos.count
            contents.todos.removeAll { $0.done }
            let removed = before - contents.todos.count
            return (removed, removed > 0)
        }
        return OperationResult(count, note: note)
    }

    // MARK: - delete

    /// `delete(id)`: removes a single todo. GUI-only — the CLI has no delete verb.
    /// An unknown id throws `DomainError.unknownID` (with the open list).
    public func delete(_ id: String) throws -> OperationResult<Void> {
        let (_, note) = try mutate { contents -> (value: Void, changed: Bool) in
            guard let uuid = UUID(uuidString: id),
                  let i = contents.todos.firstIndex(where: { $0.id == uuid })
            else {
                throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
            }
            contents.todos.remove(at: i)
            return ((), true)
        }
        return OperationResult((), note: note)
    }
}
