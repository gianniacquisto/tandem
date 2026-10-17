import Foundation
import Observation
import TandemCore

/// The single shared app model: a thin, observable adapter over `TandemCore.Store`
/// (ADR 0001). All todo semantics live in the store; this class only bridges them to
/// the UI.
///
/// One instance is shared by both faces — the SwiftUI main window (which tracks it via
/// Observation) and the AppKit menu bar (which reads it on every open) — so it is a
/// deliberate app-wide singleton: there is exactly one store, at `~/.tandem`.
///
/// Everything here runs on the main thread: mutations are driven either by a UI action
/// or by the FSEvents callback, which is scheduled on the main run loop.
@Observable
final class TandemModel {
    static let shared = TandemModel()

    private let store: Store
    private let watcher: StoreWatcher

    /// All todos in stored order (the order the human arranged — new todos land on top;
    /// never re-sorted here).
    private(set) var todos: [Todo] = []
    /// The declared section names, in render order. The default (ungrouped) area at the
    /// top of the list is implicit and unnamed, so it is not in here.
    private(set) var sections: [String] = []
    /// Set when a call triggered corrupt-store recovery; names the backup path. Sticky
    /// until the user dismisses it (a later successful reload does not clear it).
    private(set) var recoveryNote: String?
    /// The most recent operation error, surfaced to the user. Cleared on the next
    /// successful operation/reload.
    private(set) var lastError: String?

    var openTodos: [Todo] { todos.filter { !$0.done } }
    var doneTodos: [Todo] { todos.filter { $0.done } }

    private init(store: Store = Store()) {
        self.store = store
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
            todos = result.value
            sections = (try? store.sections().value) ?? []
            if let note = result.note { recoveryNote = note }
            lastError = nil
        } catch {
            apply(error: error)
        }
    }

    // MARK: - Operations (thin, non-throwing wrappers over the store)

    func add(_ rawTitle: String) {
        run { try self.store.add(rawTitle) }
    }

    /// Adds a todo on top of `section` (`nil` = the default area).
    func add(_ rawTitle: String, section: String?) {
        run { try self.store.add(rawTitle, section: section) }
    }

    // MARK: - Sections and order (GUI-only)

    func addSection(_ name: String) {
        run { try self.store.addSection(name) }
    }

    func renameSection(_ from: String, to: String) {
        run { try self.store.renameSection(from, to: to) }
    }

    /// Deleting a section moves its todos to the default area — it never deletes them.
    func deleteSection(_ name: String) {
        run { try self.store.deleteSection(name) }
    }

    /// Reorders: moves a todo to `index` (0-based) within `section` (`nil` = the default
    /// area), moving it between sections when needed.
    func move(_ id: String, toSection section: String?, at index: Int) {
        run { try self.store.move(id, toSection: section, at: index) }
    }

    /// Complete is idempotent at the store level, so completing an already-done todo
    /// is a no-op.
    func complete(_ id: String) {
        run { try self.store.complete(id) }
    }

    /// The inverse of `complete` — the checkbox on a done row reopens it. Also
    /// idempotent: reopening an already-open todo is a no-op.
    func reopen(_ id: String) {
        run { try self.store.reopen(id) }
    }

    /// Rewrites a todo's title (inline edit). GUI-only — no MCP tool; the empty-title
    /// and unknown-id errors surface verbatim.
    func update(_ id: String, title: String) {
        run { try self.store.update(id, title: title) }
    }

    func clearDone() {
        run { try self.store.clearDone() }
    }

    /// GUI-only in v1 — there is no delete tool in the MCP surface.
    func delete(_ id: String) {
        run { try self.store.delete(id) }
    }

    // MARK: - Dismissals

    func dismissRecoveryNote() { recoveryNote = nil }
    func dismissError() { lastError = nil }

    // MARK: - Internals

    /// Runs a throwing store operation, surfaces any recovery note, and reloads. On
    /// failure, records the error (and, if the failure also recovered a corrupt store,
    /// the note) so nothing is silently lost.
    private func run<T: Sendable>(_ op: () throws -> OperationResult<T>) {
        do {
            let result = try op()
            if let note = result.note { recoveryNote = note }
            reload()
        } catch {
            apply(error: error)
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
