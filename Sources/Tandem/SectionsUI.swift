import SwiftUI
import TandemCore

// The sections chrome (reach B): a quiet header per group, a compact inline field
// for naming one. Same register as the rest of the Ligne Claire controls — one
// hairline, flat fills, no capsules.

/// One rendered group in the Todos list. `key == nil` is the default (ungrouped)
/// area, which always comes first and is unnamed.
struct TodoGroup: Identifiable {
    var key: String?
    var title: String
    var todos: [Todo]

    var id: String { key ?? "\u{0}default" }
    var isDefault: Bool { key == nil }
}

// MARK: section header — name · count · hover menu (rename / delete)

/// One section's header. The default area is not editable (it is not a section):
/// pass `canEdit: false` and it draws the label + count with no menu.
struct LigneSectionHeader: View {
    var title: String
    var count: Int
    var palette: Palette
    var canEdit: Bool
    var isRenaming: Bool
    @Binding var draft: String
    var onBeginRename: () -> Void
    var onCommitRename: () -> Void
    var onDelete: () -> Void

    @FocusState private var focused: Bool
    @State private var hovering = false

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
                    .onTapGesture(count: 2) { if canEdit { onBeginRename() } }
                    .help(canEdit ? "Double-click to rename" : "")
                Text("\(count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(palette.inkDim.opacity(0.7))
            }
            Spacer(minLength: 8)
            if canEdit, !isRenaming {
                Menu {
                    Button("Rename", action: onBeginRename)
                    Button("Delete Section", role: .destructive, action: onDelete)
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
                // Kept in the hierarchy (not added/removed on hover) so an open menu
                // is never torn down when the pointer leaves the row.
                .opacity(hovering ? 1 : 0)
                .allowsHitTesting(hovering)
            }
        }
        .padding(.horizontal, Space.gutter)
        .frame(height: 30)
        .padding(.top, 6)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            if canEdit {
                Button("Rename", action: onBeginRename)
                Button("Delete Section", role: .destructive, action: onDelete)
            }
        }
    }
}

// MARK: section field — the compact inline "name a section" field

/// The inline field that names a new section (filter row, so it is always on
/// screen). Return commits; Esc cancels (blur commits, the inline-edit
/// convention — Esc clears the draft first, so the commit is a no-op).
struct LigneSectionField: View {
    @Binding var text: String
    var palette: Palette
    var focus: FocusState<Bool>.Binding
    var onCommit: () -> Void
    var onCancel: () -> Void

    var body: some View {
        TextField("Section name", text: $text)
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
