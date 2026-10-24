import SwiftUI
import Foundation
import TandemCore

// The Sessions face (spec `FC770B8A`): the records of agent sessions, read
// from `sessions.json` through the core (`Sessions`).
//
// **The record's own words.** A session has two states — `running` and
// `ended` — and four outcomes (`succeeded`, `failed`, `interrupted`,
// `unknown`). The board's `Status` (`backlog`…`done`) says nothing about a
// session, so none of those words appears here: the face reads `running` and
// `ended · succeeded`, never `in-progress` and `done`.
//
// **No write path.** The app never creates, ends or links a record — an agent
// does that, through the CLI. This face reads the file and draws it.
//
// **The face does not ask the host whether a session is live.** Whether a
// session is running *right now* is the CLI's question (`opencode api
// session.list`), and a host-specific dependency does not belong in a shipped
// build for a marker the record already carries. A record says what a session
// wrote; its own two states are what this face draws.

/// One project's sessions — a named group in the list. The projects come in
/// first-appearance order (the newest session's project first) and the
/// sessions inside a group are newest `startedAt` first: the core's own
/// order, kept.
struct SessionGroup: Identifiable {
    var project: String
    var sessions: [Session]
    var id: String { project }
}

/// A ticket a session carried: the ticket itself, or — when the store no
/// longer holds it — its id alone. Both are drawn, so a link that points at
/// nothing is visible rather than silently dropped.
enum SessionTicket: Identifiable {
    case carried(Ticket)
    case missing(UUID)

    var id: String {
        switch self {
        case .carried(let ticket): return ticket.id.uuidString
        case .missing(let id): return id.uuidString
        }
    }
}

/// A record open in its sheet, with the capture notes it filed.
///
/// **One value, not two.** The sheet is presented from a closure that captures
/// the face's state as it was when the sheet opened, so a record and the notes
/// derived for it travel together — otherwise the sheet can open with the
/// notes of the record before it.
struct OpenSession: Identifiable {
    var session: Session
    /// The capture notes the session filed, derived when the record opened.
    var notes: [String]

    var id: String { session.id }
}

// MARK: - The face

/// The Sessions tab: the recorded sessions, grouped by project.
struct SessionsFaceView: View {
    var model: TandemModel
    var bikeModel: BikeshedModel
    var palette: Palette
    /// Opens a note in the Bikeshed tab — the window owns the tab switch
    /// (the same path the board's note links use).
    var onOpenLink: (String) -> Void

