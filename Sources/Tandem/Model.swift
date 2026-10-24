import Foundation
import Observation
import TandemCore

/// The single shared app model: a thin, observable adapter over `TandemCore.Store`
/// (ADR 0001). All ticket semantics live in the store; this class only bridges them to
/// the UI.
///
/// One instance is shared by both faces — the SwiftUI main window (which tracks it via
/// Observation) and the AppKit menu bar (which reads it on every open) — so it is a
/// deliberate app-wide singleton: there is exactly one store, at
/// `TandemHome.directory` — `~/.tandem` for the shipped app, `~/.tandem-dev` for a
/// build run from source.
///
/// Everything here runs on the main thread: mutations are driven either by a UI action
/// or by the FSEvents callback, which is scheduled on the main run loop.
@Observable
final class TandemModel {
    static let shared = TandemModel()

    private let store: Store
    private let watcher: StoreWatcher

    /// The session records' own file (`sessions.json`, beside the ticket
    /// store). The core owns it, its lock and its vocabulary; the app only
    /// ever reads it (spec `FC770B8A`).
    ///
    /// It is rooted at the **store's own directory**, which is what makes an
    /// agent's write visible for free: the file is replaced by rename inside
    /// the directory the `StoreWatcher` already watches, so no second watcher
    /// and no polling is needed.
    private let sessionStore: Sessions

    /// The sprints records' own file (`sprints.json`, beside the ticket
    /// store). The core owns it, its lock and its vocabulary; the app only
    /// ever reads it (spec `4F746EFE`, R3).
    ///
    /// It is rooted at the **store's own directory**, which makes an agent's
    /// write visible for free — the file is replaced by rename inside the
    /// directory the `StoreWatcher` already watches, so no second watcher and
    /// no polling is needed.
    private let sprintStore: Sprints

    /// All tickets in stored order (the order the human arranged — new tickets land on top;
    /// never re-sorted here).
    private(set) var tickets: [Ticket] = []
    /// The recorded sessions, newest `startedAt` first — the core's own order
    /// (`Sessions.list`). Read-only: the app never creates, ends or links a
    /// record; an agent does that, through the CLI.
    private(set) var sessions: [Session] = []
    /// The recorded sprints, newest `openedAt` first — the core's own order
    /// (`Sprints.list`). Read-only: the app never opens, runs or closes a
    /// sprint; an agent does that, through the CLI.
    private(set) var sprints: [Sprint] = []
    /// A note from the sessions file — it names a corrupt file that was kept
    /// aside and the list that restarted empty. The sessions face says so
    /// quietly; the board's banner is about the ticket list, and this is a
    /// different file.
    private(set) var sessionNote: String?
    /// A note from the sprints file — it names a corrupt file that was kept
    /// aside and the list that restarted empty. The sprints face says so
    /// quietly; the board's banner is about the ticket list, and this is a
    /// different file.
    private(set) var sprintNote: String?
    /// The declared lane names, in render order. The default (ungrouped) area at the
    /// top of the list is implicit and unnamed, so it is not in here.
    private(set) var lanes: [String] = []
    /// Set when a call triggered corrupt-store recovery; names the backup path. Sticky
    /// until the user dismisses it (a later successful reload does not clear it).
    private(set) var recoveryNote: String?
    /// The most recent operation error, surfaced to the user. Cleared on the next
    /// successful operation/reload.
    private(set) var lastError: String?
    /// The lane a new ticket is filed into: the human's last choice, cached in the
    /// settings file (`TandemSettings.defaultLane`). `nil` is the default area. It is a
    /// cache, not a store field — an agent files with `--lane` — and it is clamped on every
    /// reload, so a lane that was renamed or deleted simply stops being the cache.
    private(set) var defaultLane: String?
    /// The completion this app made that the human can still take back — the toast's
    /// one undo (ticket 9A9EC9A4). **One at a time, and it expires**: the newest
    /// completion replaces the last, and the offer runs out after
    /// `CompletionUndo.window` seconds.
    ///
    /// It is raised by the *act* of completing, never by observing one: a completion
    /// that arrives from outside (the CLI, a hand edit) comes through `reload()`,
    /// which raises nothing — the toast answers this click, not a change in the file.
    private(set) var undoOffer: CompletionUndo?

