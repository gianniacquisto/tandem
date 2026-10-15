import SwiftUI
import AppKit

// PROTOTYPE — the Main window in three reach variants (ticket 01, v2 redo).
//
//   A — brand-only: stock native-clean interface (the control), system
//       colors, plain-text empty state.
//   B — subtle: theme in the chrome — paper surfaces, one hairline, flat
//       filled controls, native body typography, illustrated empty state.
//   C — deeper: B plus the quiet illustrated header band.
//
// The theme's reach is restrained by design: one shared line weight, flat
// fills, no shadows, illustration in at most two places (header band +
// empty state), native body typography throughout.

enum Reach: Int, CaseIterable, Identifiable {
    case brandOnly, subtle, deeper
    var id: Int { rawValue }

    var short: String {
        switch self {
        case .brandOnly: "A · Brand-only"
        case .subtle:    "B · Subtle"
        case .deeper:    "C · Deeper"
        }
    }
}

enum Face: String, CaseIterable, Identifiable {
    case main = "Main window"
    case popover = "Popover"
    case assets = "Assets"
    var id: String { rawValue }
}

/// Settled default UX (map, 2026-10-11): ⌘1/⌘2/⌘3 switch the filter,
/// ⌘N focuses quick-add. Registered once, applies to every reach variant.
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

struct MainFaceView: View {
    var model: TandemModel
    let reach: Reach
    let pal: Palette
    var showBanner: Bool = false
    /// The bikeshed tab (ticket 02): a second tab in the main window —
    /// Todos | Bikeshed. The menu bar item stays todos-only (map).
    var bike: BikeModel
    @Binding var tab: MainTab

    @State private var filter: Filter = .open
    @State private var draft = ""
    @FocusState private var quickAddFocus: Bool
    @Environment(\.tandemExport) private var exportPreview

    var body: some View {
        Group {
            switch reach {
            case .brandOnly: stock
            case .subtle:    themed(band: false)
            case .deeper:    themed(band: true)
            }
        }
        .modifier(TandemShortcuts(filter: $filter, focusQuickAdd: $quickAddFocus))
    }

    // MARK: visible list

    private var visible: [Todo] {
        switch filter {
        case .open: model.openTodos
        case .done: model.doneTodos
        case .all:  model.todos
        }
    }

    // MARK: A — brand-only (stock interface, the control)

    private var stock: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Text("Tandem")
                    .font(.headline)
                Picker("Tab", selection: $tab) {
                    Text("Todos").tag(MainTab.todos)
                    Text("Bikeshed").tag(MainTab.bikeshed)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 190)
                Spacer()
                if tab == .todos {
                    Button("Clear done") {
                        withAnimation(Motion.remove) { model.clearDone() }
                    }
                    .controlSize(.small)
                    .disabled(model.doneTodos.isEmpty)
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 8)

            Divider()

