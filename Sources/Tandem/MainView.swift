import SwiftUI
import TandemCore

/// The main window in reach B — subtle Ligne Claire (ticket 01 verdict).
///
///   Header    — the "Tandem" wordmark and the Todos/Bikeshed tab control,
///               closed by a hairline (identical on both tabs)
///   Todos     — a centered 540 pt column: flat parchment quick-add
///               (Return commits, ⌘N focuses it), flat segmented Open/Done/All
///               with live counts (⌘1/⌘2/⌘3) on a row with "+ Section" and
///               "Clear done" (disabled when nothing is done), and the
///               stored-order list grouped by section. Only named sections draw
///               a header — the ungrouped todos are just the top of the list.
///               Double-click a name to rename; its ⋯ menu moves the section up
///               or down, or deletes it, which ungroups its todos rather than
///               deleting them.
///               Rows: toggling checkbox (check completes, uncheck reopens);
///               title edits in place on double-click; hover/right-click delete
///               (all GUI-only)
///   Bikeshed  — the notes face (BikeFace.swift): tree sidebar, the WKWebView
///               editor, the backlinks dock
///
/// The theme lives in the chrome: flat paper surfaces, uniform 1pt hairlines,
/// line-drawn controls, illustration only in the empty state (ticket 01 handoff).

struct MainView: View {
    @Environment(TandemModel.self) private var model
    @Environment(BikeshedModel.self) private var bikeModel
    @Environment(\.openWindow) private var openWindow
    @Environment(\.colorScheme) private var colorScheme

    @State private var quickAdd = ""
    @State private var filter: Filter = .open
    @State private var tab: MainTab = .todos
    @FocusState private var quickAddFocused: Bool

    // Section chrome: the inline "name a new section" field, and the inline rename.
    @State private var addingSection = false
    @State private var sectionDraft = ""
    @State private var renamingSection: String?
    @State private var renameDraft = ""
    @FocusState private var sectionFocused: Bool