    // The shipped filters, defined once in the core (`Filter.query`): the app's counts
    // and the CLI's `--filter` are views over the same predicate, not two definitions —
    // the archive excepted, where the CLI asks for it by name (ADR 0017, 2026-10-21).
    var openTickets: [Ticket] { Filter.open.apply(to: tickets) }
    var doneTickets: [Ticket] { Filter.done.apply(to: tickets) }
    /// The archive: out of every default view, still in the store. The list's fourth state.
    var archivedTickets: [Ticket] { TicketQuery(archived: .only).apply(to: tickets) }
    /// What is waiting for the person: the tickets agents assigned to `human`.
    var inboxTickets: [Ticket] { TicketQuery.inbox.apply(to: tickets) }

    private init(store: Store = Store()) {
        self.store = store
        // The sessions file lives in the store's directory — the one the
        // watcher below already watches. So does the sprints file.
        self.sessionStore = Sessions(directory: store.directory)
        self.sprintStore = Sprints(directory: store.directory)
        self.defaultLane = TandemSettings.defaultLane()
        // Watch the store *directory* (the file is replaced via atomic rename) so an
        // agent writing the store refreshes the UI without a manual reload.
        self.watcher = StoreWatcher(directory: store.directory)
        // Attach the callback only now that `self` is fully initialized.
        self.watcher.onEvent = { [weak self] in self?.reload() }
        self.reload()
        self.watcher.start()
    }

    deinit { watcher.stop() }

    // MARK: - Loading

    /// Re-reads the whole store lock-free and refreshes observable state. Called at
    /// startup, after every operation, and on external (FSEvents) changes — so the UI
    /// is always current.
    func reload() {
        do {
            let result = try store.list(.all)
            tickets = result.value
            lanes = (try? store.lanes().value) ?? []
            if let lane = defaultLane, !lanes.contains(lane) { setDefaultLane(nil) }
            if let note = result.note { recoveryNote = note }
            reloadSessions()
            reloadSprints()
            lastError = nil
        } catch {
            apply(error: error)
        }
    }

    /// The session records, read from their own file. A read that fails leaves
    /// the list as it was: the ticket list and the session list are separate
    /// files, and one of them being unreadable says nothing about the other.
    private func reloadSessions() {
        guard let result = try? sessionStore.list() else { return }
        sessions = result.value
        sessionNote = result.note
    }

    /// The sprint records, read from their own file. A read that fails leaves
    /// the list as it was: the ticket list and the sprint list are separate
    /// files, and one of them being unreadable says nothing about the other.
    private func reloadSprints() {
        guard let result = try? sprintStore.list() else { return }
        sprints = result.value
        sprintNote = result.note
    }

    /// The ticket with this id, or `nil` when the store does not hold it —
    /// the session sheet's way of saying "a session carried a ticket that is
    /// no longer here".
    func ticket(with id: UUID) -> Ticket? {
        tickets.first { $0.id == id }
    }

    // MARK: - Operations (thin, non-throwing wrappers over the store)

    func add(_ rawTitle: String) {
        run { try self.store.add(rawTitle) }
    }

    /// Adds a ticket on top of `lane` (`nil` = the default area).
    func add(_ rawTitle: String, lane: String?) {
        run { try self.store.add(rawTitle, lane: lane) }
    }

    // MARK: - Lanes and order (the lane lifecycle is also on the CLI; reordering is GUI-only)

    /// Remembers (or, with `nil`, clears) the lane new tickets are filed into. A failure
    /// to persist is silent: the next launch falls back to the default area, and a
    /// settings write must never interrupt filing a ticket.
    func setDefaultLane(_ lane: String?) {
        defaultLane = lane
        try? TandemSettings.setDefaultLane(lane)
    }

