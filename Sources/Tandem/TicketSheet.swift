import SwiftUI
import TandemCore

/// The whole ticket in one sheet: title, Status, Cycle lane, and the body.
///
/// Opening a ticket is the one gesture that was missing — a row and a tile could be
/// changed in place, but there was nowhere to *read* one whole or to change its title,
/// stage, and lane together. Double-clicking a ticket (anywhere but its title, which keeps
/// inline rename) opens this; so does "Open ticket…" in the menu.
///
/// The body is markdown as stored — no rendering, so nothing can hide a character an agent
/// wrote — and Save is the same instruction the CLI's `set` makes, not a parallel write.
/// Assignee is read-only here: assignment is an agent's instruction, and the inbox is where
/// the human answers it. Origin is read-only for the stronger reason: it is stamped at
/// add and never edited, so the sheet only shows who filed the ticket — "you filed", or
/// the agent's name — which is how triage starts with "what did I ask for?"
struct TicketSheet: View {
    var todo: Todo
    var palette: Palette
    var lanes: [String]
    /// Called with the sheet's values; the caller writes what changed and dismisses.
    var onSave: (TicketEdits) -> Void
    var onCancel: () -> Void
    /// Opens a linked note (the window owns the tab switch). `nil` hides the button.
    var onOpenLink: ((String) -> Void)? = nil
    /// Promotes the ticket to a bikeshed note. `nil` hides the button.
    var onPromote: (() -> PromotionOutcome)? = nil

    @State private var title: String
    @State private var status: Status
    @State private var lane: String?
    @State private var bodyText: String
    /// The ticket's note link. Seeded from the ticket, then owned by the sheet: a
    /// promotion writes a new link the open ticket does not know about yet.
    @State private var link: String?
    /// Why the last promotion did not happen.
    @State private var promoteError: String?
    @FocusState private var focus: Field?

    private enum Field { case title, body }

    init(todo: Todo, palette: Palette, lanes: [String],
         onSave: @escaping (TicketEdits) -> Void,
         onCancel: @escaping () -> Void,
         onOpenLink: ((String) -> Void)? = nil,
         onPromote: (() -> PromotionOutcome)? = nil) {
        self.todo = todo
        self.palette = palette
        self.lanes = lanes
        self.onSave = onSave
        self.onCancel = onCancel
        self.onOpenLink = onOpenLink
        self.onPromote = onPromote
        _title = State(initialValue: todo.title)
        _status = State(initialValue: todo.status)
        _lane = State(initialValue: todo.lane)
        _bodyText = State(initialValue: todo.description ?? "")
        _link = State(initialValue: todo.link)
    }

    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canSave: Bool { !trimmedTitle.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextField("", text: $title)
                .textFieldStyle(.plain)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(palette.ink)
                .focused($focus, equals: .title)
                .padding(.horizontal, 9)
                .padding(.vertical, 7)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))

            HStack(spacing: 14) {
                picker("Status", value: status.displayName) {
                    ForEach(Status.workflow, id: \.self) { s in
                        choose(s.displayName, selected: s == status) { status = s }
                    }
                }
                picker("Lane", value: lane ?? "Uncategorized") {
                    choose("Uncategorized", selected: lane == nil) { lane = nil }
                    if !lanes.isEmpty { Divider() }
                    ForEach(lanes, id: \.self) { name in
                        choose(name, selected: lane == name) { lane = name }
                    }
                }
                if let assignee = todo.assignee {
                    Text(assignee == TodoQuery.human ? "human" : "@\(assignee)")
                        .font(.system(size: 11))
                        .foregroundStyle(assignee == TodoQuery.human ? palette.emberDeep
                                                                     : palette.inkDim)
                }
                // The filer, read-only (the type will not let a write touch it):
                // `human` reads as "you", an agent as its name. The agent's filing is
                // the one worth the colour — it is the signal triage starts from.
                Text(todo.origin == TodoQuery.human ? "you filed" : "@\(todo.origin) filed")
                    .font(.system(size: 11))
                    .foregroundStyle(todo.origin == TodoQuery.human ? palette.inkDim
                                                                    : palette.emberDeep)
                Spacer(minLength: 0)
                if let link {
                    if let onOpenLink {
                        Button("Open note") { onOpenLink(link) }
                            .buttonStyle(.plain)
                            .font(.system(size: 11))
                            .foregroundStyle(palette.inkDim)
                            .help(link)
                    }
                } else if onPromote != nil {
                    Button("Promote to note") { promote() }
                        .buttonStyle(.plain)
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                        .help("Make a bikeshed note from this ticket and link the two")
                }
            }

            // A refusal says why here, where the button was — the sheet answers
            // the action, it does not raise an alarm.
            if let promoteError {
                Text(promoteError)
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Rectangle().fill(palette.sand).frame(height: Line.hairline)

            Text("Description — markdown, saved with the ticket")
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
            TextEditor(text: $bodyText)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(palette.ink)
                .scrollContentBackground(.hidden)
                .focused($focus, equals: .body)
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
                Button("Save") {
                    onSave(TicketEdits(title: trimmedTitle,
                                       description: bodyText,
                                       status: status,
                                       lane: lane))
                }
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(!canSave)
            }
        }
        .padding(20)
        .frame(width: 580, height: 500)
        .background(palette.paper)
        .onAppear {
            // A ticket with a body opens for reading it; a terse one opens for writing it.
            focus = todo.description == nil ? .title : .body
        }
    }

    /// Promotes the ticket to a note. On success the sheet now shows "Open note"
    /// pointing at the new note; on a refusal the sheet stays as it was and says
    /// why nothing was written.
    private func promote() {
        guard let onPromote else { return }
        switch onPromote() {
        case .promoted(let path):
            link = path
            promoteError = nil
        case .refused(let message):
            promoteError = message
        }
    }

    /// A quiet "label · value ⌄" chip, the sheet's one control shape.
    @ViewBuilder
    private func picker<Content: View>(_ label: String, value: String,
                                       @ViewBuilder menu: () -> Content) -> some View {
        Menu {
            menu()
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                Text(value)
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                Image(systemName: "chevron.down")
                    .font(.system(size: 7.5, weight: .semibold))
                    .foregroundStyle(palette.inkDim)
            }
            .padding(.horizontal, 7)
            .frame(height: 22)
            .overlay(Capsule().strokeBorder(palette.line, lineWidth: Line.hairline))
            .contentShape(Capsule())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    @ViewBuilder
    private func choose(_ label: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            if selected {
                Label(label, systemImage: "checkmark")
            } else {
                Text(label)
            }
        }
    }
}

/// The field changes a ticket sheet proposes. The caller compares them with the stored
/// ticket and writes only what moved — a sheet that was opened, read, and saved is free.
struct TicketEdits {
    var title: String
    var description: String
    var status: Status
    var lane: String?
}