    /// The record open in its sheet, and the capture notes it filed — derived
    /// when it opened. **One value, not two:** the sheet is presented from a
    /// closure that captures the face's state as it was, so a record and its
    /// notes must travel together or the sheet can open with the old ones.
    @State private var open: OpenSession?

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            if model.sessions.isEmpty {
                // No file, no records: the quiet state, and the one command
                // that changes it.
                LigneEmptyState(palette: palette,
                                title: "No sessions recorded",
                                subtitle: "A session records itself with tandem-cli session start.",
                                hero: false)
            } else {
                list
            }
        }
        .sheet(item: $open) { item in
            SessionSheet(session: item.session,
                         notes: item.notes,
                         tickets: tickets(for: item.session),
                         model: model,
                         bikeshed: bikeModel.bikeshed,
                         palette: palette,
                         onOpenLink: onOpenLink,
                         onClose: { open = nil })
        }
    }

    // MARK: header

    private var header: some View {
        HStack(spacing: 6) {
            Text("Sessions")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
            Text("\(model.sessions.count)")
                .font(.system(size: 11))
                .foregroundStyle(palette.inkDim)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.gutter)
        .padding(.top, Space.lane)
        .padding(.bottom, Space.lane)
    }

    // MARK: list

    /// The groups: the projects in first-appearance order, the sessions inside
    /// each group newest `startedAt` first. The core already hands them over
    /// newest-first, so the grouping is a fold, not a sort.
    private var groups: [SessionGroup] {
        var order: [String] = []
        var byProject: [String: [Session]] = [:]
        for session in model.sessions {
            if byProject[session.project] == nil {
                order.append(session.project)
                byProject[session.project] = []
            }
            byProject[session.project]?.append(session)
        }
        return order.map { SessionGroup(project: $0, sessions: byProject[$0] ?? []) }
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if let note = model.sessionNote {
                    // The sessions file was kept aside and the list restarted
                    // empty. It says so here, where the sessions are — not in
                    // the board's banner, which is about the ticket list.
                    Text(note)
                        .font(.system(size: 10.5))
                        .foregroundStyle(palette.inkDim)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, Space.gutter)
                        .padding(.top, Space.lane)
                        .padding(.bottom, 4)
                }
                ForEach(groups) { group in
                    groupHeader(group)
                    ForEach(group.sessions) { session in
                        row(session)
                        if session.id != group.sessions.last?.id {
                            Rectangle().fill(palette.sand)
                                .frame(height: Line.hairline)
                                .padding(.leading, Space.gutter)
                        }
                    }
                }
            }
            .padding(.vertical, 6)
        }
    }

    /// A project's name and how many records it holds.
    private func groupHeader(_ group: SessionGroup) -> some View {
        HStack(spacing: 8) {
            Text(group.project)
                .font(.system(size: 11.5, weight: .semibold))
                .tracking(0.4)
                .foregroundStyle(palette.inkDim)
            Text("\(group.sessions.count)")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(palette.inkDim.opacity(0.7))
            Spacer(minLength: 8)
        }
        .padding(.horizontal, Space.gutter)
        .frame(height: 30)
        .padding(.top, 6)
    }

    /// One record: the project and the session's handle, the agent (or
    /// "unattributed"), the record's own state, and the day it started.
    private func row(_ session: Session) -> some View {
        Button {
            open(session)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(session.project)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.ink)
                    Text("·")
                        .font(.system(size: 12.5))
                        .foregroundStyle(palette.inkDim)
                    Text(session.id)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 8)
                    stateChip(session)
                }
                HStack(spacing: 5) {
                    Text(session.agent ?? "unattributed")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                    Spacer(minLength: 8)
                    Text(session.startedAt.formatted(
                        .dateTime.year().month(.abbreviated).day()))
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, Space.gutter)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Open this session's record")
    }

    /// The record's state, in the record's words: `running`, or
    /// `ended · <outcome>`.
    ///
    /// The two states read differently at a glance — a running record carries
    /// a live dot and the rust accent, an ended one is quiet — so a reader
    /// sees which records are still going without opening any of them.
    private func stateChip(_ session: Session) -> some View {
        HStack(spacing: 4) {
            if session.status == .running {
                Circle().fill(palette.ember).frame(width: 5, height: 5)
            }
            Text(SessionState.text(for: session))
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(session.status == .running ? palette.emberDeep
                                                           : palette.inkDim)
        }
    }

    /// Opens a record: the sheet gets the record and the capture notes the
    /// session filed, derived now (read time), not read off the record.
    private func open(_ session: Session) {
        let notes = SessionNotes.captureNotes(in: bikeModel.bikeshed,
                                              sessionID: session.id)
        open = OpenSession(session: session, notes: notes)
    }

    /// The tickets the record carried, resolved against the store.
    private func tickets(for session: Session) -> [SessionTicket] {
        session.tickets.map { id in
            if let ticket = model.ticket(with: id) { return .carried(ticket) }
            return .missing(id)
        }
    }
}

// MARK: - The record's words

/// How a session record reads its own state. The words are the record's, not
/// the board's: `running` and `ended`, with the outcome after the dot.
enum SessionState {
    static func text(for session: Session) -> String {
        switch session.status {
        case .running:
            return "running"
        case .ended:
            guard let outcome = session.outcome else { return "ended" }
            return "ended · \(outcome.rawValue)"
        }
    }
}

// MARK: - The capture notes

/// The capture notes a session filed — the paths under
/// `projects/*/.raw/**/<session-id>/`.
///
/// **The saddlebag folder is the link.** The record carries no list of its
/// notes, because consolidation moves captures (from `.raw/<id>/` to
/// `.raw/.done/<id>/`) and a stored path would rot. So the paths are read
/// from the bikeshed every time a record is opened — by the core, which owns that
/// scan (`Bikeshed.sessionNotes(session:)`), so this face and the CLI's
/// `session show` can never name different notes.
enum SessionNotes {

    /// The note paths (bikeshed-root-relative) the session filed.
    static func captureNotes(in bikeshed: Bikeshed, sessionID: String) -> [String] {
        (try? bikeshed.sessionNotes(session: sessionID).value) ?? []
    }
}

// MARK: - The sheet

/// One session's record, whole: the brief, the summary, the notes it filed,
/// the tickets it carried, and when it started and ended.
///
/// Read-only by design — the app never creates, ends or links a record, so
/// there is nothing here to save. What the sheet shows is what the record
/// says.
struct SessionSheet: View {
    var session: Session
    /// The capture notes the session filed — the paths under
    /// `projects/*/.raw/**/<session-id>/`, derived when the record opened.
    var notes: [String]
    /// The tickets the session carried, resolved against the store.
    var tickets: [SessionTicket]
    var model: TandemModel
    var bikeshed: Bikeshed
    var palette: Palette
    /// Opens a note in the Bikeshed tab — the window owns the tab switch.
    var onOpenLink: (String) -> Void
    var onClose: () -> Void