    func addLane(_ name: String) {
        run { try self.store.addLane(name) }
    }

    func renameLane(_ from: String, to: String) {
        run { try self.store.renameLane(from, to: to) }
    }

    /// Deleting a lane moves its tickets to the default area — it never deletes them.
    func deleteLane(_ name: String) {
        run { try self.store.deleteLane(name) }
    }

    /// Reorders the lanes. `index` is 0-based and clamped; the groups (and the order
    /// of tickets inside them) travel whole.
    func moveLane(_ name: String, to index: Int) {
        run { try self.store.moveLane(name, to: index) }
    }

    /// Reorders: moves a ticket to `index` (0-based) within `lane` (`nil` = the default
    /// area), moving it between lanes when needed.
    func move(_ id: String, toLane lane: String?, at index: Int) {
        run { try self.store.move(id, toLane: lane, at: index) }
    }

    /// Complete is idempotent at the store level, so completing an already-done ticket
    /// is a no-op.
    ///
    /// A completion **this app made** leaves an undo offer behind — the toast that
    /// puts the ticket back in `backlog` if the click was a mis-click. A completion
    /// that arrives from elsewhere (the CLI, a hand edit) comes through `reload()`
    /// instead, and raises no offer.
    func complete(_ id: String) {
        // Whether this call is a *transition* to done: completing a ticket that is
        // already done changes nothing, so there is nothing to take back.
        let wasDone = tickets.first(where: { $0.id.uuidString == id })?.done ?? false
        guard let completed = run({ try self.store.complete(id) }) else { return }
        guard !wasDone, completed.status == .done else { return }
        undoOffer = CompletionUndo(id: completed.id, title: completed.title)
        scheduleUndoExpiry()
    }

    /// Takes the completion back: the ticket returns to `backlog` and `completedAt`
    /// is cleared — the landing a reopen has (ADR 0017). The offer is spent either
    /// way: one undo, not a history.
    ///
    /// A ticket that has moved on since (deleted, reopened, re-staged) is left alone:
    /// the undo is for the mis-click, not for a later decision.
    func undoCompletion() {
        guard let offer = undoOffer else { return }
        undoOffer = nil
        guard tickets.first(where: { $0.id == offer.id })?.done == true else { return }
        run { try self.store.undo(offer) }
    }

    /// The inverse of `complete` — the checkbox on a done row reopens it. Also
    /// idempotent: reopening an already-open ticket is a no-op.
    func reopen(_ id: String) {
        run { try self.store.reopen(id) }
    }

    /// Rewrites a ticket's title (inline edit). GUI-only — no CLI verb; the empty-title
    /// and unknown-id errors surface verbatim.
    func update(_ id: String, title: String) {
        run { try self.store.update(id, title: title) }
    }

    /// Moves a ticket to a Status — the board's column move and the card menu's stage
    /// change. One write; the store stamps `completedAt` when the stage is done.
    func setStatus(_ id: String, to status: Status) {
        run { try self.store.set(id, status: .set(status)) }
    }

    /// The board's drag: lane and Status in one write, at a 0-based index within the lane
    /// (`Int.max` appends). Two writes would let a drop half-land.
    func place(_ id: String, lane: String?, status: Status, at index: Int = .max) {
        run { try self.store.place(id, lane: lane, status: status, at: index) }
    }

    /// Points a ticket at a Bikeshed note (bikeshed-root-relative path), or clears the link
    /// with `nil`. GUI-only — no CLI verb; the unknown-id error surfaces verbatim.
    func setLink(_ id: String, to link: String?) {
        run { try self.store.setLink(id, to: link) }
    }

    /// Files a selection from a note as a ticket, linked back to that note (the
    /// reverse of the ticket→note link). The ticket lands on top of the **default
    /// lane**, in `backlog` — the same place the quick-add files.
    ///
    /// Returns the error message when the filing failed, rather than raising a
    /// banner here: the action belongs to the bikeshed face, so that is where
    /// the human must see why nothing was filed.
    @discardableResult
    func addFromNote(_ selection: String, notePath: String) -> String? {
        do {
            let result = try store.addFromNote(selection,
                                               notePath: notePath,
                                               lane: defaultLane)
            if let note = result.note { recoveryNote = note }
            reload()
            return nil
        } catch {
            return Self.message(for: error)
        }
    }

