import SwiftUI

// PROTOTYPE — shared Ligne Claire controls (ticket 01, v2 redo).
//
// The register at UI scale is restraint: one hairline (Line.hairline),
// flat fills, no drop shadows, native body typography, generous negative
// space. Every metric comes from Scale.swift — no literals here.

enum Filter: String, CaseIterable, Identifiable {
    case open = "Open"
    case done = "Done"
    case all = "All"
    var id: String { rawValue }
}

func ageString(_ date: Date) -> String {
    let s = max(0, Int(Date().timeIntervalSince(date)))
    if s < 90 { return "now" }
    let m = s / 60
    if m < 90 { return "\(m)m" }
    let h = m / 60
    if h < 36 { return "\(h)h" }
    return "\(h / 24)d"
}

// MARK: checkbox — flat, one hairline, one-way (v2 decision)

struct LigneCheckbox: View {
    var done: Bool
    var palette: Palette
    var size: CGFloat = 18

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: Radius.checkbox)
                .fill(done ? AnyShapeStyle(palette.moss) : AnyShapeStyle(palette.parchment))
            if done {
                Path { p in
                    p.move(to: CGPoint(x: 0.26, y: 0.52))
                    p.addLine(to: CGPoint(x: 0.42, y: 0.68))
                    p.addLine(to: CGPoint(x: 0.76, y: 0.34))
                }
                .path(in: CGRect(x: 0, y: 0, width: size, height: size))
                .stroke(palette.paper, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                .transition(.opacity)
            }
            RoundedRectangle(cornerRadius: Radius.checkbox)
                .strokeBorder(palette.ink, lineWidth: Line.hairline)
        }
        .frame(width: size, height: size)
    }
}

// MARK: segmented filter — flat selected fill, muted counts, no capsules

struct LigneFilter: View {
    @Binding var selection: Filter
    var counts: (open: Int, done: Int)
    var palette: Palette

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Filter.allCases) { f in
                segment(f)
            }
        }
        .frame(width: 240, height: 30)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.ink, lineWidth: Line.hairline))
    }

    private func segment(_ f: Filter) -> some View {
        let selected = f == selection
        let count = f == .open ? counts.open : f == .done ? counts.done : counts.open + counts.done
        return Button {
            withAnimation(Motion.toggle) { selection = f }
        } label: {
            HStack(spacing: 4) {
                Text(f.rawValue)
                    .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                    .foregroundStyle(selected ? palette.paper : palette.ink)
                Text("\(count)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(selected ? palette.paper.opacity(0.65) : palette.inkDim)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 30)
            .background(selected ? AnyShapeStyle(palette.dustyBlue) : AnyShapeStyle(Color.clear))
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
    @Environment(\.tandemExport) private var exportPreview

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(focus.wrappedValue ? palette.ember : palette.inkDim)
            ZStack(alignment: .leading) {
                if text.isEmpty || exportPreview {
                    Text("Add a todo…")
                        .font(.body)
                        .foregroundStyle(palette.inkDim)
                }
                if !exportPreview {
                    TextField("", text: $text)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .foregroundStyle(palette.ink)
                        .focused(focus)
                        .onSubmit {
                            onCommit()
                            text = ""
                        }
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
                .strokeBorder(focus.wrappedValue ? palette.ember : palette.ink,
                              lineWidth: Line.hairline)
        )
    }
}

// MARK: row — checkbox · title · age · hover delete (GUI-only)

struct LigneRow: View {
    var todo: Todo
    var palette: Palette
    var showsAge: Bool
    var onComplete: () -> Void
    var onDelete: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 10) {
            Button(action: onComplete) {
                LigneCheckbox(done: todo.isDone, palette: palette)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            Text(todo.title)
                .font(.body)
                .foregroundStyle(todo.isDone ? palette.inkDim : palette.ink)
                .strikethrough(todo.isDone, color: palette.inkDim)
                .lineLimit(1)

            Spacer(minLength: 8)

            if showsAge {
                Text(ageString(todo.createdAt))
                    .font(.caption)
                    .foregroundStyle(palette.inkDim)
                    .monospacedDigit()
            }

            // fixed slot so the age never shifts when the delete appears
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
            Button("Delete", role: .destructive, action: onDelete)
        }
    }
}

// MARK: empty states — the one illustrated moment in the main window

struct LigneEmptyState: View {
    var palette: Palette
    var title: String
    var subtitle: String
    /// Hero state ("All clear"): the riders, large, where they can read.
    var hero: Bool

    var body: some View {
        VStack(spacing: 18) {
            if hero {
                BikeScene(palette: palette, riders: true, ground: true)
                    .frame(width: 220, height: 220 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
            } else {
                BikeScene(palette: palette, riders: false, ground: true)
                    .frame(width: 96, height: 96 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
            }
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
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.dustyBlue))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: corrupt-store banner — restyled, no illustration (SF symbol only)

struct LigneBanner: View {
    var palette: Palette

    var body: some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 2)
                .fill(palette.ember)
                .frame(width: 4)
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(palette.emberDeep)
            VStack(alignment: .leading, spacing: 2) {
                Text("The list was unreadable, so it was backed up and started fresh.")
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("Backup: ~/.tandem/todos.json.corrupt-2026-10-11T004200Z")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.inkDim)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(RoundedRectangle(cornerRadius: Radius.card).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.card)
            .strokeBorder(palette.ink, lineWidth: Line.hairline))
    }
}
