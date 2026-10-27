import SwiftUI
import AppKit
import TandemCore

// The shared Ligne Claire controls (reach B).
//
// The register at UI scale is restraint: one hairline (Line.hairline), flat
// fills, no drop shadows, native body typography, generous negative space.
// Every metric comes from LigneTheme.swift — no literals here.

/// What the list face is showing.
///
/// The first three are the shipped `Filter` values — `open`, `done`, `all` — and are spelled
/// as the core spells them, so the segmented control and `todo list --filter` name the same
/// three. They part on the archive alone: this `All` includes archived tickets, the CLI's
/// `all` does not, and the CLI asks for the archive by name (ADR 0017, 2026-10-21).
/// `archived` is the fourth state only the app can ask for: the archive as its own view,
/// which is how an archived ticket is found again and brought back. There is no fourth
/// `--filter` value by decision (ADR 0017): an agent reaches the archive through a query,
/// the human through a click.
enum ListScope: String, CaseIterable, Identifiable {
    case open
    case done
    case all
    case archived

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .open: return "Open"
        case .done: return "Done"
        case .all: return "All"
        case .archived: return "Archived"
        }
    }

    /// The one view definition: three shipped filters, plus the archive.
    var query: TicketQuery {
        switch self {
        case .open: return Filter.open.query
        case .done: return Filter.done.query
        case .all: return Filter.all.query
        case .archived: return TicketQuery(archived: .only)
        }
    }
}

// MARK: checkbox — flat, one hairline, one-way (v2 decision)

/// The checkbox. Checking it is the satisfying moment: the mark is *drawn in*
/// (not faded) and the box gives one springy pop as it fills to moss. The tick
/// is always in the hierarchy, clipped by `trim`, so it can animate without a
/// transition; the pop only fires on a false→true flip, never on first render,
/// so a list that loads with done rows stays still.
struct LigneCheckbox: View {
    var done: Bool
    var palette: Palette
    var size: CGFloat = 18

    /// Transient scale during the pop (1 at rest).
    @State private var pop: CGFloat = 1

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: Radius.checkbox)
                .fill(done ? AnyShapeStyle(palette.moss) : AnyShapeStyle(palette.parchment))
            Path { p in
                p.move(to: CGPoint(x: 0.26, y: 0.52))
                p.addLine(to: CGPoint(x: 0.42, y: 0.68))
                p.addLine(to: CGPoint(x: 0.76, y: 0.34))
            }
            .path(in: CGRect(x: 0, y: 0, width: size, height: size))
            .trim(from: 0, to: done ? 1 : 0)
            .stroke(palette.paper, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
            RoundedRectangle(cornerRadius: Radius.checkbox)
                .strokeBorder(palette.line, lineWidth: Line.hairline)
        }
        .frame(width: size, height: size)
        .scaleEffect(pop)
        // The fill and the drawn tick share the checkbox's own spring, so the
        // beat reads as one gesture regardless of the caller's animation.
        .animation(Motion.check, value: done)
        .onChange(of: done) { _, isDone in
            guard isDone else { return }
            withAnimation(Motion.check) { pop = 1.18 }
            withAnimation(Motion.check.delay(0.06)) { pop = 1 }
        }
    }
}

// MARK: segmented filter — soft-wash selection, muted counts, no capsules

struct LigneFilter: View {
    @Binding var selection: ListScope
    var counts: (open: Int, done: Int, archived: Int)
    var palette: Palette