    /// Archives every done ticket — "clear done" without the deleting: finished work
    /// leaves the view and stays in the file.
    func archiveDone() {
        run { try self.store.archiveDone() }
    }

    /// Sets one ticket's archive flag. Unarchiving is the way back from "Archive done";
    /// archiving a single ticket is not offered in the GUI yet (bulk is enough).
    func setArchived(_ id: String, _ archived: Bool) {
        run { try self.store.set(id, archived: .set(archived)) }
    }

    /// Applies the ticket sheet's edits. Fields travel through **one** `set` (title, Status,
    /// body — each only when it actually changed), and the lane is its own `move`, because
    /// changing a lane also decides where in the lane the ticket lands. A sheet that was
    /// opened, read, and saved writes nothing.
    func saveTicket(_ id: String, edits: TicketEdits) {
        guard let stored = tickets.first(where: { $0.id.uuidString == id }) else { return }
        let trimmedBody = edits.description.trimmingCharacters(in: .whitespacesAndNewlines)
        let titleChanged = edits.title != stored.title
        let bodyChanged = (stored.description ?? "") != trimmedBody
        let statusChanged = edits.status != stored.status
        if titleChanged || bodyChanged || statusChanged {
            run {
                try self.store.set(
                    id,
                    title: titleChanged ? .set(edits.title) : .unchanged,
                    status: statusChanged ? .set(edits.status) : .unchanged,
                    description: bodyChanged ? .set(trimmedBody) : .unchanged)
            }
        }
        if edits.lane != stored.lane {
            run { try self.store.move(id, toLane: edits.lane, at: .max) }
        }
    }

    /// GUI-only in v1 — there is no delete verb in the CLI.
    func delete(_ id: String) {
        run { try self.store.delete(id) }
    }

    // MARK: - Comments (the sidecar)

    /// The ticket's discussion: the sidecar's log, oldest first — the core's read
    /// path (`Store.commentRead(ticketID:)`), which is empty for a ticket with no
    /// log and never an error for one.
    ///
    /// A damaged log *is* an error, and it stays one: the log is append-only, so a
    /// bad line is repair, not recovery. The sheet says so where the conversation
    /// would have been, rather than showing an empty list that reads as "nobody
    /// has commented".
    ///
    /// A store locked by another process is a third thing again: the read waited
    /// out the lock and gave up, the log is untouched, and the sheet says "busy —
    /// try again" rather than "repair this file". The mapping lives in the core
    /// (`Sources/TandemCore/CommentRead.swift`) so the test seam can reach it.
    ///
    /// The read runs off the main thread: it takes the store's `flock`, and a CLI
    /// write holding the lock for the core's ~2 s window must not hold the UI's
    /// main thread for it. The caller awaits the answer and draws it when it
    /// lands — a slow read is a state, not a freeze. (`Store` is `Sendable`, and
    /// the read is the same file-and-flock call the CLI makes from its own
    /// process, so a background thread is no different from a background process.)
    func comments(for ticketID: String) async -> CommentRead {
        let store = self.store
        return await Task.detached(priority: .userInitiated) {
            store.commentRead(ticketID: ticketID)
        }.value
    }

