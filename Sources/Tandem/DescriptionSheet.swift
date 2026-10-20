import SwiftUI
import TandemCore

/// A ticket's body, as it is stored: markdown text, shown and edited in one surface.
///
/// What you open is what you read — no rendering, so nothing can hide a character the
/// agent wrote — and Save is the CLI's own instruction (`set(id, description:)`), so the
/// file cannot tell which face wrote it. The store trims the edges and keeps an empty
/// body as `nil`; the sheet does not repeat those rules.
struct DescriptionSheet: View {
    var todo: Todo
    var palette: Palette
    /// Called with the text as typed. The caller writes it and dismisses.
    var onSave: (String) -> Void
    var onCancel: () -> Void

    @State private var draft: String
    @FocusState private var editorFocused: Bool

    init(todo: Todo, palette: Palette,
         onSave: @escaping (String) -> Void, onCancel: @escaping () -> Void) {
        self.todo = todo
        self.palette = palette
        self.onSave = onSave
        self.onCancel = onCancel
        _draft = State(initialValue: todo.description ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(todo.title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
                .lineLimit(2)
            Text("Markdown. Saved with the ticket, the same way the CLI writes it.")
                .font(.system(size: 11))
                .foregroundStyle(palette.inkDim)
            TextEditor(text: $draft)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(palette.ink)
                .scrollContentBackground(.hidden)
                .focused($editorFocused)
                .padding(8)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
            HStack(spacing: 10) {
                Text("⌘↩ saves · Esc cancels")
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                Spacer()
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Save") { onSave(draft) }
                    .keyboardShortcut(.return, modifiers: .command)
            }
        }
        .padding(20)
        .frame(width: 540, height: 440)
        .background(palette.paper)
        .onAppear { editorFocused = true }
    }
}
