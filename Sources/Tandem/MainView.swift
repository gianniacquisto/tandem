import SwiftUI
import TandemCore

/// The main window in reach B — subtle Ligne Claire (ticket 01 verdict).
///
///   Header    — the "Tandem" wordmark and the Todos/Board/Bikeshed tab control,
///               closed by a hairline (identical on every tab)
///   Todos     — a centered 540 pt column: flat parchment quick-add
///               (Return commits, ⌘N focuses it), flat segmented Open/Done/All/
///               Archived with live counts (⌘1–⌘4) on a row with "+ Cycle lane" and
///               "Archive done" (disabled when nothing is done), and the
///               stored-order list grouped by lane. Only named lanes draw
///               a header — the ungrouped todos are just the top of the list.
///               A search field (⌘F) narrows the drawn rows by title and description,
///               across every lane; Escape clears it (or dismisses a banner).
///               Double-click a name to rename; its ⋯ menu moves the lane up
///               or down, or deletes it, which ungroups its todos rather than
///               deleting them.
///               Rows: toggling checkbox (check completes, uncheck reopens);
///               double-click opens the ticket sheet (title, status, lane, body);
///               rename is the menu's item; hover/right-click delete
///               (all GUI-only). An archived ticket carries a quiet "archived"
///               tag wherever a view includes it, and its menu brings it back.
///   Board     — Status columns, lane rows (BoardView.swift): the same tickets
///               laid out by where they are in the workflow
///   Inbox     — the tickets agents assigned to `human`: the escalation lane, drawn
///               with the list's own rows (the description sheet holds the question).
///               A dot on the tab says one is waiting, from any other face
///   Bikeshed  — the notes face (BikeFace.swift): tree sidebar, the WKWebView
///               editor, the backlinks dock
///   ⌘K        — the quick-open palette over the window: one ranked list of
///               the notes and the tickets (a note opens in the editor, a
///               ticket in its sheet; ↑↓ move, ↩ opens, esc dismisses)
///
/// The theme lives in the chrome: flat paper surfaces, uniform 1pt hairlines,
/// line-drawn controls, illustration only in the empty state (ticket 01 handoff).

struct MainView: View {
    @Environment(TandemModel.self) private var model
    @Environment(BikeshedModel.self) private var bikeModel
    @Environment(SetupPresenter.self) private var setup
    @Environment(SettingsPresenter.self) private var settings
    @Environment(\.openWindow) private var openWindow
    @Environment(\.colorScheme) private var colorScheme

    @State private var quickAdd = ""
    @State private var scope: ListScope = .open
    @State private var search = ""
    @State private var tab: MainTab = .todos
    @FocusState private var quickAddFocused: Bool
    @FocusState private var searchFocused: Bool

    /// The first-run check runs once per window, on the first appear.
    @State private var checkedSetup = false

    // Lane chrome: the inline "name a new lane" field, and the inline rename.
    @State private var addingLane = false
    @State private var laneDraft = ""
    @State private var renamingLane: String?
    @State private var renameDraft = ""
    @FocusState private var laneFocused: Bool

    // The todo being linked to a note, and the picker's filter text.
    @State private var linkPickerFor: Ticket?
    @State private var linkFilter = ""

    // The ticket open in the ticket sheet, if any.
    @State private var ticketFor: Ticket?

    // The quick-open palette (⌘K): the one keyboard path to a known note or ticket.
    @State private var quickOpenPresented = false

    private var palette: Palette { Palette.forScheme(colorScheme) }

