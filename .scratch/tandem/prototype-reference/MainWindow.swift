import SwiftUI

// PROTOTYPE — throwaway, not the real app.
//
// Question (ticket 04): "What should Tandem look like?"
// Three structurally different variants of the main window, cycled with ←/→
// or the floating bar at the bottom of the window, plus a live menu bar item
// stub to judge in the real menu bar. The icon glyph/badge are surfaced
// (cycle them in the bar), not picked — ticket 04 keeps that decision open.

// MARK: - Variant / icon option state

enum WindowVariant: Int, CaseIterable {
    case a, b, c

    var key: String { ["A", "B", "C"][rawValue] }

    var name: String {
        switch self {
        case .a: "Filter tabs"
        case .b: "Sections"
        case .c: "Search-first"
        }
    }

    var blurb: String {
        switch self {
        case .a: "segmented open/done/all · creation order · no search"
        case .b: "open/done sections, pinned headers · done newest-completed · no search"
        case .c: "search on top · single flat list · recency order"
        }
    }

    func moved(_ delta: Int) -> WindowVariant {
        let n = WindowVariant.allCases.count
        return WindowVariant(rawValue: (rawValue + delta + n * 10) % n)!
    }
}

enum GlyphOption: Int, CaseIterable {
    case checkmarkCircle, checklist, listBullet

    var symbol: String {
        switch self {
        case .checkmarkCircle: "checkmark.circle"
        case .checklist: "checklist"
        case .listBullet: "list.bullet"
        }
    }
}

@Observable
final class IconState {
    var glyphIndex = 0 { didSet { NotificationCenter.default.post(name: .tandemChanged, object: nil) } }
    var badged = true { didSet { NotificationCenter.default.post(name: .tandemChanged, object: nil) } }

    var glyph: GlyphOption { GlyphOption.allCases[glyphIndex] }

    func stepGlyph(_ delta: Int) {
        let n = GlyphOption.allCases.count
        glyphIndex = (glyphIndex + delta + n * 10) % n
    }
}

// MARK: - Root

struct RootView: View {
    var model: TandemModel
    var icon: IconState
    @Binding var variant: WindowVariant

    var body: some View {
        Group {
            switch variant {
            case .a: VariantA(model: model)
            case .b: VariantB(model: model)
            case .c: VariantC(model: model)
            }
        }
        .frame(minWidth: 430, idealWidth: 470, minHeight: 500, idealHeight: 600)
        .modifier(ArrowCycler(variant: $variant))
        .overlay(alignment: .bottom) {
            PrototypeSwitcher(variant: $variant, icon: icon)
        }
    }
}

/// ←/→ cycle window variants; typing in any field wins, and modifier
/// combos (⌘/⌃/⌥) are left alone.
private struct ArrowCycler: ViewModifier {
    @Binding var variant: WindowVariant
    @State private var monitor: Any?

    func body(content: Content) -> some View {
        content.onAppear {
            guard monitor == nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                let fr = NSApp.keyWindow?.firstResponder
                if fr is NSText || fr is NSSearchField { return event }
                if event.modifierFlags.intersection([.command, .control, .option]).isEmpty == false { return event }
                switch event.keyCode {
                case 123: variant = variant.moved(-1); return nil
                case 124: variant = variant.moved(1); return nil
                default: return event
                }
            }
        }
    }
}

// MARK: - Shared bits (deliberately small: each variant keeps its own layout)

struct QuickAdd: View {
    var model: TandemModel
    @State private var text = ""

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus.circle.fill").foregroundStyle(.tertiary)
            TextField("Add a todo…", text: $text)
                .onSubmit {
                    model.add(text)
                    text = ""
                }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }
}

// MARK: - Variant A: filter tabs

struct VariantA: View {
    var model: TandemModel
    @State private var filter: Filter = .open

    enum Filter: String, CaseIterable, Identifiable {
        case open = "Open", done = "Done", all = "All"
        var id: String { rawValue }
    }

    private var visible: [Todo] {
        switch filter {
        case .open: model.openTodos
        case .done: model.doneTodos
        case .all: model.todos
        }
    }

    private func count(_ f: Filter) -> Int {
        switch f {
        case .open: model.openTodos.count
        case .done: model.doneTodos.count
        case .all: model.todos.count
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Tandem").font(.title3.bold())
                Spacer()
                Button("Clear done", action: model.clearDone)
                    .disabled(model.doneTodos.isEmpty)
                    .font(.callout)
            }
            .padding([.horizontal, .top], 14)
            .padding(.bottom, 8)

            QuickAdd(model: model)

            Picker("Filter", selection: $filter) {
                ForEach(Filter.allCases) { f in
                    Text("\(f.rawValue) · \(count(f))").tag(f)
                }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, 14)
            .padding(.bottom, 8)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(visible) { todo in
                        Button { model.toggle(todo.id) } label: {
                            HStack(spacing: 8) {
                                Image(systemName: todo.isDone ? "checkmark.circle.fill" : "circle")
                                    .foregroundStyle(todo.isDone ? Color.accentColor : .secondary)
                                Text(todo.title)
                                    .strikethrough(todo.isDone)
                                    .foregroundStyle(todo.isDone ? .secondary : .primary)
                                Spacer()
                                Text(todo.createdAt, format: .relative(presentation: .named))
                                    .font(.caption)
                                    .foregroundStyle(.tertiary)
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        Divider().padding(.leading, 14)
                    }
                    if visible.isEmpty {
                        Text("Nothing here")
                            .foregroundStyle(.tertiary)
                            .frame(maxWidth: .infinity)
                            .padding(.top, 40)
                    }
                }
            }
        }
    }
}

