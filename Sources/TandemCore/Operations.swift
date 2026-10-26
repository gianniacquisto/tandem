import Foundation

/// Contract-level ticket operations (spec: "Operations (core)"). These are the only
/// public surface the GUI and the CLI use; they define the shared semantics so
/// both faces behave identically. The CLI adds only encoding; the GUI adds only
/// presentation.
///
/// Ordering: the **stored order** is authoritative — a ticket's `position` in the
/// `tickets` table is its position in the list, and `add` puts a new ticket on **top**
/// by default. Order is never recomputed from `createdAt`. Lanes group the list; the
/// table is kept grouped by lane (default first, then each declared lane) so a reader
/// can walk it straight through (see `StoreContents`).
extension Store {

    // MARK: - list

    /// `list(query)`: the tickets in view, in **stored order**. `TicketQuery` is the one
    /// filter definition both faces use, so "the ready tickets" means one thing
    /// everywhere. An empty result is a successful empty list, never an error.
    /// Display *sorting* is a UI concern and never happens here.
    public func list(_ query: TicketQuery) throws -> OperationResult<[Ticket]> {
        let (contents, note) = try readAll()
        return OperationResult(query.apply(to: contents.todos), note: note)
    }

    /// `list(filter)`: the shipped three-value filter, kept as sugar over `TicketQuery`
    /// (ADR 0017 — `--filter open|done|all` is a documented contract). Defaults to `.open`.
    public func list(_ filter: Filter = .open) throws -> OperationResult<[Ticket]> {
        try list(filter.query)
    }

    /// `list` with the raw wire filter string (the CLI boundary). `nil` (argument absent)
    /// means the default `.open`; any unrecognized value is `DomainError.invalidFilter`.
    public func list(rawFilter raw: String?) throws -> OperationResult<[Ticket]> {
        guard let filter = Filter.parse(raw: raw) else {
            throw DomainError.invalidFilter(raw ?? "")
        }
        return try list(filter)
    }

    // MARK: - lanes (both faces: the app's tree and the CLI's `lane` verbs)

    /// The lane names in render order (the default area is implicit and unnamed).
    /// Includes any name that appears only on a ticket (a hand edit), so the GUI can draw it.
    public func lanes() throws -> OperationResult<[String]> {
        let (contents, note) = try readAll()
        return OperationResult(contents.knownLanes, note: note)
    }

