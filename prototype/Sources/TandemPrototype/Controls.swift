import SwiftUI

// PROTOTYPE — shared Ligne Claire controls (used by reach B & C and the
// themed popover): line-drawn checkbox, line-drawn segmented filter,
// quick-add field, rows, empty state, corrupt-store banner.
//
// Hard constraints from the brief: flat fills, uniform linework, hard-edge
// (cel) shading only, no gradients, generous negative space.

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

// MARK: checkbox — line-drawn, one-way (v2 decision)

struct LigneCheckbox: View {
    var done: Bool
    var palette: Palette
    var size: CGFloat = 21

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 5.5)
                .fill(done ? AnyShapeStyle(palette.moss) : AnyShapeStyle(palette.parchment))
            RoundedRectangle(cornerRadius: 5.5)
                .strokeBorder(palette.ink, lineWidth: Line.standard)
            if done {
                Path { p in
                    p.move(to: CGPoint(x: 0.24, y: 0.52))
                    p.addLine(to: CGPoint(x: 0.42, y: 0.7))
                    p.addLine(to: CGPoint(x: 0.78, y: 0.32))
                }
                .path(in: CGRect(x: 0, y: 0, width: size, height: size))
                .stroke(palette.paper, style: StrokeStyle(lineWidth: 2.2, lineCap: .round, lineJoin: .round))
            }
        }
        .frame(width: size, height: size)
    }
}

// MARK: segmented filter — one line-drawn control, flat selected fill

struct LigneFilter: View {
    @Binding var selection: Filter
    var counts: (open: Int, done: Int)
    var palette: Palette

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Filter.allCases) { f in
                let selected = f == selection
                let count = f == .open ? counts.open : f == .done ? counts.done : counts.open + counts.done
                Button {
                    selection = f
                } label: {
                    HStack(spacing: 5) {
                        Text(f.rawValue)
                        Text("\(count)")
                            .font(.system(size: 11, weight: .medium, design: .rounded))
                            .padding(.horizontal, 5)
                            .padding(.vertical, 1)
                            .background(Capsule().fill(selected ? palette.paper : palette.sand))
                            .foregroundStyle(selected ? palette.dustyBlueDeep : palette.inkDim)
                    }
                    .font(.system(size: 12.5, weight: selected ? .semibold : .regular))
                    .foregroundStyle(selected ? palette.paper : palette.ink)
                    .padding(.horizontal, 11)
                    .frame(height: 28)
                    .background {
                        if selected {
                            // cel shade: hard-edge 2pt drop, no blur, no gradient
                            ZStack {
                                Rectangle().fill(palette.dustyBlueDeep)
                                    .offset(y: 2)
                                Rectangle().fill(palette.dustyBlue)
                            }
                        }
                    }
                }
                .buttonStyle(.plain)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(palette.ink, lineWidth: Line.standard))
    }
}

// MARK: quick-add — line-drawn field, ember focus

struct LigneQuickAdd: View {
    @Binding var text: String
    var palette: Palette
    var onCommit: () -> Void
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(focused ? palette.ember : palette.inkDim)
            TextField("Add a todo…", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
                .foregroundStyle(palette.ink)
                .focused($focused)
                .onSubmit {
                    onCommit()
                    text = ""
                }
        }
        .padding(.horizontal, 11)
        .frame(height: 34)
        .background(RoundedRectangle(cornerRadius: 8).fill(palette.parchment))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(focused ? palette.ember : palette.ink,
                              lineWidth: focused ? 2 : Line.standard)
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
            }
            .buttonStyle(.plain)

            Text(todo.title)
                .font(.system(size: 13))
                .foregroundStyle(todo.isDone ? palette.inkDim : palette.ink)
                .strikethrough(todo.isDone, color: palette.inkDim)
                .lineLimit(1)

            Spacer(minLength: 8)

            if showsAge {
                Text(ageString(todo.createdAt))
                    .font(.system(size: 11, design: .rounded))
                    .foregroundStyle(palette.inkDim)
                    .monospacedDigit()
            }

            if hovering {
                Button(action: onDelete) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.emberDeep)
                        .frame(width: 18, height: 18)
                        .background(Circle().strokeBorder(palette.ember, lineWidth: Line.fine))
                }
                .buttonStyle(.plain)
                .transition(.opacity)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 38)
        .background(hovering ? AnyShapeStyle(palette.sand.opacity(0.4)) : AnyShapeStyle(Color.clear))
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            Button("Delete", role: .destructive, action: onDelete)
        }
    }
}

// MARK: empty state — the bike in vast negative space

struct LigneEmptyState: View {
    var palette: Palette
    var title: String
    var subtitle: String
    var bikeWidth: CGFloat = 130

    var body: some View {
        VStack(spacing: 14) {
            BikeScene(palette: palette, riders: false)
                .frame(width: bikeWidth, height: bikeWidth * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
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
        .padding(.vertical, 30)
    }
}

// MARK: cel button — flat fill with hard 2pt offset (no blur, no gradient)

struct CelButton: View {
    var title: String
    var palette: Palette
    var enabled: Bool = true
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(enabled ? palette.paper : palette.inkDim)
                .padding(.horizontal, 12)
                .padding(.vertical, 6.5)
                .background {
                    if enabled {
                        ZStack {
                            RoundedRectangle(cornerRadius: 8).fill(palette.dustyBlueDeep).offset(y: 2)
                            RoundedRectangle(cornerRadius: 8).fill(palette.dustyBlue)
                        }
                    } else {
                        RoundedRectangle(cornerRadius: 8).fill(palette.sand)
                    }
                }
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(palette.ink, lineWidth: Line.fine))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

// MARK: corrupt-store banner (restyled; demo toggle in the floating bar)

struct LigneBanner: View {
    var palette: Palette

    var body: some View {
        HStack(spacing: 10) {
            Rectangle()
                .fill(palette.ember)
                .frame(width: 4)
            BikeScene(palette: palette, riders: false)
                .frame(width: 34, height: 34 * 0.65)
            VStack(alignment: .leading, spacing: 2) {
                Text("The list was unreadable, so it was backed up and started fresh.")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("Backup: ~/.tandem/todos.json.corrupt-2026-10-11T004200Z")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.inkDim)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(palette.ink, lineWidth: Line.standard))
    }
}