            if tab == .bikeshed {
                BikeshedFaceView(model: bike, pal: pal)
            } else {
            HStack(spacing: 8) {
                Image(systemName: "plus.circle.fill")
                    .foregroundStyle(.tertiary)
                if exportPreview {
                    Text("Add a todo…")
                        .font(.body)
                        .foregroundStyle(.secondary)
                } else {
                    TextField("Add a todo…", text: $draft)
                        .textFieldStyle(.plain)
                        .focused($quickAddFocus)
                        .onSubmit {
                            withAnimation(Motion.insert) { model.add(draft) }
                            draft = ""
                        }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 8)

            Group {
                if exportPreview {
                    // ImageRenderer mangles the native segmented control
                    // (renders it as a system-warning-yellow band with a red
                    // caret). The export gets a flat approximation — the
                    // running app keeps the real Picker.
                    HStack(spacing: 1) {
                        ForEach(Filter.allCases) { f in
                            let selected = filter == f
                            Text("\(f.rawValue) · \(count(for: f))")
                                .font(.system(size: 11))
                                .lineLimit(1)
                                .foregroundStyle(selected ? Color.primary : Color.secondary)
                                .frame(maxWidth: .infinity)
                                .frame(height: 22)
                                .background(selected ? Color.primary.opacity(0.10) : Color.clear)
                        }
                    }
                    .background(RoundedRectangle(cornerRadius: 6).fill(Color.primary.opacity(0.05)))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                } else {
                    Picker("Filter", selection: $filter) {
                        ForEach(Filter.allCases) { f in
                            Text("\(f.rawValue) · \(count(for: f))").tag(f)
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 4)
            .padding(.bottom, 8)

            if showBanner {
                Label("The list was unreadable and was backed up to ~/.tandem/todos.json.corrupt-…",
                      systemImage: "exclamationmark.triangle")
                    .font(.system(size: 12))
                    .padding(10)
                    .background(RoundedRectangle(cornerRadius: Radius.card).fill(Color.yellow.opacity(0.18)))
                    .padding(.horizontal, 14)
                    .padding(.top, 4)
            }

            stockList
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func count(for f: Filter) -> Int {
        switch f {
        case .open: model.openTodos.count
        case .done: model.doneTodos.count
        case .all:  model.todos.count
        }
    }

    private var stockList: some View {
        Group {
            if visible.isEmpty {
                VStack(spacing: 4) {
                    Text(stockEmptyTitle).font(.system(size: 14, weight: .medium))
                    Text(stockEmptySubtitle).font(.system(size: 12)).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(visible) { todo in
                            StockRow(todo: todo,
                                     onComplete: { withAnimation(Motion.toggle) { model.complete(todo.id) } },
                                     onDelete: { withAnimation(Motion.remove) { model.delete(todo.id) } })
                                .transition(.opacity)
                            if todo.id != visible.last?.id {
                                Divider().padding(.leading, Space.gutter + 24 + 10)
                            }
                        }
                    }
                }
            }
        }
        .frame(maxHeight: .infinity)
    }

    private var stockEmptyTitle: String {
        switch filter {
        case .open: "All clear"
        case .done: "Nothing done yet"
        case .all:  "The list is empty"
        }
    }

    private var stockEmptySubtitle: String {
        switch filter {
        case .open: "Nothing open — add something above."
        case .done: "Completed todos rest here."
        case .all:  "Add your first todo above."
        }
    }

    // MARK: B / C — themed

    private func themed(band: Bool) -> some View {
        VStack(spacing: 0) {
            if band {
                HeaderBand(palette: pal)
            }

            HStack(spacing: 12) {
                Text("Tandem")
                    .font(.custom("AvenirNext-DemiBold", size: 15))
                    .tracking(0.3)
                    .foregroundStyle(pal.ink)
                BikeTabControl(tab: $tab, pal: pal)
                Spacer()
                if tab == .todos {
                    TextButton(title: "Clear done", palette: pal,
                               enabled: !model.doneTodos.isEmpty) {
                        withAnimation(Motion.remove) { model.clearDone() }
                    }
                }
            }
            .padding(.horizontal, Space.gutter)
            .padding(.top, band ? 14 : 18)

            if tab == .todos {
                todosContent
            } else {
                BikeshedFaceView(model: bike, pal: pal)
            }
        }
        .background(pal.paper)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// The todos tab content. The shared window is wider than the old 470pt
    /// todos window, so the todos column is centred at 540pt — provisional
    /// until the human reacts (ticket 02 open question: window sizing).
    private var todosContent: some View {
        VStack(spacing: 0) {
            LigneQuickAdd(text: $draft, palette: pal,
                          onCommit: {
                              withAnimation(Motion.insert) { model.add(draft) }
                              draft = ""
                          },
                          focus: $quickAddFocus)
            .padding(.horizontal, Space.gutter)
            .padding(.top, 12)

            LigneFilter(selection: $filter,
                        counts: (open: model.openTodos.count, done: model.doneTodos.count),
                        palette: pal)
                .padding(.horizontal, Space.gutter)
                .padding(.top, Space.section)

            if showBanner {
                LigneBanner(palette: pal)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Space.gutter)
                    .padding(.top, Space.section)
            }

            Rectangle().fill(pal.sand).frame(height: Line.hairline)
                .padding(.top, Space.section)

            listArea
        }
        .frame(maxWidth: 540)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private var listArea: some View {
        Group {
            if visible.isEmpty {
                Group {
                    switch filter {
                    case .open:
                        LigneEmptyState(palette: pal, title: "All clear",
                                        subtitle: "Nothing open — add something above.", hero: true)
                    case .done:
                        LigneEmptyState(palette: pal, title: "Nothing done yet",
                                        subtitle: "Completed todos rest here.", hero: false)
                    case .all:
                        LigneEmptyState(palette: pal, title: "The list is empty",
                                        subtitle: "Add your first todo above.", hero: false)
                    }
                }
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(visible) { todo in
                            LigneRow(todo: todo, palette: pal, showsAge: true,
                                     onComplete: { withAnimation(Motion.toggle) { model.complete(todo.id) } },
                                     onDelete: { withAnimation(Motion.remove) { model.delete(todo.id) } })
                                .transition(.opacity)
                            if todo.id != visible.last?.id {
                                Rectangle()
                                    .fill(pal.sand)
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
}

/// Variant A's stock row (native controls, no theme).
struct StockRow: View {
    var todo: Todo
    var onComplete: () -> Void
    var onDelete: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 10) {
            Button(action: onComplete) {
                Image(systemName: todo.isDone ? "checkmark.square.fill" : "square")
                    .font(.system(size: 14))
                    .foregroundStyle(todo.isDone ? Color.accentColor : .secondary)
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .contentShape(Rectangle())

            Text(todo.title)
                .font(.body)
                .strikethrough(todo.isDone)
                .foregroundStyle(todo.isDone ? Color.secondary : .primary)
                .lineLimit(1)

            Spacer(minLength: 8)

            Text(ageString(todo.createdAt))
                .font(.caption)
                .foregroundStyle(.tertiary)
                .monospacedDigit()

            ZStack {
                if hovering {
                    Button(action: onDelete) {
                        Image(systemName: "xmark")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(.secondary)
                            .frame(width: 18, height: 18)
                            .background(Circle().strokeBorder(Color.secondary.opacity(0.4), lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                    .transition(.opacity)
                }
            }
            .frame(width: 22, height: 22)
        }
        .padding(.horizontal, Space.gutter)
        .frame(height: Space.row)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            Button("Delete", role: .destructive, action: onDelete)
        }
    }
}
