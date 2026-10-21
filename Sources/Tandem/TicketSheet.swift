import SwiftUI
import Foundation
import AppKit
import Combine
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
/// The comments are the sidecar's log: the conversation agents have on a ticket after
/// refinement, which the description (the refinement's output) is not. The sheet draws
/// them oldest first, and it writes them too — the composer at the foot of the
/// discussion is the human's own half of the log, appended to the same sidecar the
/// CLI's `todo comment add` writes.
struct TicketSheet: View {
    var ticket: Ticket
    var palette: Palette
    var lanes: [String]
    /// Called with the sheet's values; the caller writes what changed and dismisses.
    var onSave: (TicketEdits) -> Void
    var onCancel: () -> Void
    /// Posts the human's comment to the ticket's log and hands back how it ended:
    /// the log as it now reads, or the refusal that kept it as it was.
    var onComment: (String) -> CommentPostOutcome
    /// Reads the ticket's log again — the sheet's **refresh**.
    ///
    /// It is a read of its own, not the write's read-back: the write path takes
    /// the store's lock once and hands back the record it wrote, and this is
    /// what puts the rest of the log in front of the human — a comment another
    /// actor appended while the sheet was open. The read carries its own
    /// answers, and they belong to the read: a `.busy` leaves the list exactly
    /// as it was and says the list may be behind, because a read that did not
    /// happen takes nothing away.
    var onRefreshComments: () -> CommentRead
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
    /// The ticket's discussion, as the sheet draws it. Seeded from the read; a post
    /// adds the comment it wrote to the list, and a refresh replaces the list
    /// with the log as it reads now.
    @State private var discussion: CommentRead
    /// The comments the human has written in this sheet. They are in the log, and
    /// they are drawn even when the log itself could not be read: a comment that
    /// was written is never one the human cannot see.
    @State private var written: [Comment] = []
    /// Why the last refresh gave no fresh list. The list stays as it was — a
    /// read that did not happen takes nothing away — and this says so.
    @State private var refreshNote: RefreshNote? = nil
    /// The comment the human is writing. It stays after a refusal, so the comment
    /// can be fixed and posted again.
    @State private var commentDraft: String = ""
    /// Why the last post was refused. The draft stays in the input.
    @State private var commentError: String? = nil
    @FocusState private var focus: Field?

    private enum Field { case title, body }

    init(ticket: Ticket, palette: Palette, lanes: [String],
         comments: CommentRead = .list([]),
         onSave: @escaping (TicketEdits) -> Void,
         onCancel: @escaping () -> Void,
         onComment: @escaping (String) -> CommentPostOutcome,
         onRefreshComments: @escaping () -> CommentRead,
         onOpenLink: ((String) -> Void)? = nil,
         onPromote: (() -> PromotionOutcome)? = nil) {
        self.ticket = ticket
        self.palette = palette
        self.lanes = lanes
        self.onSave = onSave
        self.onCancel = onCancel
        self.onComment = onComment
        self.onRefreshComments = onRefreshComments
        self.onOpenLink = onOpenLink
        self.onPromote = onPromote
        _title = State(initialValue: ticket.title)
        _status = State(initialValue: ticket.status)
        _lane = State(initialValue: ticket.lane)
        _bodyText = State(initialValue: ticket.description ?? "")
        _link = State(initialValue: ticket.link)
        _discussion = State(initialValue: comments)
    }

    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canSave: Bool { !trimmedTitle.isEmpty }

    /// Whether the sheet draws the log itself: a log with something in it. The
    /// section is always there — the composer is how the human adds a comment, and
    /// a ticket nobody has commented on still needs one — but an empty log draws
    /// no list, because an empty box would be noise.
    private var hasCommentList: Bool {
        switch discussion {
        case .list(let list): return !list.isEmpty
        // A log that could not be read still carries what the human wrote here.
        case .busy, .failed: return !written.isEmpty
        }
    }