    var body: some View {
        @Bindable var setup = setup
        @Bindable var settings = settings
        return VStack(spacing: 0) {
            header
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            if tab == .todos {
                todosFace
                    .frame(maxWidth: 540)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else if tab == .board {
                BoardFace(model: model,
                          search: $search,
                          palette: palette,
                          focus: $searchFocused,
                          onOpenLink: openLinkedNote,
                          onPromote: promote)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .modifier(TodosShortcuts(search: $search,
                                             focusSearch: $searchFocused,
                                             hasBanner: false,
                                             dismissBanners: {}))
            } else if tab == .inbox {
                inboxFace
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
        .modifier(TandemShortcuts(scope: $scope,
                                  focusQuickAdd: $quickAddFocused,
                                  toggleQuickOpen: toggleQuickOpen))
        .overlay {
            if quickOpenPresented {
                quickOpenOverlay
            }
        }
        .sheet(item: $linkPickerFor) { todo in
            linkSheet(todo)
        }
        .sheet(item: $ticketFor) { todo in
            TicketSheet(
                todo: todo,
                palette: palette,
                lanes: model.lanes,
                comments: model.comments(for: todo.id.uuidString),
                onSave: { edits in
                    model.saveTicket(todo.id.uuidString, edits: edits)
                    ticketFor = nil
                },
                onCancel: { ticketFor = nil },
                onOpenLink: openLinkedNote,
                onPromote: { promote(todo) })
        }
        .sheet(isPresented: $setup.isPresented) {
            SetupWizardView(
                bikeshedPath: bikeModel.bikeshed.directory.path,
                onChooseBikeshed: { chooseBikeshed() },
                onClose: { setup.isPresented = false })
        }
        .sheet(isPresented: $settings.isPresented) {
            SettingsView(model: bikeModel)
        }
        .onAppear {
            // Let the menu bar item's "Open Tandem" focus (or reopen) this window.
            WindowManager.shared.openMain = { openWindow(id: "main") }
            showSetupIfNeeded()
        }
        // The undo toast: one click back from a mis-clicked completion. It floats
        // over whichever face is showing, because the completion can happen on any
        // of them (the list, the board, the inbox).
        .overlay(alignment: .bottom) {
            Group {
                if let offer = model.undoOffer {
                    LigneToast(offer: offer, palette: palette) {
                        withAnimation(Motion.toggle) { model.undoCompletion() }
                    }
                    .padding(.bottom, Space.lane)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(Motion.toggle, value: model.undoOffer)
        }
    }

    /// First launch only (the check runs once per window): raise the setup
    /// wizard when the flag is unset. The flag is written when the wizard is
    /// finished or skipped, so a crash mid-wizard re-shows it next launch.
    private func showSetupIfNeeded() {
        guard !checkedSetup else { return }
        checkedSetup = true
        if !TandemSettings.setupComplete() { setup.isPresented = true }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 10) {
            Text("Tandem")
                .font(.custom("AvenirNext-DemiBold", size: 15))
                .tracking(0.3)
                .foregroundStyle(palette.ink)
            BikeTabControl(tab: $tab, palette: palette,
                       inboxCount: model.inboxTodos.count)
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
                          focus: $quickAddFocused,
                          lane: model.defaultLane,
                          lanes: model.lanes,
                          onPickLane: { model.setDefaultLane($0) })
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.lane)
            HStack(spacing: 12) {
                LigneFilter(selection: $scope,
                            counts: (open: model.openTodos.count,
                                     done: model.doneTodos.count,
                                     archived: model.archivedTodos.count),
                            palette: palette)
                Spacer(minLength: 8)
                if addingLane {
                    LigneLaneField(text: $laneDraft, palette: palette,
                                      focus: $laneFocused,
                                      onCommit: commitLane, onCancel: cancelLane)
                } else {
                    TextButton(title: "+ Cycle lane", palette: palette) { beginAddLane() }
                }
                TextButton(title: "Archive done", palette: palette,
                           enabled: !model.doneTodos.isEmpty) {
                    withAnimation(Motion.remove) { model.archiveDone() }
                }
            }
            .padding(.horizontal, Space.gutter)
            .padding(.top, Space.lane)
            // View-only search: it narrows what is drawn (case- and diacritic-
            // insensitive, over title and description, across every lane and the
            // current Open/Done/All/Archived scope). It never re-sorts or mutates
            // the store.
            LigneSearchField(text: $search, palette: palette, focus: $searchFocused)
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.lane)
            recoveryBanner
            errorBanner
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, Space.lane)
            listArea
        }
        .modifier(TodosShortcuts(
            search: $search,
            focusSearch: $searchFocused,
            hasBanner: model.recoveryNote != nil || model.lastError != nil,
            dismissBanners: {
                model.dismissRecoveryNote()
                model.dismissError()
            }))
    }