    /// `addLane(name)`: append a new, empty lane. The name is trimmed; an empty or
    /// already-used name is rejected. Returns the lane list after the write, so a caller
    /// (the CLI) reports the state without a second, unlocked read.
    public func addLane(_ rawName: String) throws -> OperationResult<[String]> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { throw DomainError.emptyLaneName }
        let (lanes, note) = try mutate { contents -> (value: [String], changed: Bool) in
            guard !contents.knownLanes.contains(name) else {
                throw DomainError.duplicateLane(name)
            }
            contents.lanes.append(name)
            return (contents.knownLanes, true)
        }
        return OperationResult(lanes, note: note)
    }

    /// `renameLane(from, to)`: rename a lane in place — the lane list and every
    /// ticket that belongs to it, in one write. An unknown `from`, an empty `to`, or a `to`
    /// already in use is rejected. Renaming to the same name is a no-op. Returns the lane
    /// list after the write.
    public func renameLane(_ rawFrom: String, to rawTo: String) throws -> OperationResult<[String]> {
        let from = rawFrom.trimmingCharacters(in: .whitespacesAndNewlines)
        let to = rawTo.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !to.isEmpty else { throw DomainError.emptyLaneName }
        let (lanes, note) = try mutate { contents -> (value: [String], changed: Bool) in
            guard let i = contents.lanes.firstIndex(of: from) else {
                throw DomainError.unknownLane(from)
            }
            if from == to { return (contents.knownLanes, false) }
            guard !contents.knownLanes.contains(to) else {
                throw DomainError.duplicateLane(to)
            }
            contents.lanes[i] = to
            for j in contents.todos.indices where contents.todos[j].lane == from {
                contents.todos[j].lane = to
            }
            return (contents.knownLanes, true)
        }
        return OperationResult(lanes, note: note)
    }

    /// `deleteLane(name)`: remove a lane and move its tickets to the default area —
    /// **a lane is never a way to delete tickets**. An unknown name is rejected. Returns
    /// how many tickets moved, plus the lane list after the write.
    public func deleteLane(_ rawName: String) throws -> OperationResult<LaneDeletion> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        let (deletion, note) = try mutate { contents -> (value: LaneDeletion, changed: Bool) in
            guard let i = contents.lanes.firstIndex(of: name) else {
                throw DomainError.unknownLane(name)
            }
            contents.lanes.remove(at: i)
            var moved = 0
            for j in contents.todos.indices where contents.todos[j].lane == name {
                contents.todos[j].lane = nil
                moved += 1
            }
            return (LaneDeletion(lanes: contents.knownLanes, moved: moved), true)
        }
        return OperationResult(deletion, note: note)
    }

    /// `moveLane(name, to:)`: reorder the declared lanes. `index` is 0-based in the
    /// lane list and is clamped (so `Int.max` moves it to the end). An unknown name is
    /// rejected. Only the lane list changes — the groups travel whole, so the order of
    /// tickets *within* every lane is untouched. Returns the new lane order.
    /// GUI-only.
    public func moveLane(_ rawName: String, to index: Int) throws -> OperationResult<[String]> {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        let (order, note) = try mutate { contents -> (value: [String], changed: Bool) in
            guard let from = contents.lanes.firstIndex(of: name) else {
                throw DomainError.unknownLane(name)
            }
            contents.lanes.remove(at: from)
            let to = max(0, min(index, contents.lanes.count))
            contents.lanes.insert(name, at: to)
            return (contents.lanes, to != from)
        }
        return OperationResult(order, note: note)
    }

    // MARK: - add

    /// `add(title, lane, status, assignee, origin)`: the title is trimmed; empty/whitespace-only
    /// is rejected with an actionable error. Duplicate titles are allowed. The new ticket
    /// lands on the **top of its lane** (`nil` is the default area); a lane that is not a
    /// known lane is rejected. A title alone stays the default — a ticket is never born
    /// archived, and the assignee rule is the same as `set`'s (see `normalizeAssignee`).
    /// The filer is stamped here and never touched again: `origin` is the one field
    /// `set` cannot change (ADR 0020).
    public func add(
        _ rawTitle: String,
        lane: String? = nil,
        status: Status = .backlog,
        assignee: String? = nil,
        origin: String = "human"
    ) throws -> OperationResult<Ticket> {
        let title = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { throw DomainError.emptyTitle }
        let who = Self.normalizeAssignee(assignee)
        let filer = Self.normalizeOrigin(origin)
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            try contents.validateLane(lane)
            var todo = Ticket(title: title, assignee: who, origin: filer, lane: lane)
            // Born in a stage: `setStatus` stamps `completedAt` if that stage is done.
            todo.setStatus(status)
            contents.todos.insert(todo, at: contents.blockStart(of: lane))
            return (todo, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - Id resolution

    /// The store's one rule for "which ticket is this id?": the id parsed as a
    /// UUID, and that ticket's index in `todos` — `nil` when the id is
    /// malformed or the store does not carry it.
    ///
    /// **One spelling, so it cannot drift.** Every id-taking operation —
    /// `move`, `place`, `complete`, `set`, `setLink`, `reopen`, `delete`, the
    /// comment append, the note promotion — resolves through here, and so does
    /// a caller that only wants to know whether a ticket is on the board
    /// (`ticket(id:)`). The rule used to be written out at each site, and a
    /// second copy of it in the CLI was a third: one question, one answer.
    static func resolveTicket(_ raw: String, in todos: [Ticket]) -> (uuid: UUID, index: Int)? {
        guard let uuid = UUID(uuidString: raw),
              let index = todos.firstIndex(where: { $0.id == uuid })
        else {
            return nil
        }
        return (uuid, index)
    }

    /// The ticket a raw id names, or the unknown-id failure — the shape every
    /// id-taking operation raises, carrying the open list the caller recovers
    /// from in one turn.
    static func requireTicket(_ raw: String, in todos: [Ticket]) throws -> (uuid: UUID, index: Int) {
        guard let found = resolveTicket(raw, in: todos) else {
            throw DomainError.unknownID(id: raw, open: todos.filter { !$0.done })
        }
        return found
    }

    /// `ticket(id:)`: the ticket a raw id names — the store's own answer to
    /// "which ticket is this id?", for a caller that only wants to know whether
    /// a ticket is on the board.
    ///
    /// A **lock-free read**, like `todo list`: the caller is asking a question
    /// about the board, not changing it, so the store is read without taking
    /// the write lock. An id the store does not carry throws
    /// `DomainError.unknownID` with the current open list — the same failure
    /// `complete` and `set` raise, from the one place that decides it.
    public func ticket(id raw: String) throws -> OperationResult<Ticket> {
        let (contents, note) = try readAll()
        do {
            let found = try Self.requireTicket(raw, in: contents.todos)
            return OperationResult(contents.todos[found.index], note: note)
        } catch {
            // A read that recovered a corrupt file says so, even when it fails.
            guard let note else { throw error }
            let underlying: any TandemError = error as? any TandemError
                ?? StoreIOError.io("\(error)")
            throw RecoveryFailure(underlying: underlying, note: note)
        }
    }

    // MARK: - move

    /// `move(id, toLane:at:)`: place a ticket at a 0-based `index` within `toLane`
    /// (`nil` is the default area), moving it between lanes when needed. `index` is
    /// clamped into range, so `Int.max` appends. Covers both reorder-in-place and
    /// move-between-lanes. An unknown id throws `DomainError.unknownID`; an unknown
    /// `lane` is rejected. The agent's `todo move` and the GUI's drag both land here.
    public func move(_ id: String, toLane lane: String?, at index: Int) throws -> OperationResult<Ticket> {
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let from = try Self.requireTicket(id, in: contents.todos).index
            try contents.validateLane(lane)
            var moved = contents.todos.remove(at: from)
            insert(&moved, into: lane, of: &contents, at: index)
            return (moved, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - place

    /// `place(id, lane:status:at:)`: the board's drag — **one write** that sets the lane and
    /// the Status together and puts the ticket at a 0-based `index` within the lane
    /// (`nil` is the default area; clamped, so `Int.max` appends). `move` changes a
    /// ticket's place and `set` changes its fields; a board drop does both at once, and
    /// two writes would let a drag land a ticket in a cell yet leave it in the old column
    /// if the second one failed.
    ///
    /// The rules are its two halves', unchanged: an unknown id throws
    /// `DomainError.unknownID`, an unknown lane is rejected, and `setStatus` stamps or
    /// clears `completedAt`. A drop that lands the ticket exactly where it already was
    /// writes nothing. GUI-only — the agent surface keeps `set --status` and `move`.
    public func place(
        _ id: String,
        lane: String?,
        status: Status,
        at index: Int = .max
    ) throws -> OperationResult<Ticket> {
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let from = try Self.requireTicket(id, in: contents.todos).index
            try contents.validateLane(lane)
            let before = contents.todos
            var moved = contents.todos.remove(at: from)
            moved.setStatus(status)
            insert(&moved, into: lane, of: &contents, at: index)
            return (moved, contents.todos != before)
        }
        return OperationResult(todo, note: note)
    }

    /// The one place a lane insertion's arithmetic lives: `index` is 0-based within `lane`
    /// and clamped into range, so `Int.max` appends. `move` and `place` differ only in what
    /// they set on the ticket before it goes back in.
    private func insert(
        _ todo: inout Ticket,
        into lane: String?,
        of contents: inout StoreContents,
        at index: Int
    ) {
        todo.lane = lane
        let start = contents.blockStart(of: lane)
        let count = contents.todos.lazy.filter { $0.lane == lane }.count
        contents.todos.insert(todo, at: start + max(0, min(index, count)))
    }

    // MARK: - complete

    /// `complete(id)`: idempotent — an already-done ticket is returned unchanged, no error.
    /// On a transition to done, `completedAt` is set to the current UTC time. An unknown
    /// id throws `DomainError.unknownID` carrying the full current open list. The inverse
    /// is `reopen`. The stored order is never changed.
    public func complete(_ id: String) throws -> OperationResult<Ticket> {
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            var changed = false
            if contents.todos[i].status != .done {
                contents.todos[i].setStatus(.done)
                changed = true
            }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - set

    /// `set(id, …)`: changes the named fields in **one write**, leaving the rest
    /// untouched. Each argument is an `Edit`, so "leave it alone" (`.unchanged`, the
    /// default) and "clear it" (`.set(nil)` on an optional field) are different
    /// instructions — a pile of optional parameters could not express that.
    ///
    /// The field rules are the ones the rest of the store uses: a title is trimmed and
    /// must not be empty; a description and an assignee are trimmed, and empty means
    /// cleared; `human` is stored lowercase; `status` stamps or clears `completedAt`. A
    /// call that changes nothing writes nothing. An unknown id throws
    /// `DomainError.unknownID` carrying the current open list. The settable fields are
    /// exactly the parameters: `origin` is not among them, because it is written once,
    /// at add, and never changed (ADR 0020).
    public func set(
        _ id: String,
        title: Edit<String> = .unchanged,
        status: Edit<Status> = .unchanged,
        assignee: Edit<String?> = .unchanged,
        description: Edit<String?> = .unchanged,
        archived: Edit<Bool> = .unchanged
    ) throws -> OperationResult<Ticket> {
        // Validated before the write, so a bad value never reaches the store.
        var newTitle: String?
        if let raw = title.newValue {
            let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { throw DomainError.emptyTitle }
            newTitle = trimmed
        }
        let newAssignee = assignee.isSet ? Self.normalizeAssignee(assignee.newValue ?? nil) : nil
        let newDescription = description.isSet ? Self.normalizeDescription(description.newValue ?? nil) : nil

        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            var todo = contents.todos[i]
            var changed = false
            if let newTitle, todo.title != newTitle {
                todo.title = newTitle
                changed = true
            }
            if status.isSet, let newStatus = status.newValue, todo.status != newStatus {
                todo.setStatus(newStatus)
                changed = true
            }
            if assignee.isSet, todo.assignee != newAssignee {
                todo.assignee = newAssignee
                changed = true
            }
            if description.isSet, todo.description != newDescription {
                todo.description = newDescription
                changed = true
            }
            if archived.isSet, let value = archived.newValue, todo.archived != value {
                todo.archived = value
                changed = true
            }
            contents.todos[i] = todo
            return (todo, changed)
        }
        return OperationResult(todo, note: note)
    }

    /// `update(id, title)`: rewrites a ticket's title in place — the GUI's inline rename,
    /// the same instruction as `set(id, title:)`. Kept because renaming is the one field
    /// edit the list does on every double-click.
    public func update(_ id: String, title rawTitle: String) throws -> OperationResult<Ticket> {
        try set(id, title: .set(rawTitle))
    }

    // MARK: - answer

    /// `answer(id, status, assignee, body)`: the human's answer recorded and the hold
    /// closed — a `## Answer — <date>` block appended to the description, the status
    /// the answer opens and the next assignee, all in **one** write.
    ///
    /// **The write cannot half-land.** Recording the answer in the description and
    /// moving the pair in a second call is what let answered tickets sit in the inbox
    /// wearing `human` (D2522EC3, seven in the 2026-10-21 audit); one mutation means an
    /// answer cannot be written without the pair moving with it. `assignee` is the next
    /// owner — `nil` (or `""`, the store's spelling for the unowned) clears it, because
    /// the question is settled. `body` must carry text: `DescriptionBody.required`
    /// refuses the empty stdin body, and a refused answer writes nothing (ADR 0024).
    /// An unknown id throws `DomainError.unknownID` carrying the open list, as every
    /// id-taking operation does.
    public func answer(
        _ id: String,
        status: Status,
        assignee: String? = nil,
        body rawBody: String,
        at now: Date = .now
    ) throws -> OperationResult<Ticket> {
        let body = try DescriptionBody.required(rawBody)
        let who = Self.normalizeAssignee(assignee)
        let block = Self.answerBlock(body, at: now)
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            var todo = contents.todos[i]
            todo.description = Self.appending(block, to: todo.description)
            todo.setStatus(status, at: now)
            todo.assignee = who
            contents.todos[i] = todo
            return (todo, true)
        }
        return OperationResult(todo, note: note)
    }

    /// The answer block: `## Answer — <yyyy-MM-dd>` (UTC, the day the answer was
    /// recorded), a blank line, then the body. The heading is what
    /// `DescriptionBody.answered` reads back, so the written form and the read agree
    /// by construction.
    static func answerBlock(_ body: String, at now: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyy-MM-dd"
        return "## Answer — \(formatter.string(from: now))\n\n\(body)"
    }

    /// A section appended to a description, separated by one blank line — the
    /// ticket-as-document rule: a description is written once and appended to, never
    /// rewritten. No description at all means the section *is* the description.
    static func appending(_ section: String, to description: String?) -> String {
        guard let existing = Store.normalizeDescription(description) else { return section }
        return existing + "\n\n" + section
    }

    // MARK: - Field rules

    /// The assignee's spelling rule: trimmed; empty means unassigned; the reserved word
    /// `human` is stored lowercase so it is one name; anything else is an agent's name
    /// exactly as given. Nothing validates the name — the persona registry that would is
    /// out of scope (ADR 0017).
    static func normalizeAssignee(_ raw: String?) -> String? {
        guard let raw else { return nil }
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        return name.lowercased() == "human" ? "human" : name
    }

    /// The origin's spelling rule — the assignee's, except there is no absent value: a
    /// filer always exists, so `nil` and empty both mean the human (the face that adds
    /// without naming one is the app, the human's face); the reserved word `human` is
    /// stored lowercase so it is one name; anything else is an agent's name exactly as
    /// given. Nothing validates the name — the persona registry that would is out of
    /// scope, as with `assignee` (ADR 0020).
    static func normalizeOrigin(_ raw: String?) -> String {
        guard let raw else { return "human" }
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return "human" }
        return name.lowercased() == "human" ? "human" : name
    }

    /// A description is trimmed at its edges and an empty result means `nil` — the rule a
    /// note link already follows.
    static func normalizeDescription(_ raw: String?) -> String? {
        guard let raw else { return nil }
        let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    // MARK: - setLink (GUI-only — no CLI verb)

    /// `setLink(id, to:)`: points a ticket at a Bikeshed note (a bikeshed-root-relative path
    /// like `projects/App.md`), or clears the link with `nil`. The link is trimmed; an
    /// empty/whitespace-only value unlinks. GUI-only: no CLI verb links notes — the agent
    /// surface stays tickets, and a note is the human's connection (ADR 0010). An
    /// unchanged link is a no-op (nothing written).
    /// `title`, `done`, `completedAt` and the lane are untouched, and the stored order
    /// is never changed. An unknown id throws `DomainError.unknownID` carrying the full
    /// current open list.
    public func setLink(_ id: String, to rawLink: String?) throws -> OperationResult<Ticket> {
        let trimmed = rawLink?.trimmingCharacters(in: .whitespacesAndNewlines)
        let link = (trimmed?.isEmpty ?? true) ? nil : trimmed
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            let changed = contents.todos[i].link != link
            if changed { contents.todos[i].link = link }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - reopen

    /// `reopen(id)`: the inverse of `complete`. Idempotent — a ticket that is not done is
    /// returned unchanged, no error. **A reopen lands in `backlog`**, because completing
    /// erased the stage it came from: with `done` as a Status value there is no previous
    /// stage to restore (ADR 0017). A caller that means a particular stage sets it — a
    /// status is just another field. `completedAt` is cleared. An unknown id throws
    /// `DomainError.unknownID` carrying the full current open list.
    public func reopen(_ id: String) throws -> OperationResult<Ticket> {
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            var changed = false
            if contents.todos[i].status == .done {
                contents.todos[i].setStatus(.backlog)
                changed = true
            }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - undo

    /// `undo(offer)`: take a completion back. The ticket returns to **`backlog`** —
    /// the only stage a reopen can land in, because completing it erased the stage it
    /// came from (ADR 0017) — and `completedAt` is cleared.
    ///
    /// This is `reopen` under the name the interaction has: the undo of *a
    /// completion*, not a general reopen. It is idempotent, like `reopen` — a
    /// ticket that is no longer done is returned unchanged, so an undo that lands
    /// after the ticket moved on does nothing rather than clobbering it. An
    /// unknown id throws `DomainError.unknownID` carrying the full current open
    /// list. GUI-only: the agent face has no undo (an agent's completion is
    /// deliberate, and `reopen` is its way back).
    public func undo(_ offer: CompletionUndo) throws -> OperationResult<Ticket> {
        try reopen(offer.id.uuidString)
    }

    // MARK: - archiveDone

    /// `archiveDone()`: archives every done ticket in one write and returns how many it
    /// archived — already-archived ones are left alone, so a second call returns 0 and
    /// writes nothing. **Nothing is deleted.** This replaces the old "clear done", which
    /// removed tickets; archiving only takes them out of the default views, and any
    /// ticket can be brought back with `set(id, archived: .set(false))`.
    public func archiveDone() throws -> OperationResult<Int> {
        let (count, note) = try mutate { contents -> (value: Int, changed: Bool) in
            var archived = 0
            for i in contents.todos.indices
            where contents.todos[i].done && !contents.todos[i].archived {
                contents.todos[i].archived = true
                archived += 1
            }
            return (archived, archived > 0)
        }
        return OperationResult(count, note: note)
    }

    // MARK: - archive (one ticket)

    /// `archive(id)`: sets one ticket aside — the per-ticket half of `archiveDone`,
    /// which sweeps only the done tickets and so cannot reach an open one.
    /// Idempotent — an already-archived ticket is returned unchanged, no error,
    /// nothing written — like `complete`. An unknown id throws
    /// `DomainError.unknownID` carrying the current open list. **Nothing is
    /// deleted.** Archiving only takes the ticket out of the default views, and
    /// any ticket can be brought back with `set(id, archived: .set(false))`.
    public func archive(_ id: String) throws -> OperationResult<Ticket> {
        let (todo, note) = try mutate { contents -> (value: Ticket, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            var changed = false
            if !contents.todos[i].archived {
                contents.todos[i].archived = true
                changed = true
            }
            return (contents.todos[i], changed)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - delete

    /// `delete(id)`: removes a single ticket. GUI-only — the CLI has no delete verb.
    /// An unknown id throws `DomainError.unknownID` (with the open list).
    public func delete(_ id: String) throws -> OperationResult<Void> {
        let (_, note) = try mutate { contents -> (value: Void, changed: Bool) in
            let i = try Self.requireTicket(id, in: contents.todos).index
            contents.todos.remove(at: i)
            return ((), true)
        }
        return OperationResult((), note: note)
    }
}