    /// The sheet grows to make room for the discussion: a fixed height would squeeze
    /// the description editor to nothing on a ticket that has one. The composer is
    /// always there; the log above it is what makes the sheet grow.
    private var sheetHeight: CGFloat { hasCommentList ? 700 : 560 }

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
                if let assignee = ticket.assignee {
                    Text(assignee == TicketQuery.human ? "human" : "@\(assignee)")
                        .font(.system(size: 11))
                        .foregroundStyle(assignee == TicketQuery.human ? palette.emberDeep
                                                                     : palette.inkDim)
                }
                // The filer, read-only (the type will not let a write touch it):
                // `human` reads as "you", an agent as its name. The agent's filing is
                // the one worth the colour — it is the signal triage starts from.
                Text(ticket.origin == TicketQuery.human ? "you filed" : "@\(ticket.origin) filed")
                    .font(.system(size: 11))
                    .foregroundStyle(ticket.origin == TicketQuery.human ? palette.inkDim
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
            // appends to, and the human's own composer at the foot of it.
            commentsSection

            HStack(spacing: 10) {
                // The composer takes the shifted one: ⌘↩ is the sheet's Save,
                // and a comment is not a ticket change.
                Text("⌘↩ saves · ⌘⇧↩ posts · Esc cancels")
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
            focus = ticket.description == nil ? .title : .body
        }
        // The human comes back to the app from the terminal, where an agent
        // commented on this ticket. The comments sidecar is its own file under
        // `<home>/comments/`, and the app's watcher watches the store's
        // directory — so nothing else would bring that comment in. (A reopen
        // is a refresh too: the sheet is opened with the log already read.)
        .onReceive(NotificationCenter.default.publisher(
            for: NSApplication.didBecomeActiveNotification)) { _ in
            refreshComments()
        }
    }

    // MARK: - The discussion (the sidecar's log, and the human's composer)