    // MARK: - Quick add

    private func commitAdd() {
        let title = quickAdd.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        withAnimation(Motion.insert) { model.add(title, lane: model.defaultLane) }
        // A new ticket is never archived, so in the archive it would vanish the moment it
        // was created: go where it landed.
        if scope == .archived { scope = .open }
        quickAdd = ""
        quickAddFocused = true  // keep focus for rapid entry
    }

    // MARK: - List

    /// The tickets in view: the shipped filter, narrowed by the search field — one
    /// `TicketQuery` for both, the same predicate an agent's `list` runs. The search
    /// matches the title *and the description*, and it never re-sorts or mutates the
    /// store.
    private var visibleTodos: [Ticket] {
        var query = scope.query
        query.text = search
        return query.apply(to: model.todos)
    }

    /// Whether the search field has anything in it. The matching itself — case- and
    /// diacritic-insensitive, over title and description — belongs to `TicketQuery`.
    private var queryActive: Bool {
        !search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The list grouped for display: the default area first, then the declared lanes, then
    /// any lane that only appears on a todo (a hand edit) so nothing is hidden. The key
    /// order is the board's (`TicketBoard.orderedKeys`) — both faces group a lane the same
    /// way, including the no-lanes-declared case; the order *within* a group stays the
    /// stored order, which is what a list shows.
    private var groups: [TodoGroup] {
        let visible = visibleTodos
        return TicketBoard.orderedKeys(declared: model.lanes, todos: visible).map { key in
            TodoGroup(key: key, todos: visible.filter { $0.lane == key })
        }
    }

    /// Lanes are shown once any exist; the flat look is the no-lane case.
    private var showsLanes: Bool { !model.lanes.isEmpty }

    private var listArea: some View {
        Group {
            if queryActive && visibleTodos.isEmpty {
                // A search that matched nothing: a quiet empty state, not a wall
                // of lane headers at 0. Distinct from the true-empty states
                // below, which are about the store itself being empty.
                LigneEmptyState(palette: palette, title: "No matches",
                                subtitle: "No todo title matches your search.",
                                hero: false)
            } else if visibleTodos.isEmpty && !showsLanes {
                switch scope {
                case .open:
                    LigneEmptyState(palette: palette, title: "All clear",
                                    subtitle: "Nothing open — add something above.", hero: true)
                case .done:
                    LigneEmptyState(palette: palette, title: "Nothing done yet",
                                    subtitle: "Completed todos rest here.", hero: false)
                case .all:
                    LigneEmptyState(palette: palette, title: "The list is empty",
                                    subtitle: "Add your first todo above.", hero: false)
                case .archived:
                    LigneEmptyState(palette: palette, title: "The archive is empty",
                                    subtitle: "Archived todos rest here, out of the way.",
                                    hero: false)
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(groups) { group in
                            let key = group.key
                            // The default area gets no header — it is just the top of the
                            // list, not a lane. Only named groups draw one.
                            if let key, showsLanes {
                                LigneLaneHeader(
                                    title: key,
                                    count: group.todos.count,
                                    palette: palette,
                                    isRenaming: renamingLane == key,
                                    draft: $renameDraft,
                                    onBeginRename: { beginRename(key) },
                                    onCommitRename: { commitRename(key) },
                                    onDelete: {
                                        withAnimation(Motion.remove) { model.deleteLane(key) }
                                    },
                                    canMoveUp: canMove(key, by: -1),
                                    canMoveDown: canMove(key, by: 1),
                                    onMoveUp: { moveLane(key, by: -1) },
                                    onMoveDown: { moveLane(key, by: 1) },
                                    onDropIntoLane: { moveTodo($0, toLane: key) })
                            }
                            ForEach(group.todos) { todo in
                                ticketRow(todo)
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

    // MARK: - One row

    /// One ticket row, with every handler the list and the inbox share. The faces differ in
    /// what they show and how they order it, never in what a ticket can do — so the row is
    /// built once, here.
    private func ticketRow(_ todo: Ticket) -> some View {
        LigneRow(todo: todo, palette: palette,
                 onComplete: { withAnimation(Motion.toggle) { model.complete(todo.id.uuidString) } },
                 onReopen: { withAnimation(Motion.toggle) { model.reopen(todo.id.uuidString) } },
                 onEdit: { model.update(todo.id.uuidString, title: $0) },
                 onDelete: { withAnimation(Motion.remove) { model.delete(todo.id.uuidString) } },
                 onOpenLink: { openLinkedNote($0) },
                 lanes: model.lanes,
                 onMoveToLane: { moveTodo(todo.id.uuidString, toLane: $0) },
                 onSetLink: { beginLink(todo) },
                 onClearLink: { model.setLink(todo.id.uuidString, to: nil) },
                 onUnarchive: { withAnimation(Motion.toggle) {
                     model.setArchived(todo.id.uuidString, false)
                 } },
                 onOpenTicket: { ticketFor = todo },
                 onDropTodo: { draggedID, edge in
                     drop(draggedID, onto: todo, edge: edge)
                 })
            .transition(.opacity)
    }

    // MARK: - Inbox face

    /// The human's inbox: the tickets an agent assigned to `human` to ask for a decision.
    /// A query (`TicketQuery.inbox`), not a second store — the row is the list's row, and the
    /// question itself is the ticket's description, read in the description sheet.
    private var inboxFace: some View {
        let inbox = model.inboxTodos
        return VStack(spacing: 0) {
            HStack(spacing: 6) {
                Text("For you")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(palette.ink)
                Text("\(inbox.count)")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, Space.gutter)
            .padding(.top, Space.lane)
            .padding(.bottom, Space.lane)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)

            if inbox.isEmpty {
                LigneEmptyState(
                    palette: palette,
                    title: "Nothing for you",
                    subtitle: "When an agent needs a decision, it assigns a ticket to human.",
                    hero: false)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(inbox) { todo in
                            ticketRow(todo)
                            if todo.id != inbox.last?.id {
                                Rectangle()
                                    .fill(palette.sand)
                                    .frame(height: Line.hairline)
                                    .padding(.leading, Space.gutter + 24 + 10)
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
    /// arithmetic lives in the core (`TicketOrdering.dropIndex`) so it is provable against
    /// the real `move` — see `OrderingTests`.
    private func drop(_ draggedID: String, onto anchor: Ticket, edge: DropEdge) {
        guard let dragged = UUID(uuidString: draggedID),
              let index = TicketOrdering.dropIndex(in: model.todos,
                                                 moving: dragged,
                                                 relativeTo: anchor.id,
                                                 after: edge == .after)
        else { return }
        withAnimation(Motion.toggle) {
            model.move(draggedID, toLane: anchor.lane, at: index)
        }
    }

    /// Files a todo at the top of `lane` (`nil` = the default area). Used by the
    /// header drop and by the row's "Move to" menu — the latter being the way out of
    /// a lane that works at any scroll position.
    private func moveTodo(_ id: String, toLane lane: String?) {
        withAnimation(Motion.toggle) { model.move(id, toLane: lane, at: 0) }
    }

    // MARK: - Note links (GUI-only, ADR 0010)

    /// Opens the todo's linked note: switch to the Bikeshed tab and load it.
    private func openLinkedNote(_ path: String) {
        tab = .bikeshed
        bikeModel.open(path)
    }

    /// Promotes a ticket to a bikeshed note, in the bikeshed this window shows. The
    /// outcome travels back to the sheet, which says what happened.
    private func promote(_ todo: Ticket) -> PromotionOutcome {
        model.promoteToNote(todo.id.uuidString, into: bikeModel.bikeshed)
    }

    /// Opens the note picker for `todo`.
    private func beginLink(_ todo: Ticket) {
        linkFilter = ""
        linkPickerFor = todo
    }

    /// The note picker: a filterable list of the bikeshed's note paths. Picking one
    /// links the todo and dismisses; the picker never reads note content (the core
    /// owns it). Styled like the rename sheet (monospaced field, parchment, hairline).
    private func linkSheet(_ todo: Ticket) -> some View {
        let paths = bikeModel.notePaths()
        let q = linkFilter.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let shown = q.isEmpty ? paths : paths.filter { $0.lowercased().contains(q) }
        return VStack(spacing: 14) {
            Text("Link to note")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
            Text("Pick a note in the bikeshed.")
                .font(.system(size: 11.5))
                .foregroundStyle(palette.inkDim)
            TextField("Filter", text: $linkFilter)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5, design: .monospaced))
                .foregroundStyle(palette.ink)
                .padding(.horizontal, 11)
                .frame(height: 28)
                .frame(maxWidth: .infinity)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if shown.isEmpty {
                        Text(paths.isEmpty ? "No notes yet." : "No notes match.")
                            .font(.system(size: 12))
                            .foregroundStyle(palette.inkDim)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 6)
                    } else {
                        ForEach(shown, id: \.self) { path in
                            linkRow(path, todo: todo)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
            .frame(maxHeight: 260)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))
            HStack {
                Button("Cancel") { linkPickerFor = nil }
                    .keyboardShortcut(.cancelAction)
                Spacer()
            }
        }
        .padding(20)
        .frame(width: 420)
    }

    /// One note in the picker; the todo's current link is checkmarked.
    private func linkRow(_ path: String, todo: Ticket) -> some View {
        Button {
            model.setLink(todo.id.uuidString, to: path)
            linkPickerFor = nil
        } label: {
            HStack(spacing: 6) {
                if todo.link == path {
                    Image(systemName: "checkmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.emberDeep)
                }
                Text(path)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(palette.ink)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Quick open (⌘K)

    /// The quick-open palette over the window: a wash that dismisses on a click, and
    /// the ranked list itself. It is an overlay, not a sheet, so opening a ticket from
    /// it can still raise the ticket sheet.
    private var quickOpenOverlay: some View {
        ZStack(alignment: .top) {
            Rectangle()
                .fill(palette.paper.opacity(0.9))
                .onTapGesture { quickOpenPresented = false }
            QuickOpenPalette(
                items: quickOpenItems,
                palette: palette,
                onOpen: openQuickOpenItem,
                onDismiss: { quickOpenPresented = false })
                .padding(.top, 72)
        }
    }

    /// The candidates: the bikeshed's notes and the store's tickets. The core does the
    /// matching and the ranking; this only gathers what there is to rank.
    private var quickOpenItems: [QuickOpen.Item] {
        bikeModel.notePaths().map { QuickOpen.Item.note(path: $0) }
            + model.todos.map { QuickOpen.Item.ticket($0) }
    }

    /// Opens a quick-open choice: a note in the editor, a ticket in its sheet.
    private func openQuickOpenItem(_ item: QuickOpen.Item) {
        quickOpenPresented = false
        switch item {
        case .note(let path):
            // The note opens in the Bikeshed tab, the way a linked note does.
            tab = .bikeshed
            bikeModel.open(path)
        case .ticket(let todo):
            ticketFor = todo
        }
    }

    /// ⌘K: the palette opens, or closes if it is already up.
    private func toggleQuickOpen() {
        withAnimation(Motion.toggle) { quickOpenPresented.toggle() }
    }

    // MARK: - Lane actions

    /// Whether `key` can move one place towards the start (`by == -1`) or the end (+1).
    private func canMove(_ key: String, by delta: Int) -> Bool {
        guard let i = model.lanes.firstIndex(of: key) else { return false }
        return model.lanes.indices.contains(i + delta)
    }

    private func moveLane(_ key: String, by delta: Int) {
        guard let i = model.lanes.firstIndex(of: key) else { return }
        withAnimation(Motion.toggle) { model.moveLane(key, to: i + delta) }
    }

    private func beginAddLane() {
        addingLane = true
        laneDraft = ""
    }

    private func commitLane() {
        addingLane = false
        let name = laneDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        laneDraft = ""
        guard !name.isEmpty else { return }
        model.addLane(name)
    }

    private func cancelLane() {
        addingLane = false
        laneDraft = ""
    }

    private func beginRename(_ key: String) {
        renameDraft = key
        renamingLane = key
    }

    /// Return or clicking away: keep a non-empty, changed name; otherwise leave the
    /// lane as it was.
    private func commitRename(_ key: String) {
        guard renamingLane == key else { return }
        renamingLane = nil
        let name = renameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name != key else { return }
        model.renameLane(key, to: name)
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
                .padding(.top, Space.lane)
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
                .padding(.top, Space.lane)
        }
    }
}

/// Settled default UX (map, 2026-10-11): ⌘1/⌘2/⌘3 switch the list's Open/Done/All,
/// ⌘4 shows the archive, ⌘N focuses quick-add, ⌘K opens the quick-open palette.
/// Registered once, invisible.
struct TandemShortcuts: ViewModifier {
    @Binding var scope: ListScope
    var focusQuickAdd: FocusState<Bool>.Binding
    /// Opens (or closes) the quick-open palette — the one keyboard path to a known
    /// note or ticket, from any tab.
    var toggleQuickOpen: () -> Void

    func body(content: Content) -> some View {
        content.background {
            VStack(spacing: 0) {
                Button("") { setScope(.open) }.keyboardShortcut("1", modifiers: .command)
                Button("") { setScope(.done) }.keyboardShortcut("2", modifiers: .command)
                Button("") { setScope(.all) }.keyboardShortcut("3", modifiers: .command)
                Button("") { setScope(.archived) }.keyboardShortcut("4", modifiers: .command)
                Button("") { focusQuickAdd.wrappedValue = true }.keyboardShortcut("n", modifiers: .command)
                Button("") { toggleQuickOpen() }.keyboardShortcut("k", modifiers: .command)
            }
            .buttonStyle(.plain)
            .hidden()
        }
    }

    private func setScope(_ s: ListScope) {
        withAnimation(Motion.toggle) { scope = s }
    }
}

/// Todos-face shortcuts, registered the same invisible-button way as
/// `TandemShortcuts` (which keeps ⌘1/⌘2/⌘3 and ⌘N — this never steals them):
///
///   ⌘F      focuses the search field
///   Escape  clears a focused, non-empty search; otherwise dismisses a banner
///
/// Scoped to the Todos face so its Escape never contends with the Bikeshed's own
/// Escape handler. Focus is the system's single first responder, so focusing one
/// field (`⌘F`) automatically resigns the other (`⌘N` quick-add) and vice versa.
struct TodosShortcuts: ViewModifier {
    @Binding var search: String
    var focusSearch: FocusState<Bool>.Binding
    /// Whether a banner is actually up — Escape is a no-op otherwise.
    var hasBanner: Bool
    var dismissBanners: () -> Void

    func body(content: Content) -> some View {
        content.background {
            VStack(spacing: 0) {
                Button("") { focusSearch.wrappedValue = true }
                    .keyboardShortcut("f", modifiers: .command)
                Button("") {
                    // Clear an active search first; only when there is nothing to
                    // clear does Escape fall through to dismissing a banner.
                    if focusSearch.wrappedValue && !search.isEmpty {
                        search = ""
                    } else if hasBanner {
                        dismissBanners()
                    }
                }
                .keyboardShortcut(.escape, modifiers: [])
            }
            .buttonStyle(.plain)
            .hidden()
        }
    }
}
