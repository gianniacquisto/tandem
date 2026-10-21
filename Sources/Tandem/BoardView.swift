import SwiftUI
import TandemCore

// The board face: Status columns, cycle-lane rows.
//
// It is the third tab, beside the list rather than instead of it (2026-10-20): the list
// stays the place to capture ("Add a todo…"), name lanes, and read the flat running
// order; the board is where a ticket's *stage* is visible and changeable. Same store,
// same query, same drag arithmetic — the board draws what the list holds, cut the other
// way.
//
// The grid follows the register of the rest of the app: parchment tickets on paper, one
// hairline each, flat washes for a drop target, no shadows. Status and lane are the two
// axes and nothing else is added to a ticket but its title, its assignee, and the
// paperclip of a linked note. The description stays out of it — the body is map ticket 6.

extension Status {
    /// The column title ("in-progress" is a wire value, not a phrase).
    var displayName: String {
        switch self {
        case .backlog: return "Backlog"
        case .refined: return "Refined"
        case .ready: return "Ready"
        case .inProgress: return "In progress"
        case .inReview: return "In Review"
        case .done: return "Done"
        }
    }
}

/// What every cell and ticket on the board needs: the model it writes through, the theme,
/// the lanes a ticket can be filed into, and the parent's note opener.
struct BoardContext {
    var model: TandemModel
    var palette: Palette
    /// Every lane the board can file into — declared lanes plus any a ticket already names
    /// (the board draws a row for those, so its menu must offer them).
    var lanes: [String]
    var onOpenLink: (String) -> Void
    /// Opens the ticket sheet for a ticket — the face owns the sheet.
    var onOpenTicket: (Todo) -> Void
    /// Promotes a ticket to a bikeshed note — the parent owns the bikeshed.
    var onPromote: (Todo) -> PromotionOutcome
}

/// The board's outer frame: the search field (the same `TodoQuery` text the list uses),
/// "Archive done", then the grid.
struct BoardFace: View {
    var model: TandemModel
    @Binding var search: String
    var palette: Palette
    var focus: FocusState<Bool>.Binding
    /// Opens the ticket's linked note — the parent owns the tab switch.
    var onOpenLink: (String) -> Void
    /// Promotes a ticket to a bikeshed note — the parent owns the bikeshed.
    var onPromote: (Todo) -> PromotionOutcome

    /// The ticket open in the ticket sheet, if any.
    @State private var ticketFor: Todo?

    /// The tickets in view: archived ones are out (the board is about work in the way),
    /// and the search narrows by title and description. One `TodoQuery`, as ever.
    private var visible: [Todo] {
        var query = TodoQuery()
        query.text = search
        return query.apply(to: model.todos)
    }

    private var rows: [BoardLane] {
        TodoBoard.rows(from: visible, lanes: model.lanes)
    }

    private var searching: Bool {
        !search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The lanes a ticket can be moved into: the declared ones plus any dangling name
    /// already on a ticket, in the board's own row order.
    private var lanes: [String] {
        TodoBoard.orderedKeys(declared: model.lanes, todos: model.todos).compactMap { $0 }
    }

    private var context: BoardContext {
        BoardContext(model: model,
                     palette: palette,
                     lanes: lanes,
                     onOpenLink: onOpenLink,
                     onOpenTicket: { ticketFor = $0 },
                     onPromote: onPromote)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                LigneSearchField(text: $search, palette: palette, focus: focus)
                TextButton(title: "Archive done", palette: palette,
                           enabled: !model.doneTodos.isEmpty) {
                    withAnimation(Motion.remove) { model.archiveDone() }
                }
            }
            .padding(.horizontal, Space.gutter)
            .padding(.top, Space.lane)
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
                .padding(.top, Space.lane)

            if searching && visible.isEmpty {
                // A search with no hits says so instead of drawing empty columns.
                LigneEmptyState(palette: palette,
                                title: "No matches",
                                subtitle: "No ticket title or description matches your search.",
                                hero: false)
            } else if model.todos.isEmpty {
                // A store with nothing in it draws no grid: a row of empty cells reads as a
                // board that failed to load. Lanes without tickets still draw — they are
                // the targets that make the next drop possible.
                LigneEmptyState(palette: palette,
                                title: "Nothing on the board",
                                subtitle: "Add a ticket in Todos, or from the CLI.",
                                hero: true)
            } else {
                board
            }
        }
        .sheet(item: $ticketFor) { todo in
            TicketSheet(
                todo: todo,
                palette: palette,
                lanes: lanes,
                onSave: { edits in
                    model.saveTicket(todo.id.uuidString, edits: edits)
                    ticketFor = nil
                },
                onCancel: { ticketFor = nil },
                onOpenLink: onOpenLink,
                onPromote: { onPromote(todo) })
        }
    }

    private var board: some View {
        VStack(spacing: 0) {
            columnHeader
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Space.lane) {
                    ForEach(rows) { row in
                        laneRow(row)
                    }
                }
                .padding(.horizontal, Space.gutter)
                .padding(.vertical, Space.lane)
            }
        }
    }

    /// The six statuses in workflow order, each with how many tickets the whole board has
    /// in it. The header is fixed: it must stay above the scroll.
    private var columnHeader: some View {
        HStack(spacing: BoardMetrics.gap) {
            ForEach(Status.workflow, id: \.self) { status in
                HStack(spacing: 5) {
                    Text(status.displayName)
                        .font(.system(size: 11.5, weight: .semibold))
                        .foregroundStyle(palette.ink)
                    Text("\(count(of: status))")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(palette.inkDim)
                    Spacer(minLength: 0)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, Space.gutter)
        .padding(.vertical, 8)
    }

    private func count(of status: Status) -> Int {
        rows.reduce(0) { $0 + $1.tickets(status).count }
    }

    private func laneRow(_ row: BoardLane) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(row.key ?? "Uncategorized")
                    .font(.system(size: 12.5, weight: row.isDefault ? .regular : .semibold))
                    .foregroundStyle(row.isDefault ? palette.inkDim : palette.ink)
                Text("\(row.count)")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
            }
            HStack(alignment: .top, spacing: BoardMetrics.gap) {
                ForEach(Status.workflow, id: \.self) { status in
                    BoardCell(lane: row.key,
                              status: status,
                              tickets: row.tickets(status),
                              context: context)
                }
            }
        }
    }
}