    /// Posts the human's comment to the ticket's log — the app's write path, the
    /// same sidecar the CLI's `todo comment add` writes, with the human as the
    /// author.
    ///
    /// An empty body is refused by the core (`DomainError.emptyCommentBody`), the
    /// way the CLI refuses it, and nothing is written: the human's own text stays
    /// in the input, so it can be fixed and posted again. The refusal comes back
    /// as the outcome, not as the app-wide banner — the comment belongs to the
    /// ticket sheet, so that is where the human must see why nothing was written.
    ///
    /// On success the outcome carries the record that was written — the comment
    /// itself, from the append that wrote it. The log is not read back: the
    /// write path takes the store's lock once, so a comment that **was** written
    /// is never handed back as "busy" (the sheet's own read path reads the log,
    /// with its own `.busy` and `.failed`).
    func postComment(_ ticketID: String, body: String) -> CommentPostOutcome {
        do {
            let result = try store.commentWrite(ticketID: ticketID, body: body)
            if let note = result.note { recoveryNote = note }
            return .posted(result.value)
        } catch {
            if let recovery = error as? RecoveryFailure { recoveryNote = recovery.note }
            return .refused(message: Self.message(for: error))
        }
    }

    // MARK: - Promotion to a note (GUI-only, the mirror of `addFromNote`)

    /// Promotes a ticket to a bikeshed note: the note is made from the ticket's title and
    /// description, and the ticket's `link` points at it. The ticket keeps its status,
    /// lane, and place in the list — the board keeps the work item, the note holds the
    /// prose.
    ///
    /// A ticket that already links to a note is refused rather than re-promoted: that
    /// note is the human's own prose now. The refusal comes back as the outcome, not as
    /// the app-wide banner — the action belongs to the ticket sheet, so that is where
    /// the human must see why nothing was written.
    func promoteToNote(_ id: String, into bikeshed: Bikeshed) -> PromotionOutcome {
        do {
            let result = try TicketToNote(store: store, bikeshed: bikeshed).promote(id)
            reload()
            return .promoted(path: result.value.path)
        } catch {
            if let recovery = error as? RecoveryFailure { recoveryNote = recovery.note }
            return .refused(message: Self.message(for: error))
        }
    }

    // MARK: - Dismissals

    func dismissRecoveryNote() { recoveryNote = nil }
    func dismissError() { lastError = nil }

    // MARK: - Internals

    /// The offer runs out after `CompletionUndo.window` seconds — the toast is brief
    /// by design. The clock is the model's, not the toast's, so an offer raised while
    /// no face is drawing it still expires. A newer completion replaces the offer,
    /// and the older timer (which would clear it early) stands down.
    private func scheduleUndoExpiry() {
        let offer = undoOffer
        DispatchQueue.main.asyncAfter(deadline: .now() + CompletionUndo.window) { [weak self] in
            guard let self, self.undoOffer == offer else { return }
            self.undoOffer = nil
        }
    }

    /// Runs a throwing store operation, surfaces any recovery note, and reloads. On
    /// failure, records the error (and, if the failure also recovered a corrupt store,
    /// the note) so nothing is silently lost. Returns the operation's value, or `nil`
    /// when it failed.
    @discardableResult
    private func run<T: Sendable>(_ op: () throws -> OperationResult<T>) -> T? {
        do {
            let result = try op()
            if let note = result.note { recoveryNote = note }
            reload()
            return result.value
        } catch {
            apply(error: error)
            return nil
        }
    }

    private func apply(error: any Error) {
        if let recovery = error as? RecoveryFailure {
            recoveryNote = recovery.note
        }
        lastError = Self.message(for: error)
    }

    private static func message(for error: any Error) -> String {
        if let tandem = error as? any TandemError { return tandem.message }
        return error.localizedDescription
    }
}

/// What a promotion did: the note it made, or why it made none. The sheet shows the
/// message where the button was — a refusal is an answer to "Promote to note", not an
/// app-wide alarm.
enum PromotionOutcome {
    /// The note was created — at this bikeshed-root-relative path, which is now the
    /// ticket's `link`.
    case promoted(path: String)
    /// Nothing was written. The message says why, in the human's words.
    case refused(message: String)
}

/// What posting a comment did: it landed in the ticket's log — and here is the
/// record that was written — or nothing was written, and this is why.
enum CommentPostOutcome {
    /// The comment was appended. The record is the one the write made: the sheet
    /// draws it with the log it has, and the human's comment is in the list at
    /// once.
    case posted(Comment)
    /// Nothing was written. The message says why, in the core's words.
    case refused(message: String)
}
