import SwiftUI
import AppKit
import TandemCore

// The shared Ligne Claire controls (reach B).
//
// The register at UI scale is restraint: one hairline (Line.hairline), flat
// fills, no drop shadows, native body typography, generous negative space.
// Every metric comes from LigneTheme.swift — no literals here.

extension Filter {
    var displayName: String {
        switch self {
        case .open: return "Open"
        case .done: return "Done"
        case .all: return "All"
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
    @Binding var selection: Filter
    var counts: (open: Int, done: Int)
    var palette: Palette

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Filter.allCases, id: \.self) { f in
                segment(f)
            }
        }
        .frame(width: 240, height: Space.control)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    private func segment(_ f: Filter) -> some View {
        let selected = f == selection
        let count = f == .open ? counts.open : f == .done ? counts.done : counts.open + counts.done
        return Button {
            withAnimation(Motion.toggle) { selection = f }
        } label: {
            HStack(spacing: 4) {
                Text(f.displayName)
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
    @State private var placeholder: Bool = true

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(focus.wrappedValue ? palette.ember : palette.inkDim)
            ZStack(alignment: .leading) {
                if placeholder {
                    Text("Add a todo…")
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

/// The Todos-face search field: a magnifying glass, a plain `TextField` on
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
            TextField("Search todos…", text: $text)
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
        .help("Filter todos by title (⌘F)")
    }
}

// MARK: row — checkbox · title · hover delete (GUI-only)

/// Which half of a row a dragged todo was dropped on. The row's own height decides
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
    /// as "this todo", not a raw string.
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
/// un-checking a done todo reopens it (GUI-only — no reopen in the CLI).
/// The title edits in place: double-click it, or use the row's Edit item; Return
/// commits, Esc cancels, and clicking away commits. A plain row click is never
/// destructive. Delete appears on hover and in the context menu — also GUI-only
/// (agents have no delete in the CLI). Rows drag to reorder and to move
/// between lanes when the caller passes `onDropTodo`, and the context menu's
/// "Move to" files one into a lane at any scroll position.
/// A linked todo (`todo.link`) shows a quiet trailing paperclip that opens the note,
/// with "Link to note…" / "Remove link" in the context menu (GUI-only, ADR 0010).
struct LigneRow: View {
    var todo: Todo
    var palette: Palette
    var onComplete: () -> Void
    var onReopen: () -> Void
    var onEdit: (String) -> Void
    var onDelete: () -> Void
    /// Opens the todo's linked Bikeshed note (its bikeshed-root-relative path). GUI-only.
    var onOpenLink: (String) -> Void
    /// Lane names for the "Move to" menu; empty (the flat case) hides it, since
    /// there is nowhere to move to. GUI-only.
    var lanes: [String] = []
    var onMoveToLane: ((String?) -> Void)? = nil
    /// Opens the note picker to link this todo to a note. `nil` hides the item.
    var onSetLink: (() -> Void)? = nil
    /// Clears this todo's link. `nil` hides the item.
    var onClearLink: (() -> Void)? = nil
    /// Called when another row is dropped on this one, with which half. `nil`
    /// leaves the row undraggable.
    var onDropTodo: ((_ draggedID: String, _ edge: DropEdge) -> Void)? = nil
    @State private var hovering = false
    @State private var editing = false
    @State private var draft = ""
    @FocusState private var titleFocused: Bool

    var body: some View {
        HStack(spacing: 10) {
            Button(action: todo.done ? onReopen : onComplete) {
                LigneCheckbox(done: todo.done, palette: palette)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

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
                Text(todo.title)
                    .font(.body)
                    .foregroundStyle(todo.done ? palette.inkDim : palette.ink)
                    .strikethrough(todo.done, color: palette.inkDim)
                    .lineLimit(1)
                    .onTapGesture(count: 2, perform: beginEdit)
                    .help("Double-click to edit")
            }

            Spacer(minLength: 8)

            // The link affordance: a quiet paperclip, drawn only when the todo is
            // linked. Clicking it opens the note (the GUI never reads note content).
            if let link = todo.link {
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
        .frame(height: Space.row)
        .background(hovering ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            if let link = todo.link {
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
            Button("Edit", action: beginEdit)
            if todo.done {
                Button("Reopen", action: onReopen)
            }
            Button("Delete", role: .destructive, action: onDelete)
        }
        .modifier(RowDragAndDrop(payload: todo.id.uuidString, title: todo.title,
                                 palette: palette, onDrop: onDropTodo))
    }

    /// One "Move to" entry, checkmarked when it is the todo's current lane.
    /// (`Label` in a menu is what draws the checkmark; a selected `Toggle` would
    /// also work but reads as a switch.)
    @ViewBuilder
    private func moveItem(_ title: String, target: String?,
                          action: @escaping (String?) -> Void) -> some View {
        Button {
            action(target)
        } label: {
            if todo.lane == target {
                Label(title, systemImage: "checkmark")
            } else {
                Text(title)
            }
        }
    }

    // MARK: inline edit

    private func beginEdit() {
        draft = todo.title
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
        if !trimmed.isEmpty, trimmed != todo.title { onEdit(trimmed) }
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

// MARK: quiet text button — "Clear done"

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
