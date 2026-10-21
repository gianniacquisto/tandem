import SwiftUI
import TandemCore

// The lanes chrome (reach B): a quiet header per group, a compact inline field
// for naming one. Same register as the rest of the Ligne Claire controls — one
// hairline, flat fills, no capsules.

/// One rendered group in the Tickets list. `key == nil` is the default (ungrouped)
/// area: it always comes first and gets **no header** — it is just the top of the
/// list. Named groups draw a header.
struct TicketGroup: Identifiable {
    var key: String?
    var tickets: [Ticket]

    var id: String { key ?? "\u{0}default" }
    var isDefault: Bool { key == nil }
}

// MARK: lane header — name · count · hover menu (rename / move / delete)

/// One lane's header. There is no header for the default (ungrouped) area — that
/// area is just the top of the list. A lane can be renamed, moved up or down, and
/// deleted (deleting one only ungroups its tickets). The header is also a drop target:
/// dropping a row here moves it to the top of the lane.
struct LigneLaneHeader: View {
    var title: String
    var count: Int
    var palette: Palette
    var isRenaming: Bool
    @Binding var draft: String
    var onBeginRename: () -> Void
    var onCommitRename: () -> Void
    var onDelete: () -> Void
    var canMoveUp: Bool
    var canMoveDown: Bool
    var onMoveUp: () -> Void
    var onMoveDown: () -> Void
    /// Called when a row is dropped on this header. `nil` leaves it inert.
    var onDropIntoLane: ((_ draggedID: String) -> Void)? = nil

    @FocusState private var focused: Bool
    @State private var hovering = false
    @State private var targeted = false

    var body: some View {
        HStack(spacing: 8) {
            if isRenaming {
                TextField("", text: $draft)
                    .textFieldStyle(.plain)
                    .font(.system(size: 11.5, weight: .semibold))
                    .foregroundStyle(palette.ink)
                    .focused($focused)
                    .onSubmit(onCommitRename)
                    .onExitCommand(perform: onCommitRename)
                    .onChange(of: focused) { _, f in if !f { onCommitRename() } }
                    .onAppear { focused = true }
            } else {
                Text(title)
                    .font(.system(size: 11.5, weight: .semibold))
                    .tracking(0.4)
                    .foregroundStyle(palette.inkDim)
                    .onTapGesture(count: 2) { onBeginRename() }
                    .help("Double-click to rename")
                Text("\(count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(palette.inkDim.opacity(0.7))
            }
            Spacer(minLength: 8)
            if !isRenaming {
                Menu {
                    Button("Rename", action: onBeginRename)
                    Button("Move Up", action: onMoveUp).disabled(!canMoveUp)
                    Button("Move Down", action: onMoveDown).disabled(!canMoveDown)
                    Button("Delete Cycle lane", role: .destructive, action: onDelete)
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                // Dim until hovered, never absent: the control has to be findable.
                // Kept in the hierarchy (not added/removed on hover) so an open menu
                // is never torn down when the pointer leaves the row.
                .opacity(hovering ? 1 : 0.45)
            }
        }
        .padding(.horizontal, Space.gutter)
        .frame(height: 30)
        .padding(.top, 6)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .dropDestination(for: String.self) { items, _ in
            guard let id = items.first, let onDropIntoLane else { return false }
            onDropIntoLane(id)
            return true
        } isTargeted: { targeted = $0 }
        .background(targeted ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .overlay(alignment: .leading) {
            if targeted {
                Rectangle().fill(palette.accent).frame(width: 2)
            }
        }
        .contextMenu {
            Button("Rename", action: onBeginRename)
            Button("Move Up", action: onMoveUp).disabled(!canMoveUp)
            Button("Move Down", action: onMoveDown).disabled(!canMoveDown)
            Button("Delete Cycle lane", role: .destructive, action: onDelete)
        }
    }
}

// MARK: lane field — the compact inline "name a lane" field

/// The inline field that names a new lane (filter row, so it is always on
/// screen). Return commits; Esc cancels (blur commits, the inline-edit
/// convention — Esc clears the draft first, so the commit is a no-op).
struct LigneLaneField: View {
    @Binding var text: String
    var palette: Palette
    var focus: FocusState<Bool>.Binding
    var onCommit: () -> Void
    var onCancel: () -> Void

    var body: some View {
        TextField("Cycle lane name", text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 12.5))
            .foregroundStyle(palette.ink)
            .focused(focus)
            .onSubmit(onCommit)
            .onExitCommand(perform: onCancel)
            .onChange(of: focus.wrappedValue) { _, f in if !f { onCommit() } }
            .padding(.horizontal, 8)
            .frame(width: 120, height: Space.control)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.ember, lineWidth: Line.hairline))
            .onAppear { focus.wrappedValue = true }
    }
}