    /// The ticket open in the ticket sheet. The sheet owns it, so the two
    /// never race for the window's one modal slot.
    @State private var ticketFor: Ticket?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            title
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            ScrollView {
                sections
                    .padding(.vertical, 2)
            }
            footer
        }
        .padding(20)
        .frame(width: 580, height: 620)
        .background(palette.paper)
        .sheet(item: $ticketFor) { ticket in
            TicketSheet(
                ticket: ticket,
                palette: palette,
                lanes: model.lanes,
                roster: TandemSettings.roster(),
                onSave: { edits in
                    model.saveTicket(ticket.id.uuidString, edits: edits)
                    ticketFor = nil
                },
                onCancel: { ticketFor = nil },
                onComment: { model.postComment(ticket.id.uuidString, body: $0) },
                onRefreshComments: { await model.comments(for: ticket.id.uuidString) },
                onOpenLink: onOpenLink,
                onPromote: { model.promoteToNote(ticket.id.uuidString, into: bikeshed) })
        }
    }

    // MARK: title

    private var title: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                Text(session.id)
                    .font(.system(size: 14, weight: .semibold, design: .monospaced))
                    .foregroundStyle(palette.ink)
                    .textSelection(.enabled)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                chip
            }
            HStack(spacing: 8) {
                Text(session.project)
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                Text("·")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                Text(session.agent ?? "unattributed")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
            }
        }
    }

    /// The record's state, in the record's words.
    private var chip: some View {
        HStack(spacing: 4) {
            if session.status == .running {
                Circle().fill(palette.ember).frame(width: 5, height: 5)
            }
            Text(SessionState.text(for: session))
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(session.status == .running ? palette.emberDeep
                                                           : palette.inkDim)
        }
    }

    // MARK: sections

    private var sections: some View {
        VStack(alignment: .leading, spacing: 14) {
            section("Brief") {
                Text(session.brief)
                    .font(.system(size: 12.5))
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            if let summary = session.summary {
                section("Summary") {
                    Text(summary)
                        .font(.system(size: 12.5))
                        .foregroundStyle(palette.ink)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
            }
            section("Notes (\(notes.count))") {
                if notes.isEmpty {
                    Text("This session filed no notes.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(notes, id: \.self) { path in
                            noteRow(path)
                        }
                    }
                }
            }
            section("Tickets (\(tickets.count))") {
                if tickets.isEmpty {
                    Text("This session carried no tickets.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(tickets) { item in
                            ticketRow(item)
                        }
                    }
                }
            }
            section("Times") {
                VStack(alignment: .leading, spacing: 4) {
                    timeRow("Started", session.startedAt)
                    timeRow("Ended", session.endedAt)
                }
            }
        }
    }

    /// A labelled section.
    private func section<Content: View>(_ title: String,
                                        @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title)
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
            content()
        }
    }

    /// One capture note: its name and where it lives. Clicking it opens the
    /// note in the Bikeshed tab — the same path the board's note links take.
    private func noteRow(_ path: String) -> some View {
        Button {
            onOpenLink(path)
            onClose()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "doc.text")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.accent)
                VStack(alignment: .leading, spacing: 1) {
                    Text(BikeshedPath.name(of: path))
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(palette.accent)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Text(BikeshedPath.folder(of: path))
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Open this note in the Bikeshed")
    }

    /// One carried ticket: its title, opening the ticket sheet. A ticket the
    /// store no longer holds shows its id alone — the link is there, and the
    /// sheet says so.
    @ViewBuilder
    private func ticketRow(_ item: SessionTicket) -> some View {
        switch item {
        case .carried(let ticket):
            Button {
                ticketFor = ticket
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "checklist")
                        .font(.system(size: 10))
                        .foregroundStyle(palette.accent)
                    Text(ticket.title)
                        .font(.system(size: 12))
                        .foregroundStyle(palette.ink)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Open this ticket")
        case .missing(let id):
            HStack(spacing: 6) {
                Image(systemName: "questionmark.circle")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.inkDim)
                Text(id.uuidString)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.inkDim)
                Text("not in the list")
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim.opacity(0.8))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// One time: the local time, with the record's own UTC stamp in the
    /// tooltip — the file says UTC, and a reader comparing the two should not
    /// have to guess.
    private func timeRow(_ label: String, _ date: Date?) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.system(size: 11.5))
                .foregroundStyle(palette.inkDim)
                .frame(width: 56, alignment: .leading)
            if let date {
                Text(date.formatted(
                    .dateTime.year().month(.abbreviated).day().hour().minute()))
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.ink)
                    .help(SessionSheet.utc(date))
            } else {
                Text("—")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
            }
            Spacer(minLength: 0)
        }
    }

    /// The record's UTC stamp, as the file holds it.
    static func utc(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        return formatter.string(from: date)
    }

    // MARK: footer

    private var footer: some View {
        HStack(spacing: 10) {
            Text("A record is written by the agent that ran it — the app never edits one.")
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
                .lineLimit(1)
            Spacer()
            Button("Close", action: onClose)
                .keyboardShortcut(.cancelAction)
        }
    }
}