// MARK: - One column of one lane

/// A cell is one lane × one status: its tickets, and the whole cell is a drop target — the
/// ticket lands in this lane at this stage. Dropping a ticket on another ticket reorders
/// inside the cell, with the same edge arithmetic the list uses.
private struct BoardCell: View {
    var lane: String?
    var status: Status
    var tickets: [Todo]
    var context: BoardContext

    @State private var targeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: BoardMetrics.ticketGap) {
            ForEach(tickets) { todo in
                BoardTicket(todo: todo, context: context)
            }
        }
        .frame(maxWidth: .infinity, minHeight: BoardMetrics.minCellHeight, alignment: .top)
        .padding(BoardMetrics.cellPadding)
        .background(RoundedRectangle(cornerRadius: Radius.control)
            .fill(targeted ? context.palette.parchment : Color.clear))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(targeted ? context.palette.accent : Color.clear, lineWidth: Line.hairline))
        .contentShape(Rectangle())
        .dropDestination(for: String.self) { items, _ in
            guard let id = items.first else { return false }
            // The end of the lane, unless a ticket inside the cell claims the drop first.
            withAnimation(Motion.toggle) {
                context.model.place(id, lane: lane, status: status)
            }
            return true
        } isTargeted: { targeted = $0 }
    }
}

// MARK: - A ticket on the board

/// The ticket's tile: its title, who is on it, and whether it is linked. The checkbox is
/// the one-click completion; the menu carries the stage and the lane when dragging is
/// inconvenient (a keyboard, a narrow window, a long scroll).
private struct BoardTicket: View {
    var todo: Todo
    var context: BoardContext

    @State private var hovering = false
    @State private var editing = false
    @State private var draft = ""
    @FocusState private var titleFocused: Bool

    private var palette: Palette { context.palette }

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Button {
                withAnimation(Motion.toggle) {
                    todo.done ? context.model.reopen(todo.id.uuidString)
                               : context.model.complete(todo.id.uuidString)
                }
            } label: {
                LigneCheckbox(done: todo.done, palette: palette, size: 14)
                    .frame(width: 18, height: 18)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())
            .help(todo.done ? "Reopen" : "Complete")