    var body: some View {
        HStack(spacing: 0) {
            ForEach(ListScope.allCases) { scope in
                segment(scope)
            }
        }
        .frame(width: 296, height: Space.control)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    private func segment(_ scope: ListScope) -> some View {
        let selected = scope == selection
        let count = scope == .open ? counts.open
                  : scope == .done ? counts.done
                  : scope == .archived ? counts.archived
                  : counts.open + counts.done + counts.archived
        return Button {
            withAnimation(Motion.toggle) { selection = scope }
        } label: {
            HStack(spacing: 4) {
                Text(scope.displayName)
                    .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                    .foregroundStyle(selected ? palette.ink : palette.inkDim)
                Text("\(count)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(palette.inkDim)
            }
            .frame(maxWidth: .infinity)
            .frame(height: Space.control)
            .background(selected ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: quick-add — flat field, one hairline, ember focus (same weight)

struct LigneQuickAdd: View {
    @Binding var text: String
    var palette: Palette
    var onCommit: () -> Void
    var focus: FocusState<Bool>.Binding
    var height: CGFloat = Space.control
    /// The lane new tickets are filed into, and the lanes to choose from. A `nil`
    /// `onPickLane` draws no chip: the menu-bar popover files nothing into a lane.
    var lane: String? = nil
    var lanes: [String] = []
    var onPickLane: ((String?) -> Void)? = nil
    @State private var placeholder: Bool = true

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(focus.wrappedValue ? palette.ember : palette.inkDim)
            ZStack(alignment: .leading) {
                if placeholder {
                    Text("Add a ticket…")
                        .font(.body)
                        .foregroundStyle(palette.inkDim)
                }
                TextField("", text: $text)
                    .textFieldStyle(.plain)
                    .font(.body)
                    .foregroundStyle(palette.ink)
                    .focused(focus)
                    .onSubmit(onCommit)
                    .onChange(of: text) { _, newValue in
                        placeholder = newValue.isEmpty
                    }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // Where the next ticket will go: the cached default lane, changed here and
            // remembered (settings, not the store — the CLI has `--lane`).
            if let onPickLane {
                QuickAddLaneChip(lane: lane, lanes: lanes, palette: palette, onPick: onPickLane)
            }
        }
        .padding(.horizontal, 11)
        .frame(height: height)
        .frame(maxWidth: .infinity)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
        .overlay(
            RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(focus.wrappedValue ? palette.ember : palette.line,
                              lineWidth: Line.hairline)
        )
    }
}

// MARK: search — magnifying-glass · plain field, one hairline (the sidebar's in-tree look)

/// The Tickets-face search field: a magnifying glass, a plain `TextField` on
/// parchment with one `line` hairline, `Radius.control` — visually the same
/// field as the Bikeshed sidebar's in-tree search. It is a pure view filter:
/// it narrows what is drawn and never touches the store.
struct LigneSearchField: View {
    @Binding var text: String
    var palette: Palette
    var focus: FocusState<Bool>.Binding
    var height: CGFloat = Space.control

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(palette.inkDim)
            TextField("Search tickets…", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5))
                .foregroundStyle(palette.ink)
                .focused(focus)
        }
        .padding(.horizontal, 11)
        .frame(height: height)
        .frame(maxWidth: .infinity)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
        .help("Filter tickets by title (⌘F)")
    }
}

// MARK: row — checkbox · title · hover delete (GUI-only)

/// Which half of a row a dragged ticket was dropped on. The row's own height decides
/// the split, so the caller never sees a coordinate.
enum DropEdge {
    case before
    case after
}

/// Drag-and-drop for one row: the row is a drag source (its id is the payload) and a
/// drop target (dropping on its top half means "before me", the bottom half "after
/// me"). No-op when the caller passes no `onDrop`, so the menu-bar popover and any
/// future read-only list can reuse `LigneRow` without drag.
struct RowDragAndDrop: ViewModifier {
    var payload: String
    var title: String
    var palette: Palette
    var onDrop: ((String, DropEdge) -> Void)?

    @State private var targeted = false

    func body(content: Content) -> some View {
        if let onDrop {
            content
                .draggable(payload) { preview }
                .dropDestination(for: String.self) { items, location in
                    // Ignore a drop of the row on itself — nothing to do.
                    guard let id = items.first, id != payload else { return false }
                    onDrop(id, location.y < Space.row / 2 ? .before : .after)
                    return true
                } isTargeted: { targeted = $0 }
                .background(targeted ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
                .overlay(alignment: .leading) {
                    if targeted {
                        Rectangle().fill(palette.accent).frame(width: 2)
                    }
                }
        } else {
            content
        }
    }

    /// The drag image: the row's title on a small parchment card, so the drag reads
    /// as "this ticket", not a raw string.
    private var preview: some View {
        Text(title)
            .font(.system(size: 12.5))
            .foregroundStyle(palette.ink)
            .lineLimit(1)
            .frame(maxWidth: 260, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}

/// A main-window row. The checkbox is the toggle: checking completes, and
/// un-checking a done ticket reopens it (GUI-only — no reopen in the CLI).
/// Double-clicking the row opens the ticket sheet — the whole ticket in one place.
/// The title edits in place from the menu's Rename item: Return commits, Esc cancels,
/// and clicking away commits. A plain row click is never destructive. Delete appears on hover and in the context menu — also GUI-only
/// (agents have no delete in the CLI). Rows drag to reorder and to move
/// between lanes when the caller passes `onDropTicket`, and the context menu's
/// "Move to" files one into a lane at any scroll position.
/// A linked ticket (`ticket.link`) shows a quiet trailing paperclip that opens the note,
/// with "Link to note…" / "Remove link" in the context menu (GUI-only, ADR 0010).
struct LigneRow: View {
    var ticket: Ticket
    var palette: Palette
    var onComplete: () -> Void
    var onReopen: () -> Void
    var onEdit: (String) -> Void
    var onDelete: () -> Void
    /// Opens the ticket's linked Bikeshed note (its bikeshed-root-relative path). GUI-only.
    var onOpenLink: (String) -> Void
    /// Lane names for the "Move to" menu; empty (the flat case) hides it, since
    /// there is nowhere to move to. GUI-only.
    var lanes: [String] = []
    var onMoveToLane: ((String?) -> Void)? = nil
    /// Opens the note picker to link this ticket to a note. `nil` hides the item.
    var onSetLink: (() -> Void)? = nil
    /// Clears this ticket's link. `nil` hides the item.
    var onClearLink: (() -> Void)? = nil
    /// Brings an archived ticket back into the default views. `nil` hides the item; only
    /// an archived row shows it, so every caller can pass the same closure.
    var onUnarchive: (() -> Void)? = nil
    /// Opens the ticket sheet — the whole ticket, read and edited in one place. Double
    /// clicking a row (anywhere but the title, which renames) does the same. `nil` hides
    /// both.
    var onOpenTicket: (() -> Void)? = nil
    /// Called when another row is dropped on this one, with which half. `nil`
    /// leaves the row undraggable.
    var onDropTicket: ((_ draggedID: String, _ edge: DropEdge) -> Void)? = nil
    @State private var hovering = false
    @State private var editing = false
    @State private var draft = ""
    @FocusState private var titleFocused: Bool

    var body: some View {
        HStack(spacing: 10) {
            Button(action: ticket.done ? onReopen : onComplete) {
                LigneCheckbox(done: ticket.done, palette: palette)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            // The title, and — when the ticket has one — the first lines of its body. The
            // body is the ticket's substance, so it reads in the row; editing it is the
            // sheet's job, and a row never expands.
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    if editing {
                        TextField("", text: $draft)
                            .textFieldStyle(.plain)
                            .font(.body)
                            .foregroundStyle(palette.ink)
                            .focused($titleFocused)
                            .onSubmit(commitEdit)
                            .onExitCommand(perform: cancelEdit)
                            // Clicking away commits (the inline-edit convention).
                            .onChange(of: titleFocused) { _, focused in
                                if !focused { commitEdit() }
                            }
                    } else {
                        Text(ticket.title)
                            .font(.body)
                            .foregroundStyle(ticket.done ? palette.inkDim : palette.ink)
                            .strikethrough(ticket.done, color: palette.inkDim)
                            .lineLimit(1)
                    }

                    // An archived ticket is still in the store; in the views that include
                    // the archive it says so, quietly, so it is never mistaken for live work.
                    if ticket.archived {
                        Text("archived")
                            .font(.system(size: 9.5))
                            .foregroundStyle(palette.inkDim)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .overlay(Capsule().strokeBorder(palette.sand, lineWidth: Line.hairline))
                            .help("Archived: out of the default views, still in the store")
                    }

                    // The labels the board's tile already draws, on the title's line:
                    // Status, ref, assignee. `backlog` is the default, so it stays
                    // silent — the Open view would otherwise be a wall of one word.
                    if ticket.status != .backlog {
                        Text(ticket.status.displayName)
                            .font(.system(size: 9.5))
                            .foregroundStyle(palette.inkDim)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .overlay(Capsule().strokeBorder(palette.sand, lineWidth: Line.hairline))
                    }
                    if let ref = ticket.ref {
                        Text(ref)
                            .font(.system(size: 9.5, design: .monospaced))
                            .foregroundStyle(palette.inkDim)
                            .lineLimit(1)
                    }
                    if let assignee = ticket.assignee, let label = AssigneeLabel.of(assignee) {
                        Text(label)
                            .font(.system(size: 10.5))
                            .foregroundStyle(assignee == TicketQuery.human ? palette.emberDeep
                                                                         : palette.inkDim)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                }
                if !editing, let body = ticket.description {
                    Text(body)
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                        .help("Double-click to open the ticket")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            Spacer(minLength: 8)

            // The link affordance: a quiet paperclip, drawn only when the ticket is
            // linked. Clicking it opens the note (the GUI never reads note content).
            if let link = ticket.link {
                Button { onOpenLink(link) } label: {
                    Image(systemName: "paperclip")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 16, height: 16)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Open note: \(link)")
            }

            // fixed slot so nothing shifts when the delete appears
            ZStack {
                if hovering {
                    Button(action: onDelete) {
                        Image(systemName: "xmark")
                            .font(.system(size: 8, weight: .semibold))
                            .foregroundStyle(palette.emberDeep)
                            .frame(width: 14, height: 14)
                            .background(Circle().strokeBorder(palette.ember, lineWidth: Line.hairline))
                    }
                    .buttonStyle(.plain)
                    .transition(.opacity)
                }
            }
            .frame(width: 22, height: 22)
        }
        .padding(.horizontal, Space.gutter)
        .padding(.vertical, 7)
        .frame(minHeight: Space.row)
        .background(hovering ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .contentShape(Rectangle())
        .onTapGesture(count: 2) {
            // Double-click is "open the ticket", the whole row — including the title. Rename
            // is the menu's item (inline, still) or the sheet's title field, so the big,
            // predictable target does the common thing.
            if !editing { onOpenTicket?() }
        }
        .onHover { hovering = $0 }
        .contextMenu {
            if let onOpenTicket {
                Button("Open ticket…", action: onOpenTicket)
                Divider()
            }
            if ticket.archived, let onUnarchive {
                Button("Unarchive", action: onUnarchive)
                Divider()
            }
            if let link = ticket.link {
                Button("Open note") { onOpenLink(link) }
                if let onSetLink { Button("Link to note…", action: onSetLink) }
                if let onClearLink { Button("Remove link", action: onClearLink) }
            } else if let onSetLink {
                Button("Link to note…", action: onSetLink)
            }
            if let onMoveToLane, !lanes.isEmpty {
                Menu("Move to") {
                    moveItem("Uncategorized", target: nil, action: onMoveToLane)
                    Divider()
                    ForEach(lanes, id: \.self) { name in
                        moveItem(name, target: name, action: onMoveToLane)
                    }
                }
            }
            Button("Rename", action: beginEdit)
            if ticket.done {
                Button("Reopen", action: onReopen)
            }
            Button("Delete", role: .destructive, action: onDelete)
        }
        .modifier(RowDragAndDrop(payload: ticket.id.uuidString, title: ticket.title,
                                 palette: palette, onDrop: onDropTicket))
    }

    /// One "Move to" entry, checkmarked when it is the ticket's current lane.
    /// (`Label` in a menu is what draws the checkmark; a selected `Toggle` would
    /// also work but reads as a switch.)
    @ViewBuilder
    private func moveItem(_ title: String, target: String?,
                          action: @escaping (String?) -> Void) -> some View {
        Button {
            action(target)
        } label: {
            if ticket.lane == target {
                Label(title, systemImage: "checkmark")
            } else {
                Text(title)
            }
        }
    }

    // MARK: inline edit

    private func beginEdit() {
        draft = ticket.title
        editing = true
        titleFocused = true
    }

    /// Return, or clicking away: keep a non-empty change, drop an empty or
    /// unchanged one (the store also refuses empty titles — this avoids the error
    /// banner for what reads as "nothing typed").
    private func commitEdit() {
        guard editing else { return }
        editing = false
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty, trimmed != ticket.title { onEdit(trimmed) }
    }

    /// Esc: leave the title as it was.
    private func cancelEdit() {
        editing = false
    }
}

// MARK: empty states — the one illustrated moment in the main window

struct LigneEmptyState: View {
    var palette: Palette
    var title: String
    var subtitle: String
    /// Hero state ("All clear"): the brand photo, large, where it can read.
    /// Other empty states get the smaller photo.
    var hero: Bool

    var body: some View {
        VStack(spacing: 18) {
            brandArt
            VStack(spacing: 3) {
                Text(title)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text(subtitle)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.inkDim)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.vertical, 28)
    }

    /// The brand photo (ticket 03), framed with a hairline + corner rounding
    /// so it reads as deliberate on the paper. Falls back to the vector bike
    /// when the asset is missing, so the state never renders empty.
    @ViewBuilder
    private var brandArt: some View {
        if let photo = BrandAsset.bikePhoto {
            let width: CGFloat = hero ? 220 : 96
            let radius: CGFloat = hero ? Radius.control : Radius.checkbox
            photo
                .resizable()
                .scaledToFill()
                .frame(width: width, height: width * BrandAsset.bikePhotoAspect)
                .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: radius, style: .continuous)
                        .strokeBorder(palette.sand, lineWidth: Line.hairline)
                )
        } else if hero {
            BikeScene(palette: palette, riders: true, ground: true)
                .frame(width: 220, height: 220 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
        } else {
            BikeScene(palette: palette, riders: false, ground: true)
                .frame(width: 96, height: 96 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
        }
    }
}

// MARK: quiet text button — "Archive done"

struct TextButton: View {
    var title: String
    var palette: Palette
    var enabled: Bool = true
    var action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12.5))
                .lineLimit(1)
                .fixedSize()
                .foregroundStyle(enabled ? (hovering ? palette.ink : palette.inkDim)
                                         : palette.inkDim.opacity(0.45))
                .padding(.horizontal, 6)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .onHover { hovering = $0 }
    }
}

// MARK: primary button — the popover's "Open Tandem"

struct PrimaryButton: View {
    var title: String
    var palette: Palette
    var height: CGFloat = 30
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(palette.paper)
                .frame(maxWidth: .infinity)
                .frame(height: height)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.accent))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: status banners — restyled, no illustration (SF symbol only)

/// The restyled status banner (corrupt-store recovery or an operation error):
/// parchment, 1pt ink hairline, ember accent bar on recovery, SF symbol, flat
/// fills. No illustration in the banner (ticket 01 handoff).
struct LigneBanner: View {
    var palette: Palette
    var systemImage: String
    var title: String
    /// Optional monospaced detail line (e.g. the backup path).
    var detail: String? = nil
    /// The ember accent bar — recovery banners only.
    var accentBar: Bool = false
    /// When set, offers a "Restore" button that reveals the backup in Finder.
    /// The GUI never writes the store (ADR 0001): core is the sole writer, and
    /// the backup was corrupt — the human inspects/repairs it and restores it
    /// by hand.
    var restorePath: String? = nil
    /// Optional action buttons (the conflict banner's
    /// [Overwrite with mine] / [Take theirs] choice).
    var actions: [(title: String, action: () -> Void)] = []
    /// When false, no dismiss xmark (the conflict banner: the choice is the
    /// only way out — dismissing would silently discard it).
    var showDismiss: Bool = true
    var onDismiss: (() -> Void)? = nil

    var body: some View {
        HStack(spacing: 10) {
            if accentBar {
                RoundedRectangle(cornerRadius: 2)
                    .fill(palette.ember)
                    .frame(width: 4)
            }
            Image(systemName: systemImage)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(palette.emberDeep)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                if let detail {
                    Text(detail)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .textSelection(.enabled)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let restorePath {
                Button("Restore") {
                    NSWorkspace.shared.activateFileViewerSelecting(
                        [URL(fileURLWithPath: restorePath)]
                    )
                }
                .controlSize(.small)
            }
            ForEach(actions.indices, id: \.self) { i in
                Button(actions[i].title, action: actions[i].action)
                    .controlSize(.small)
            }
            if showDismiss, let onDismiss {
                Button(action: onDismiss) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(RoundedRectangle(cornerRadius: Radius.card).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.card)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}

/// The backup path named in a core recovery note ("Previous store was corrupt
/// and backed up to <path>; the store restarted empty."), or `nil` when the
/// note is not in that shape (the banner then shows the whole note).
func backupPath(from note: String) -> String? {
    guard let start = note.range(of: "backed up to ")?.upperBound else { return nil }
    let rest = note[start...]
    guard let end = rest.firstIndex(of: ";") else { return nil }
    return String(rest[..<end])
}

/// The quick-add's lane chip: the cached default lane, and a menu to change it. Quiet by
/// design — it is a destination, not a warning — and it never takes the focus from the
/// field (choosing a lane is a click, then Return files the ticket).
private struct QuickAddLaneChip: View {
    var lane: String?
    var lanes: [String]
    var palette: Palette
    var onPick: (String?) -> Void

    @State private var hovering = false

    var body: some View {
        Menu {
            Button { onPick(nil) } label: {
                if lane == nil { Label("No lane", systemImage: "checkmark") } else { Text("No lane") }
            }
            if !lanes.isEmpty { Divider() }
            ForEach(lanes, id: \.self) { name in
                Button { onPick(name) } label: {
                    if lane == name { Label(name, systemImage: "checkmark") } else { Text(name) }
                }
            }
        } label: {
            HStack(spacing: 3) {
                Text(lane ?? "No lane")
                    .font(.system(size: 11))
                    .foregroundStyle(hovering ? palette.ink : palette.inkDim)
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.system(size: 7.5, weight: .semibold))
                    .foregroundStyle(palette.inkDim)
            }
            .padding(.horizontal, 6)
            .frame(height: 20)
            .overlay(Capsule().strokeBorder(palette.line, lineWidth: Line.hairline))
            .contentShape(Capsule())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("File new tickets into this cycle lane")
        .onHover { hovering = $0 }
    }
}

// MARK: - undo toast — a brief offer, one undo

/// The toast a completion leaves behind: the ticket's name, and an **Undo**.
///
/// Completing a ticket is one click and immediate, and it erases the stage the
/// ticket came from (ADR 0017) — so a mis-click costs a re-triage, not one click
/// back. The toast is the way back: it is brief (`CompletionUndo.window` — six
/// seconds), it offers exactly one undo, and it is not a history.
///
/// The clock belongs to the model, not to this view: the offer expires whether or
/// not a face is drawing it.
struct LigneToast: View {
    var offer: CompletionUndo
    var palette: Palette
    var onUndo: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: "checkmark")
                .font(.system(size: 8.5, weight: .bold))
                .foregroundStyle(palette.paper)
                .frame(width: 15, height: 15)
                .background(Circle().fill(palette.moss))
            Text("Completed “\(offer.title)”")
                .font(.system(size: 12.5))
                .foregroundStyle(palette.ink)
                .lineLimit(1)
            Spacer(minLength: 10)
            // The one primary action in the toast: sage is the app's single
            // primary-action colour, and this is it.
            Button(action: onUndo) {
                Text("Undo")
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(hovering ? palette.ink : palette.accent)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { hovering = $0 }
            .help("Put the ticket back in Backlog")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(RoundedRectangle(cornerRadius: Radius.card).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.card)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}
