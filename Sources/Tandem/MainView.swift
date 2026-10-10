import SwiftUI
import TandemCore

extension Filter {
    var displayName: String {
        switch self {
        case .open: return "Open"
        case .done: return "Done"
        case .all: return "All"
        }
    }
}

/// The main window (spec user stories 1–11) — the approved "variant A: filter tabs".
///
///   Header    — "Tandem" + "Clear done" (disabled when nothing is done)
///   Quick-add — Return commits
///   Filter    — segmented Open · Done · All with live counts
///   List      — creation order; tap a row to complete; hover for delete
struct MainView: View {
    @Environment(TandemModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    @State private var quickAdd = ""
    @State private var filter: Filter = .open
    @FocusState private var quickAddFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            quickAddField
            filterBar
            todoList
            recoveryBanner
            errorBanner
        }
        .frame(minWidth: 360, minHeight: 240)
        .onAppear {
            // Let the menu bar's "Open Tandem" focus (or reopen) this window.
            WindowManager.shared.openMain = { openWindow(id: "main") }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("Tandem")
                .font(.headline)
            Spacer()
            Button("Clear done") { model.clearDone() }
                .controlSize(.small)
                .disabled(model.doneTodos.isEmpty)
        }
        .padding(.horizontal, 14)
        .padding(.top, 12)
        .padding(.bottom, 8)
    }

    // MARK: - Quick add

    private var quickAddField: some View {
        HStack(spacing: 8) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tertiary)
            TextField("Add a todo…", text: $quickAdd)
                .textFieldStyle(.plain)
                .focused($quickAddFocused)
                .onSubmit(commitAdd)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }

    private func commitAdd() {
        let title = quickAdd.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        model.add(title)
        quickAdd = ""
        quickAddFocused = true  // keep focus for rapid entry
    }

    // MARK: - Filter

    private var filterBar: some View {
        Picker("Filter", selection: $filter) {
            ForEach(Filter.allCases, id: \.self) { f in
                Text("\(f.displayName) · \(count(f))").tag(f)
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .padding(.horizontal, 14)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }

    private func count(_ f: Filter) -> Int {
        switch f {
        case .open: return model.openTodos.count
        case .done: return model.doneTodos.count
        case .all: return model.todos.count
        }
    }

    private var visibleTodos: [Todo] {
        switch filter {
        case .open: return model.openTodos
        case .done: return model.doneTodos
        case .all: return model.todos
        }
    }

    // MARK: - List

    private var todoList: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(visibleTodos) { todo in
                    TodoRow(
                        todo: todo,
                        onComplete: { model.complete(todo.id.uuidString) },
                        onDelete: { model.delete(todo.id.uuidString) }
                    )
                    Divider()
                        .padding(.leading, 34)
                }
                if visibleTodos.isEmpty {
                    Text(emptyMessage)
                        .foregroundStyle(.tertiary)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 40)
                }
            }
        }
    }

    private var emptyMessage: String {
        switch filter {
        case .open: return model.todos.isEmpty ? "No todos yet — add one above." : "All clear — nothing open."
        case .done: return "Nothing done yet."
        case .all: return "No todos yet — add one above."
        }
    }

    // MARK: - Banners

    @ViewBuilder
    private var recoveryBanner: some View {
        if let note = model.recoveryNote {
            Banner(
                systemImage: "exclamationmark.triangle.fill",
                tint: .orange,
                title: "Store recovered",
                message: note,
                onDismiss: { model.dismissRecoveryNote() }
            )
        }
    }

    @ViewBuilder
    private var errorBanner: some View {
        if let error = model.lastError {
            Banner(
                systemImage: "xmark.circle.fill",
                tint: .red,
                title: nil,
                message: error,
                onDismiss: { model.dismissError() }
            )
        }
    }
}

/// A small dismissable status banner (recovery note or operation error).
private struct Banner: View {
    let systemImage: String
    let tint: Color
    let title: String?
    let message: String
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            VStack(alignment: .leading, spacing: 2) {
                if let title {
                    Text(title).font(.subheadline.weight(.medium))
                }
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tertiary)
        }
        .padding(10)
        .background(tint.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
    }
}
