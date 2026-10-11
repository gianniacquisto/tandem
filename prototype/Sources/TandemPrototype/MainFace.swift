import SwiftUI
import AppKit

// PROTOTYPE — the Main window in three reach variants (ticket 01, core
// question): how far does the Ligne Claire theme reach into the interface?
//
//   A — brand-only: stock native-clean interface; theme only in the wordmark,
//       icon, and accents.
//   B — subtle: theme in the chrome — paper surfaces, uniform linework,
//       line-drawn checkbox/filter, cel accents, illustrated empty state.
//   C — deeper: B + illustrated header band (sky, geometric hills, the bike),
//       panel-style list treatment, more illustrated moments.

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

struct MainFaceView: View {
    var model: TandemModel
    let reach: Reach
    let pal: Palette
    var showBanner: Bool = false

    @State private var filter: Filter = .open
    @State private var draft = ""

    var body: some View {
        switch reach {
        case .brandOnly: stock
        case .subtle:    themed(band: false)
        case .deeper:    themed(band: true)
        }
    }

    // MARK: visible list

    private var visible: [Todo] {
        switch filter {
        case .open: model.openTodos
        case .done: model.doneTodos
        case .all:  model.todos
        }
    }

    private var emptyCopy: (title: String, subtitle: String) {
        switch filter {
        case .open: ("All clear", "Nothing open — add something above.")
        case .done: ("Nothing done yet", "Completed todos rest here.")
        case .all:  ("The list is empty", "Add your first todo above.")
        }
    }

    // MARK: A — brand-only (stock interface)

    private var stock: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                BikeScene(palette: pal, riders: true)
                    .frame(width: 30, height: 19)
                Text("Tandem")
                    .font(.system(size: 17, weight: .semibold))
                Spacer()
                Button("Clear done") { model.clearDone() }
                    .controlSize(.small)
                    .disabled(model.doneTodos.isEmpty)
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)

            HStack {
                TextField("Add a todo…", text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 320)
                    .onSubmit { model.add(draft); draft = "" }
                Spacer()
                Picker("Filter", selection: $filter) {
                    ForEach(Filter.allCases) { f in
                        Text("\(f.rawValue) (\(count(for: f)))").tag(f)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 320)
            }
            .padding(.horizontal, 16)
            .padding(.top, 10)

            if showBanner {
                Label("The list was unreadable and was backed up to ~/.tandem/todos.json.corrupt-…",
                      systemImage: "exclamationmark.triangle")
                    .font(.system(size: 12))
                    .padding(10)
                    .background(RoundedRectangle(cornerRadius: 8).fill(Color.yellow.opacity(0.18)))
                    .padding(.horizontal, 16)
                    .padding(.top, 10)
            }

            Divider()
                .padding(.top, 12)

            stockList
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
                    Text(emptyCopy.title).font(.system(size: 14, weight: .medium))
                    Text(emptyCopy.subtitle).font(.system(size: 12)).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(visible.enumerated()), id: \.element.id) { i, todo in
                            stockRow(todo)
                            if i < visible.count - 1 {
                                Divider().padding(.leading, 41)
                            }
                        }
                    }
                }
            }
        }
        .frame(maxHeight: .infinity)
    }

    private func stockRow(_ todo: Todo) -> some View {
        StockRow(todo: todo,
                 onComplete: { model.complete(todo.id) },
                 onDelete: { model.delete(todo.id) })
    }

    // MARK: B / C — themed

    private func themed(band: Bool) -> some View {
        ZStack(alignment: .top) {
            pal.paper

            if band {
                VStack(spacing: 0) {
                    ZStack(alignment: .top) {
                        HeaderBand(palette: pal)
                            .frame(height: 106)
                        VStack(spacing: 10) {
                            HStack(spacing: 12) {
                                Wordmark(treatment: .lineLettering, palette: pal, height: 24)
                                Spacer()
                                CelButton(title: "Clear done", palette: pal,
                                          enabled: !model.doneTodos.isEmpty) { model.clearDone() }
                            }
                            LigneQuickAdd(text: $draft, palette: pal) { model.add(draft) }
                        }
                        .padding(.horizontal, 16)
                        .padding(.top, 12)
                    }
                    if showBanner {
                        LigneBanner(palette: pal).padding(.horizontal, 16).padding(.top, 10)
                    }
                    listArea(panel: true)
                        .padding(16)
                        .padding(.top, 12)
                }
            } else {
                VStack(spacing: 12) {
                    HStack(spacing: 12) {
                        Wordmark(treatment: .lineLettering, palette: pal, height: 26)
                        Spacer()
                        CelButton(title: "Clear done", palette: pal,
                                  enabled: !model.doneTodos.isEmpty) { model.clearDone() }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 16)

                    LigneQuickAdd(text: $draft, palette: pal) { model.add(draft) }
                        .padding(.horizontal, 16)

                    if showBanner {
                        LigneBanner(palette: pal).padding(.horizontal, 16)
                    }

                    listArea(panel: false)
                        .padding(.horizontal, 12)
                        .padding(.bottom, 14)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func listArea(panel: Bool) -> some View {
        let list: some View = Group {
            if visible.isEmpty {
                LigneEmptyState(palette: pal,
                                title: emptyCopy.title,
                                subtitle: emptyCopy.subtitle,
                                bikeWidth: panel ? 150 : 120)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(visible.enumerated()), id: \.element.id) { i, todo in
                            LigneRow(todo: todo, palette: pal, showsAge: true,
                                     onComplete: { model.complete(todo.id) },
                                     onDelete: { model.delete(todo.id) })
                            if i < visible.count - 1 {
                                Rectangle()
                                    .fill(pal.sand)
                                    .frame(height: Line.fine)
                                    .padding(.leading, 41)
                            }
                        }
                    }
                }
            }
        }

        return Group {
            if panel {
                list
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                    .background {
                        // hard cel shadow: flat offset, no blur, no gradient
                        RoundedRectangle(cornerRadius: 12)
                            .fill(pal.shadow)
                            .offset(x: 4, y: 4)
                    }
                    .background(RoundedRectangle(cornerRadius: 12).fill(pal.parchment))
                    .overlay(RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(pal.ink, lineWidth: Line.standard))
            } else {
                list
            }
        }
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
            }
            .buttonStyle(.plain)

            Text(todo.title)
                .font(.system(size: 13))
                .strikethrough(todo.isDone)
                .foregroundStyle(todo.isDone ? Color.secondary : .primary)
                .lineLimit(1)

            Spacer(minLength: 8)

            Text(ageString(todo.createdAt))
                .font(.system(size: 11, design: .rounded))
                .foregroundStyle(.tertiary)
                .monospacedDigit()

            if hovering {
                Button(action: onDelete) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 18, height: 18)
                        .background(Circle().strokeBorder(Color.secondary.opacity(0.4), lineWidth: 1))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 36)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            Button("Delete", role: .destructive, action: onDelete)
        }
    }
}