            VStack(alignment: .leading, spacing: 2) {
                if editing {
                    TextField("", text: $draft)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12.5))
                        .foregroundStyle(palette.ink)
                        .focused($titleFocused)
                        .onSubmit(commitEdit)
                        .onExitCommand(perform: { editing = false })
                        .onChange(of: titleFocused) { _, focused in
                            if !focused { commitEdit() }
                        }
                } else {
                    Text(todo.title)
                        .font(.system(size: 12.5))
                        .foregroundStyle(todo.done ? palette.inkDim : palette.ink)
                        .strikethrough(todo.done, color: palette.inkDim)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let body = todo.description {
                    Text(body)
                        .font(.system(size: 10.5))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                }
                if let assignee = todo.assignee {
                    Text(assignee == TodoQuery.human ? "human" : "@\(assignee)")
                        .font(.system(size: 10.5))
                        .foregroundStyle(assignee == TodoQuery.human ? palette.emberDeep
                                                                     : palette.inkDim)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if let link = todo.link {
                Button { context.onOpenLink(link) } label: {
                    Image(systemName: "paperclip")
                        .font(.system(size: 9))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 12, height: 12)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Open note: \(link)")
            }
        }
        .padding(.horizontal, BoardMetrics.ticketPadding)
        .padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(hovering ? palette.line : palette.sand, lineWidth: Line.hairline))
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            if !editing { context.onOpenTicket(todo) }
        }
        .onHover { hovering = $0 }
        .contextMenu { menu }
        .draggable(todo.id.uuidString) { dragPreview }
        .dropDestination(for: String.self) { items, location in
            guard let id = items.first, id != todo.id.uuidString else { return false }
            drop(id, at: location.y < BoardMetrics.reorderSplit ? .before : .after)
            return true
        }
    }

    // MARK: menu

    @ViewBuilder
    private var menu: some View {
        Button("Open ticket…") { context.onOpenTicket(todo) }
        Divider()
        Menu("Status") {
            ForEach(Status.workflow, id: \.self) { status in
                Button {
                    withAnimation(Motion.toggle) {
                        context.model.setStatus(todo.id.uuidString, to: status)
                    }
                } label: {
                    if todo.status == status {
                        Label(status.displayName, systemImage: "checkmark")
                    } else {
                        Text(status.displayName)
                    }
                }
            }
        }
        if !context.lanes.isEmpty {
            Menu("Move to") {
                laneItem("Uncategorized", target: nil)
                Divider()
                ForEach(context.lanes, id: \.self) { name in laneItem(name, target: name) }
            }
        }
        Divider()
        if let link = todo.link {
            Button("Open note") { context.onOpenLink(link) }
        }
        Button("Rename", action: beginEdit)
        if todo.done {
            Button("Reopen") {
                withAnimation(Motion.toggle) { context.model.reopen(todo.id.uuidString) }
            }
        }
        Button("Delete", role: .destructive) {
            withAnimation(Motion.remove) { context.model.delete(todo.id.uuidString) }
        }
    }

    @ViewBuilder
    private func laneItem(_ title: String, target: String?) -> some View {
        Button {
            withAnimation(Motion.toggle) {
                context.model.place(todo.id.uuidString, lane: target, status: todo.status)
            }
        } label: {
            if todo.lane == target {
                Label(title, systemImage: "checkmark")
            } else {
                Text(title)
            }
        }
    }

    private var dragPreview: some View {
        Text(todo.title)
            .font(.system(size: 12.5))
            .foregroundStyle(palette.ink)
            .lineLimit(2)
            .frame(maxWidth: 200, alignment: .leading)
            .padding(.horizontal, 9)
            .padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    // MARK: edit + drop

    private func beginEdit() {
        draft = todo.title
        editing = true
        titleFocused = true
    }

    private func commitEdit() {
        guard editing else { return }
        editing = false
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty, trimmed != todo.title {
            context.model.update(todo.id.uuidString, title: trimmed)
        }
    }

    /// A ticket dropped on this one: same cell, just before or after it — the arithmetic is
    /// the core's (`TodoOrdering.dropIndex`), the same one the list's drop uses.
    private func drop(_ draggedID: String, at edge: DropEdge) {
        guard let dragged = UUID(uuidString: draggedID),
              let index = TodoOrdering.dropIndex(in: context.model.todos,
                                                 moving: dragged,
                                                 relativeTo: todo.id,
                                                 after: edge == .after)
        else { return }
        withAnimation(Motion.toggle) {
            context.model.place(draggedID, lane: todo.lane, status: todo.status, at: index)
        }
    }
}

// MARK: - Metrics

/// The board's own scale, kept here beside the grid it measures (the shared scale is in
/// LigneTheme: this is what only the board needs).
private enum BoardMetrics {
    static let gap: CGFloat = 8
    static let ticketGap: CGFloat = 6
    static let ticketPadding: CGFloat = 8
    static let cellPadding: CGFloat = 4
    /// An empty cell still has to be a target, and its tickets have to wrap.
    static let minCellHeight: CGFloat = 40
    /// Which half of a ticket counts as "after" when one is dropped on another. A tile is
    /// one to three lines tall, so the split is a fixed, forgiving height.
    static let reorderSplit: CGFloat = 18
}
