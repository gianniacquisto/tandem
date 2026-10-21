import SwiftUI
import Foundation
import TandemCore

/// The whole ticket in one sheet: title, Status, Cycle lane, the body, and the
/// ticket's discussion.
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
///
/// The comments are the sidecar's log, read for this ticket and nothing else: the
/// conversation agents have on a ticket after refinement, which the description (the
/// refinement's output) is not. The sheet draws them oldest first; it never writes
/// them (ticket `3A44B7FD` is the write half).
struct TicketSheet: View {
    var todo: Todo
    var palette: Palette
    var lanes: [String]
    /// The ticket's discussion, as the sidecar holds it. `.list([])` is a ticket with
    /// no log: no section, no error.
    var comments: TicketComments = .list([])
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
         comments: TicketComments = .list([]),
         onSave: @escaping (TicketEdits) -> Void,
         onCancel: @escaping () -> Void,
         onOpenLink: ((String) -> Void)? = nil,
         onPromote: (() -> PromotionOutcome)? = nil) {
        self.todo = todo
        self.palette = palette
        self.lanes = lanes
        self.comments = comments
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

    /// Whether the sheet draws a comments section: a log with something in it, or a
    /// log that could not be read (which says so where the list would be). A ticket
    /// nobody has commented on draws nothing — an empty box would be noise.
    private var showsComments: Bool {
        switch comments {
        case .list(let list): return !list.isEmpty
        case .failed: return true
        }
    }

    /// The sheet grows to make room for the discussion: a fixed height would squeeze
    /// the description editor to nothing on a ticket that has one.
    private var sheetHeight: CGFloat { showsComments ? 640 : 500 }

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

            // The discussion, under the body: the log the CLI's `todo comment add`
            // appends to. The sheet draws it; it never writes it.
            if showsComments {
                commentsSection
            }

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
        .frame(width: 580, height: sheetHeight)
        .background(palette.paper)
        .onAppear {
            // A ticket with a body opens for reading it; a terse one opens for writing it.
            focus = todo.description == nil ? .title : .body
        }
    }

    // MARK: - The discussion (the sidecar's log, read-only)

    /// The ticket's comments: a header, and either the log itself or the reason it
    /// could not be read. The two never share a shape — an empty log is nothing to
    /// say, a damaged one is something to repair.
    @ViewBuilder
    private var commentsSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Comments — the ticket's discussion log")
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
            switch comments {
            case .list(let list):
                commentList(list)
            case .failed(let message):
                // The core's own words, which name the damaged file: the log is
                // append-only, so a bad line is repair, not recovery.
                Text(message)
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// The log, oldest first, in a scrollable parchment panel.
    private func commentList(_ list: [Comment]) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(list.enumerated()), id: \.offset) { index, comment in
                    commentRow(comment)
                    if index < list.count - 1 {
                        Rectangle().fill(palette.sand).frame(height: Line.hairline)
                    }
                }
            }
            .padding(.vertical, 2)
        }
        .frame(maxHeight: 190)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    /// One comment: who said it, when, and what they said. The body is markdown as
    /// stored, like the description — no rendering, so nothing can hide a character
    /// an agent wrote.
    private func commentRow(_ comment: Comment) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                // An unnamed author is the empty string, and it says so rather than
                // leaving a gap where a name would be.
                Text(comment.author.isEmpty ? "anonymous" : comment.author)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(comment.author.isEmpty ? palette.inkDim : palette.ink)
                Text(comment.at.formatted(
                    .dateTime.year().month(.abbreviated).day().hour().minute()))
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
            }
            Text(comment.body)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(palette.ink)
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
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