// MARK: - Variant B: sections

struct VariantB: View {
    var model: TandemModel

    private var doneNewestFirst: [Todo] {
        model.doneTodos.sorted { ($0.completedAt ?? .distantPast) > ($1.completedAt ?? .distantPast) }
    }

    var body: some View {
        VStack(spacing: 0) {
            QuickAdd(model: model)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                    Section {
                        ForEach(model.openTodos) { todo in
                            row(todo)
                            Divider().padding(.leading, 36)
                        }
                    } header: {
                        header("Open", count: model.openTodos.count)
                    }

                    if !model.doneTodos.isEmpty {
                        Section {
                            ForEach(doneNewestFirst) { todo in
                                row(todo)
                                Divider().padding(.leading, 36)
                            }
                        } header: {
                            HStack {
                                header("Done", count: model.doneTodos.count)
                                Spacer()
                                Button("Clear", action: model.clearDone)
                                    .font(.callout)
                                    .padding(.trailing, 14)
                            }
                        }
                    }
                }
            }
        }
    }

    private func header(_ title: String, count: Int) -> some View {
        HStack(spacing: 6) {
            Text(title).font(.headline)
            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 6)
                .padding(.vertical, 1)
                .background(.quaternary, in: Capsule())
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .background(.bar)
    }

    private func row(_ todo: Todo) -> some View {
        Button { model.toggle(todo.id) } label: {
            HStack(spacing: 10) {
                Image(systemName: todo.isDone ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(todo.isDone ? Color.accentColor : .secondary)
                Text(todo.title)
                    .strikethrough(todo.isDone)
                    .foregroundStyle(todo.isDone ? .secondary : .primary)
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Variant C: search-first

struct VariantC: View {
    var model: TandemModel
    @State private var query = ""
    @State private var newTitle = ""

    private var visible: [Todo] {
        let q = query.trimmingCharacters(in: .whitespaces)
        let base = q.isEmpty ? model.todos : model.todos.filter { $0.title.localizedCaseInsensitiveContains(q) }
        return base.sorted { $0.updatedAt > $1.updatedAt }
    }

    var body: some View {
        VStack(spacing: 10) {
            TextField("Search…", text: $query)
                .textFieldStyle(.roundedBorder)
                .padding(.horizontal, 14)
                .padding(.top, 12)

            HStack(spacing: 8) {
                TextField("Add a todo…", text: $newTitle)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(add)
                Button("Add", action: add)
                    .disabled(newTitle.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.horizontal, 14)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(visible) { todo in
                        Button { model.toggle(todo.id) } label: {
                            HStack(spacing: 8) {
                                Image(systemName: todo.isDone ? "checkmark.circle.fill" : "circle")
                                    .foregroundStyle(todo.isDone ? Color.accentColor : .secondary)
                                Text(todo.title)
                                    .strikethrough(todo.isDone)
                                    .foregroundStyle(todo.isDone ? .secondary : .primary)
                                Spacer()
                                Text(todo.updatedAt, format: .relative(presentation: .named))
                                    .font(.caption)
                                    .foregroundStyle(.tertiary)
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        Divider().padding(.leading, 14)
                    }
                    if visible.isEmpty {
                        Text(query.isEmpty ? "Nothing here" : "No matches for “\(query)”")
                            .foregroundStyle(.tertiary)
                            .frame(maxWidth: .infinity)
                            .padding(.top, 40)
                    }
                }
            }
        }
        .padding(.bottom, 8)
    }

    private func add() {
        model.add(newTitle)
        newTitle = ""
    }
}

// MARK: - Floating prototype switcher (obviously not part of the design)

struct PrototypeSwitcher: View {
    @Binding var variant: WindowVariant
    var icon: IconState

    var body: some View {
        VStack(spacing: 6) {
            Text(variant.blurb)
                .font(.caption2)
                .foregroundStyle(.white.opacity(0.7))

            HStack(spacing: 8) {
                arrow("chevron.left") { variant = variant.moved(-1) }
                Text("\(variant.key) · \(variant.name)")
                    .font(.caption.monospaced().bold())
                arrow("chevron.right") { variant = variant.moved(1) }

                Divider().frame(height: 14)

                arrow("chevron.left") { icon.stepGlyph(-1) }
                Image(systemName: icon.glyph.symbol).font(.system(size: 11))
                arrow("chevron.right") { icon.stepGlyph(1) }
                Button(icon.badged ? "badge ✓" : "plain") { icon.badged.toggle() }
                    .buttonStyle(.plain)
                    .font(.caption.monospaced())
                    .foregroundStyle(icon.badged ? .white : .white.opacity(0.45))

                Text("menu bar ↑").font(.caption2).foregroundStyle(.white.opacity(0.45))
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.black.opacity(0.88), in: RoundedRectangle(cornerRadius: 12))
        .foregroundStyle(.white)
        .padding(.bottom, 10)
    }

    private func arrow(_ symbol: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.caption.bold())
        }
        .buttonStyle(.plain)
    }
}