    /// The ticket's comments: a header, either the log itself or the reason it
    /// could not be read, and the composer the human writes in. The two never
    /// share a shape — an empty log is nothing to say, a damaged one is something
    /// to repair — and the composer is under both, because the human's comment is
    /// the one the app is here to record.
    @ViewBuilder
    private var commentsSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Comments — the ticket's discussion log")
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
                // The refresh on demand: the log is read again, and the list is
                // put back as the file holds it. It is a read of its own, so it
                // can happen without a write.
                Button("Refresh") { refreshComments() }
                    .buttonStyle(.plain)
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                    .help("Read the ticket's log again")
            }
            // A refresh that did not read the log says so here, beside the list
            // it is about: the list may be behind, and nothing was lost. When
            // the discussion is itself a failure, its own message already says
            // so, and this would only repeat it.
            if let refreshNote, case .list = discussion {
                Text(refreshNote.text)
                    .font(.system(size: 10.5))
                    .foregroundStyle(refreshNote.isRepair ? palette.emberDeep : palette.inkDim)
                    .fixedSize(horizontal: false, vertical: true)
            }
            switch discussion {
            case .list(let list):
                // A ticket nobody has commented on draws no list: an empty box
                // would be noise. The composer below is not.
                if !list.isEmpty { commentList(list) }
            case .busy(let message):
                // The store was locked by another process and the read gave up.
                // Nothing is wrong with the log, so this is not the repair colour:
                // it is a quiet "try again", and the core's own words.
                Text(message)
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                    .fixedSize(horizontal: false, vertical: true)
                // The human's own comments are in the log and are drawn anyway:
                // a comment that was written is never one the human cannot see.
                if !written.isEmpty { commentList(written) }
            case .failed(let message):
                // The core's own words, which name the damaged file: the log is
                // append-only, so a bad line is repair, not recovery.
                Text(message)
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
                if !written.isEmpty { commentList(written) }
            }
            commentComposer
        }
    }

    /// The composer's field: the human's own comment, in markdown as stored — no
    /// rendering, so nothing can hide a character the human wrote.
    private var commentComposer: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Add a comment — appended to the log, signed as you")
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Write a comment…", text: $commentDraft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(palette.ink)
                    .lineLimit(1...3)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 7)
                    .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
                    .overlay(RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(palette.line, lineWidth: Line.hairline))
                // Quiet, like the sheet's other chips: the sheet's one loud
                // action is Save, and a comment is not a ticket change.
                Button("Post") { post() }
                    .buttonStyle(.plain)
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .overlay(Capsule().strokeBorder(palette.line, lineWidth: Line.hairline))
                    .contentShape(Capsule())
                    // ⌘↩ is the sheet's Save; the comment takes the shifted one.
                    .keyboardShortcut(.return, modifiers: [.command, .shift])
                    .help("Append this comment to the ticket's log (⌘⇧↩)")
            }
            // A refusal says why here, where the Post button was — the sheet
            // answers the action, it does not raise an alarm. The draft stays
            // where it was typed, so the comment can be fixed and posted again.
            if let commentError {
                Text(commentError)
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.emberDeep)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// Posts the human's comment to the ticket's log. A refusal keeps the text:
    /// nothing was written, and the comment can be fixed and posted again.
    private func post() {
        switch onComment(commentDraft) {
        case .posted(let comment):
            // The write path hands back the record it wrote — one lock, one
            // answer. The sheet draws that record with the log it has: the
            // human's own comment is in the list at once. The log is not read
            // back here — a second lock is how a comment that was written came
            // to be reported as "busy".
            written.append(comment)
            discussion = discussion.appending(comment)
            commentDraft = ""
            commentError = nil
            // The log is read again, on its own: a separate read, with its own
            // answers, that puts the rest of the log in front of the human —
            // a comment another actor appended in the meantime is in the file,
            // and the human's own is in it too, in the order the sidecar holds.
            refreshComments()
        case .refused(let message):
            commentError = message
        }
    }

    /// Reads the ticket's log again and draws what it holds.
    ///
    /// A refresh that reads the log replaces the list with the file's own
    /// order. A refresh that does **not** read it — the store was busy, or the
    /// log is damaged — leaves the list exactly as it was and says why: a read
    /// that did not happen takes nothing away, and the comment the human just
    /// wrote stays in the list. The draft is never touched: it is the human's
    /// half of the conversation, and a refresh is not an answer to it.
    private func refreshComments() {
        switch onRefreshComments() {
        case .list(let list):
            discussion = .list(list)
            refreshNote = nil
        case .busy(let message):
            refreshNote = .busy(message)
        case .failed(let message):
            refreshNote = .failed(message)
        }
    }

    /// The log, oldest first, in a scrollable parchment panel. A new comment
    /// scrolls into view: the log is append-only, so the newest one is the one
    /// the human just wrote.
    private func commentList(_ list: [Comment]) -> some View {
        ScrollViewReader { proxy in
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
            .onChange(of: list.count) { _, count in
                guard count > 0 else { return }
                withAnimation(Motion.toggle) { proxy.scrollTo(count - 1, anchor: .bottom) }
            }
        }
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

/// The log with one more comment on it: what the sheet draws after the human's
/// comment was written. A log that could not be read stays as it was — the read
/// path's own answer, which the sheet's own comments are drawn beside.
private extension CommentRead {
    func appending(_ comment: Comment) -> CommentRead {
        if case .list(let list) = self { return .list(list + [comment]) }
        return self
    }
}

/// Why a refresh gave no fresh list. A refresh that did not read the log takes
/// nothing away: the list stays as it was, and this says why.
private enum RefreshNote {
    /// The store was locked by another process: the log is fine, the read gave
    /// up. Nothing is wrong, so the sheet says it quietly.
    case busy(String)
    /// The log could not be read — a damaged line, most often. That is a file to
    /// repair, and the sheet says so in the repair colour.
    case failed(String)

    /// The line the sheet draws under the comments header.
    var text: String {
        switch self {
        case .busy(let message):
            return "The list may be behind — the store was busy. \(message)"
        case .failed(let message):
            return "The list may be behind — the log could not be read. \(message)"
        }
    }

    /// Whether this is a repair (a damaged log) rather than a retry (a busy
    /// store): the two ask the human for different things.
    var isRepair: Bool {
        if case .failed = self { return true }
        return false
    }
}
