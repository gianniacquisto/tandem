import SwiftUI
import TandemCore

// The board face: Status columns, cycle-lane rows.
//
// It is the third tab, beside the list rather than instead of it (2026-10-20): the list
// stays the place to capture ("Add a ticket…"), name lanes, and read the flat running
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
        case .onHold: return "On hold"
        // The retired stage. No column or menu offers it; a legacy row still reads
        // with the phrase its column used to carry (TAN-196).
        case .refined: return "To refine"
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
    var onOpenTicket: (Ticket) -> Void
    /// Promotes a ticket to a bikeshed note — the parent owns the bikeshed.
    var onPromote: (Ticket) -> PromotionOutcome
}

/// The board's outer frame: the search field (the same `TicketQuery` text the list uses),
/// "Archive done", then the grid.
struct BoardFace: View {
    var model: TandemModel
    @Binding var search: String
    var palette: Palette
    var focus: FocusState<Bool>.Binding
    /// Opens the ticket's linked note — the parent owns the tab switch.
    var onOpenLink: (String) -> Void
    /// Promotes a ticket to a bikeshed note — the parent owns the bikeshed.
    var onPromote: (Ticket) -> PromotionOutcome

    /// The ticket open in the ticket sheet, if any.
    @State private var ticketFor: Ticket?

    /// The tickets in view: archived ones are out (the board is about work in the way),
    /// and the search narrows by title and description. One `TicketQuery`, as ever.
    private var visible: [Ticket] {
        var query = TicketQuery()
        query.text = search
        return query.apply(to: model.tickets)
    }

    private var rows: [BoardLane] {
        TicketBoard.rows(from: visible, lanes: model.lanes)
    }

    private var searching: Bool {
        !search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The lanes a ticket can be moved into: the declared ones plus any dangling name
    /// already on a ticket, in the board's own row order.
    private var lanes: [String] {
        TicketBoard.orderedKeys(declared: model.lanes, todos: model.tickets).compactMap { $0 }
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
                           enabled: !model.doneTickets.isEmpty) {
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
            } else if model.tickets.isEmpty {
                // A store with nothing in it draws no grid: a row of empty cells reads as a
                // board that failed to load. Lanes without tickets still draw — they are
                // the targets that make the next drop possible.
                LigneEmptyState(palette: palette,
                                title: "Nothing on the board",
                                subtitle: "Add a ticket in Tickets, or from the CLI.",
                                hero: true)
            } else {
                board
            }
        }
        .sheet(item: $ticketFor) { ticket in
            TicketSheet(
                ticket: ticket,
                palette: palette,
                lanes: lanes,
                roster: TandemSettings.roster(),
                onSave: { edits in
                    model.saveTicket(ticket.id.uuidString, edits: edits)
                    ticketFor = nil
                },
                onCancel: { ticketFor = nil },
                onComment: { model.postComment(ticket.id.uuidString, body: $0) },
                onRefreshComments: { await model.comments(for: ticket.id.uuidString) },
                onOpenLink: onOpenLink,
                onPromote: { onPromote(ticket) })
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
    var tickets: [Ticket]
    var context: BoardContext

    @State private var targeted = false

    var body: some View {
        VStack(alignment: .leading, spacing: BoardMetrics.ticketGap) {
            ForEach(tickets) { ticket in
                BoardTicket(ticket: ticket, context: context)
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
    var ticket: Ticket
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
                    ticket.done ? context.model.reopen(ticket.id.uuidString)
                               : context.model.complete(ticket.id.uuidString)
                }
            } label: {
                LigneCheckbox(done: ticket.done, palette: palette, size: 14)
                    .frame(width: 18, height: 18)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())
            .help(ticket.done ? "Reopen" : "Complete")

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
                    Text(ticket.title)
                        .font(.system(size: 12.5))
                        .foregroundStyle(ticket.done ? palette.inkDim : palette.ink)
                        .strikethrough(ticket.done, color: palette.inkDim)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let body = ticket.description {
                    Text(body)
                        .font(.system(size: 10.5))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                }
                if let assignee = ticket.assignee {
                    Text(assignee == TicketQuery.human ? "human" : "@\(assignee)")
                        .font(.system(size: 10.5))
                        .foregroundStyle(assignee == TicketQuery.human ? palette.emberDeep
                                                                     : palette.inkDim)
                        .lineLimit(1)
                }
                // The ticket's ref (ticket 6598E091) — the handle a human writes in chat.
                // A ticket in the default area has none.
                if let ref = ticket.ref {
                    Text(ref)
                        .font(.system(size: 9.5, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if let link = ticket.link {
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
            if !editing { context.onOpenTicket(ticket) }
        }
        .onHover { hovering = $0 }
        .contextMenu { menu }
        .draggable(ticket.id.uuidString) { dragPreview }
        .dropDestination(for: String.self) { items, location in
            guard let id = items.first, id != ticket.id.uuidString else { return false }
            drop(id, at: location.y < BoardMetrics.reorderSplit ? .before : .after)
            return true
        }
    }

    // MARK: menu

    @ViewBuilder
    private var menu: some View {
        Button("Open ticket…") { context.onOpenTicket(ticket) }
        Divider()
        Menu("Status") {
            ForEach(Status.workflow, id: \.self) { status in
                Button {
                    withAnimation(Motion.toggle) {
                        context.model.setStatus(ticket.id.uuidString, to: status)
                    }
                } label: {
                    if ticket.status == status {
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
        if let link = ticket.link {
            Button("Open note") { context.onOpenLink(link) }
        }
        Button("Rename", action: beginEdit)
        if ticket.done {
            Button("Reopen") {
                withAnimation(Motion.toggle) { context.model.reopen(ticket.id.uuidString) }
            }
        }
        Button("Delete", role: .destructive) {
            withAnimation(Motion.remove) { context.model.delete(ticket.id.uuidString) }
        }
    }

    @ViewBuilder
    private func laneItem(_ title: String, target: String?) -> some View {
        Button {
            withAnimation(Motion.toggle) {
                context.model.place(ticket.id.uuidString, lane: target, status: ticket.status)
            }
        } label: {
            if ticket.lane == target {
                Label(title, systemImage: "checkmark")
            } else {
                Text(title)
            }
        }
    }

    private var dragPreview: some View {
        Text(ticket.title)
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
        draft = ticket.title
        editing = true
        titleFocused = true
    }

    private func commitEdit() {
        guard editing else { return }
        editing = false
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty, trimmed != ticket.title {
            context.model.update(ticket.id.uuidString, title: trimmed)
        }
    }

    /// A ticket dropped on this one: same cell, just before or after it — the arithmetic is
    /// the core's (`TicketOrdering.dropIndex`), the same one the list's drop uses.
    private func drop(_ draggedID: String, at edge: DropEdge) {
        guard let dragged = UUID(uuidString: draggedID),
              let index = TicketOrdering.dropIndex(in: context.model.tickets,
                                                 moving: dragged,
                                                 relativeTo: ticket.id,
                                                 after: edge == .after)
        else { return }
        withAnimation(Motion.toggle) {
            context.model.place(draggedID, lane: ticket.lane, status: ticket.status, at: index)
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
