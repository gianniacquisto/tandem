import SwiftUI
import TandemCore

/// A single todo row in the main window (spec user stories 3, 5, 8, 10).
///
/// Checkbox glyph + title (strikethrough + dimmed when done) + relative creation time
/// trailing. Tapping the row (or the checkbox) completes the todo; a delete affordance
/// appears on hover (GUI-only — agents have no delete in v1).
struct TodoRow: View {
    let todo: Todo
    let onComplete: () -> Void
    let onDelete: () -> Void

    @State private var hovered = false

    var body: some View {
        HStack(spacing: 10) {
            Button(action: onComplete) {
                Image(systemName: todo.done ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 16))
                    .foregroundStyle(todo.done ? Color.accentColor : .secondary)
            }
            .buttonStyle(.plain)

            Text(todo.title)
                .strikethrough(todo.done, color: .secondary)
                .foregroundStyle(todo.done ? .secondary : .primary)
                .lineLimit(2)

            Spacer(minLength: 8)

            Text(todo.createdAt, format: .relative(presentation: .named))
                .font(.caption)
                .foregroundStyle(.tertiary)

            // Delete affordance: only visible (and only clickable) while hovering, so
            // it never steals a tap meant for the row's complete action.
            Button(action: onDelete) {
                Image(systemName: "trash")
                    .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.red)
            .opacity(hovered ? 1 : 0)
            .allowsHitTesting(hovered)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .onTapGesture(perform: onComplete)
        .onHover { hovered = $0 }
    }
}