    private var palette: Palette { Palette.forScheme(colorScheme) }

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            if tab == .todos {
                todosFace
                    .frame(maxWidth: 540)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else {
                BikeFaceView(model: bikeModel)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(palette.paper)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .frame(minWidth: 760, minHeight: 560)
        .modifier(TandemShortcuts(filter: $filter, focusQuickAdd: $quickAddFocused))
        .onAppear {
            // Let the menu bar item's "Open Tandem" focus (or reopen) this window.
            WindowManager.shared.openMain = { openWindow(id: "main") }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 10) {
            Text("Tandem")
                .font(.custom("AvenirNext-DemiBold", size: 15))
                .tracking(0.3)
                .foregroundStyle(palette.ink)
            BikeTabControl(tab: $tab, palette: palette)
            Spacer()
        }
        .padding(.horizontal, Space.gutter)
        .padding(.top, 16)
        .padding(.bottom, 12)
    }

    // MARK: - Todos face (the centered 540 pt column)

    private var todosFace: some View {
        VStack(spacing: 0) {
            LigneQuickAdd(text: $quickAdd, palette: palette,
                          onCommit: commitAdd,
                          focus: $quickAddFocused)
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.section)
            HStack(spacing: 12) {
                LigneFilter(selection: $filter,
                            counts: (open: model.openTodos.count,
                                     done: model.doneTodos.count),
                            palette: palette)
                Spacer(minLength: 8)
                if addingSection {
                    LigneSectionField(text: $sectionDraft, palette: palette,
                                      focus: $sectionFocused,
                                      onCommit: commitSection, onCancel: cancelSection)
                } else {
                    TextButton(title: "+ Section", palette: palette) { beginAddSection() }
                }
                TextButton(title: "Clear done", palette: palette,
                           enabled: !model.doneTodos.isEmpty) {
                    withAnimation(Motion.remove) { model.clearDone() }
                }
            }
            .padding(.horizontal, Space.gutter)
            .padding(.top, Space.section)
            recoveryBanner
            errorBanner
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, Space.section)
            listArea
        }
    }

    // MARK: - Quick add

    private func commitAdd() {
        let title = quickAdd.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        withAnimation(Motion.insert) { model.add(title) }
        quickAdd = ""
        quickAddFocused = true  // keep focus for rapid entry
    }

    // MARK: - List

    private var visibleTodos: [Todo] {
        switch filter {
        case .open: return model.openTodos
        case .done: return model.doneTodos
        case .all: return model.todos
        }
    }

    /// The list grouped for display. With no sections declared this is a single
    /// unnamed group (the flat list, unchanged); otherwise the default area comes
    /// first, then the declared sections, then any section that only appears on a
    /// todo (a hand edit) so nothing is hidden. Order within a group is the stored
    /// order.
    private var groups: [TodoGroup] {
        let visible = visibleTodos
        guard !model.sections.isEmpty else {
            return [TodoGroup(key: nil, todos: visible)]
        }
        var keys: [String?] = [nil]
        for name in model.sections where !keys.contains(name) { keys.append(name) }
        for todo in visible where todo.section != nil && !keys.contains(todo.section) {
            keys.append(todo.section)
        }
        return keys.map { key in
            TodoGroup(key: key, todos: visible.filter { $0.section == key })
        }
    }

    /// Sections are shown once any exist; the flat look is the no-section case.
    private var showsSections: Bool { !model.sections.isEmpty }

    private var listArea: some View {
        Group {
            if visibleTodos.isEmpty && !showsSections {
                switch filter {
                case .open:
                    LigneEmptyState(palette: palette, title: "All clear",
                                    subtitle: "Nothing open — add something above.", hero: true)
                case .done:
                    LigneEmptyState(palette: palette, title: "Nothing done yet",
                                    subtitle: "Completed todos rest here.", hero: false)
                case .all:
                    LigneEmptyState(palette: palette, title: "The list is empty",
                                    subtitle: "Add your first todo above.", hero: false)
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(groups) { group in
                            let key = group.key
                            // The default area gets no header — it is just the top of the
                            // list, not a section. Only named groups draw one.
                            if let key, showsSections {
                                LigneSectionHeader(
                                    title: key,
                                    count: group.todos.count,
                                    palette: palette,
                                    isRenaming: renamingSection == key,
                                    draft: $renameDraft,
                                    onBeginRename: { beginRename(key) },
                                    onCommitRename: { commitRename(key) },
                                    onDelete: {
                                        withAnimation(Motion.remove) { model.deleteSection(key) }
                                    },
                                    canMoveUp: canMove(key, by: -1),
                                    canMoveDown: canMove(key, by: 1),
                                    onMoveUp: { moveSection(key, by: -1) },
                                    onMoveDown: { moveSection(key, by: 1) },
                                    onDropIntoSection: { moveTodo($0, toSection: key) })
                            }
                            ForEach(group.todos) { todo in
                                LigneRow(todo: todo, palette: palette,
                                         onComplete: { withAnimation(Motion.toggle) { model.complete(todo.id.uuidString) } },
                                         onReopen: { withAnimation(Motion.toggle) { model.reopen(todo.id.uuidString) } },
                                         onEdit: { model.update(todo.id.uuidString, title: $0) },
                                         onDelete: { withAnimation(Motion.remove) { model.delete(todo.id.uuidString) } },
                                         sections: model.sections,
                                         onMoveToSection: { moveTodo(todo.id.uuidString, toSection: $0) },
                                         onDropTodo: { draggedID, edge in
                                             drop(draggedID, onto: todo, edge: edge)
                                         })
                                    .transition(.opacity)
                                if todo.id != group.todos.last?.id {
                                    Rectangle()
                                        .fill(palette.sand)
                                        .frame(height: Line.hairline)
                                        .padding(.leading, Space.gutter + 24 + 10)
                                }
                            }
                        }
                    }
                    .padding(.vertical, 6)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - Drag and drop

    /// Drop of `draggedID` onto the row showing `anchor`, on the given half. The
    /// arithmetic lives in the core (`TodoOrdering.dropIndex`) so it is provable against
    /// the real `move` — see `OrderingTests`.
    private func drop(_ draggedID: String, onto anchor: Todo, edge: DropEdge) {
        guard let dragged = UUID(uuidString: draggedID),
              let index = TodoOrdering.dropIndex(in: model.todos,
                                                 moving: dragged,
                                                 relativeTo: anchor.id,
                                                 after: edge == .after)
        else { return }
        withAnimation(Motion.toggle) {
            model.move(draggedID, toSection: anchor.section, at: index)
        }
    }

    /// Files a todo at the top of `section` (`nil` = the default area). Used by the
    /// header drop and by the row's "Move to" menu — the latter being the way out of
    /// a section that works at any scroll position.
    private func moveTodo(_ id: String, toSection section: String?) {
        withAnimation(Motion.toggle) { model.move(id, toSection: section, at: 0) }
    }

    // MARK: - Section actions

    /// Whether `key` can move one place towards the start (`by == -1`) or the end (+1).
    private func canMove(_ key: String, by delta: Int) -> Bool {
        guard let i = model.sections.firstIndex(of: key) else { return false }
        return model.sections.indices.contains(i + delta)
    }

    private func moveSection(_ key: String, by delta: Int) {
        guard let i = model.sections.firstIndex(of: key) else { return }
        withAnimation(Motion.toggle) { model.moveSection(key, to: i + delta) }
    }

    private func beginAddSection() {
        addingSection = true
        sectionDraft = ""
    }

    private func commitSection() {
        addingSection = false
        let name = sectionDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        sectionDraft = ""
        guard !name.isEmpty else { return }
        model.addSection(name)
    }

    private func cancelSection() {
        addingSection = false
        sectionDraft = ""
    }

    private func beginRename(_ key: String) {
        renameDraft = key
        renamingSection = key
    }

    /// Return or clicking away: keep a non-empty, changed name; otherwise leave the
    /// section as it was.
    private func commitRename(_ key: String) {
        guard renamingSection == key else { return }
        renamingSection = nil
        let name = renameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name != key else { return }
        model.renameSection(key, to: name)
    }

    // MARK: - Banners (restyled per reach B; no illustration in the banner)

    @ViewBuilder
    private var recoveryBanner: some View {
        if let note = model.recoveryNote {
            let path = backupPath(from: note)
            LigneBanner(palette: palette,
                        systemImage: "exclamationmark.triangle",
                        title: "The list was unreadable, so it was backed up and started fresh.",
                        detail: path.map { "Backup: \($0)" } ?? note,
                        accentBar: true,
                        restorePath: path,
                        onDismiss: { model.dismissRecoveryNote() })
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.section)
        }
    }

    @ViewBuilder
    private var errorBanner: some View {
        if let error = model.lastError {
            LigneBanner(palette: palette,
                        systemImage: "xmark.circle.fill",
                        title: error,
                        onDismiss: { model.dismissError() })
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.section)
        }
    }
}

/// Settled default UX (map, 2026-10-11): ⌘1/⌘2/⌘3 switch the Filter,
/// ⌘N focuses quick-add. Registered once, invisible.
struct TandemShortcuts: ViewModifier {
    @Binding var filter: Filter
    var focusQuickAdd: FocusState<Bool>.Binding

    func body(content: Content) -> some View {
        content.background {
            VStack(spacing: 0) {
                Button("") { setFilter(.open) }.keyboardShortcut("1", modifiers: .command)
                Button("") { setFilter(.done) }.keyboardShortcut("2", modifiers: .command)
                Button("") { setFilter(.all) }.keyboardShortcut("3", modifiers: .command)
                Button("") { focusQuickAdd.wrappedValue = true }.keyboardShortcut("n", modifiers: .command)
            }
            .buttonStyle(.plain)
            .hidden()
        }
    }

    private func setFilter(_ f: Filter) {
        withAnimation(Motion.toggle) { filter = f }
    }
}
