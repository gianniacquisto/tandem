import SwiftUI
import Foundation
import TandemCore

// The Sprints face (spec `4F746EFE`, R3): the records of sprints, read from
// `sprints.json` through the core (`Sprints`).
//
// **The record's own words.** A sprint has two states — `open` and `closed` —
// and the board's `Status` (`backlog`…`done`) says nothing about one: a sprint
// is not work on a board, so none of those words appears here.
//
// **No write path.** The app never opens, runs or closes a sprint — an agent
// does that, through the CLI, exactly as it never creates or ends a session.
// This face reads the file and draws it.
//
// **The face does not ask the host whether the opening session is live.**
// Whether a session is running *right now* is the CLI's question, not a
// shipped build's (the Sessions decision).

/// One lane's sprints — a named group in the list. The lanes come in
/// first-appearance order (the newest sprint's lane first) and the sprints
/// inside a group are newest `openedAt` first: the core's own order, kept
/// (`Sprints.groupedByLane`, the one tested seam for this rule).
typealias SprintGroup = SprintLaneGroup

/// A ticket a sprint's batch named: the ticket itself, or — when the store no
/// longer holds it — its id alone. Both are drawn, so a batch ticket deleted
/// after the write reads "not on the board", never an error.
enum SprintBatchTicket: Identifiable {
    case carried(Ticket)
    case missing(UUID)

    var id: String {
        switch self {
        case .carried(let ticket): return ticket.id.uuidString
        case .missing(let id): return id.uuidString
        }
    }
}

/// A sprint open in its sheet, with the batch tickets resolved against the
/// store **when it opened**.
///
/// **One value, not two.** The sheet is presented from a closure that captures
/// the face's state as it was when the sheet opened, so a record and the
/// tickets derived for it travel together — otherwise the sheet can open with
/// the batch of the record before it.
struct OpenSprint: Identifiable {
    var sprint: Sprint
    /// The batch, resolved when the record opened.
    var batch: [SprintBatchTicket]

    var id: String { sprint.name }
}

// MARK: - The face

/// The Sprints tab: the recorded sprints, grouped by lane.
struct SprintsFaceView: View {
    var model: TandemModel
    var bikeModel: BikeshedModel
    var palette: Palette
    /// Opens a note in the Bikeshed tab — the window owns the tab switch
    /// (the same path the board's note links use).
    var onOpenLink: (String) -> Void

    /// The record open in its sheet, and the batch derived for it — one value,
    /// not two (the sheet's state is captured when it opens).
    @State private var open: OpenSprint?

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            if model.sprints.isEmpty {
                // No file, no records: the quiet state, and the one command
                // that changes it.
                LigneEmptyState(palette: palette,
                                title: "No sprints recorded",
                                subtitle: "A sprint records itself with tandem-cli sprint open.",
                                hero: false)
            } else {
                list
            }
        }
        .sheet(item: $open) { item in
            SprintSheet(sprint: item.sprint,
                        batch: item.batch,
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
            Text("Sprints")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
            Text("\(model.sprints.count)")
                .font(.system(size: 11))
                .foregroundStyle(palette.inkDim)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.gutter)
        .padding(.top, Space.lane)
        .padding(.bottom, Space.lane)
    }

    // MARK: list

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if let note = model.sprintNote {
                    // The sprints file was kept aside and the list restarted
                    // empty. It says so here, where the sprints are — not in
                    // the board's banner, which is about the ticket list.
                    Text(note)
                        .font(.system(size: 10.5))
                        .foregroundStyle(palette.inkDim)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, Space.gutter)
                        .padding(.top, Space.lane)
                        .padding(.bottom, 4)
                }
                ForEach(Sprints.groupedByLane(model.sprints), id: \.lane) { group in
                    groupHeader(group)
                    ForEach(group.sprints, id: \.name) { sprint in
                        row(sprint)
                        if sprint.name != group.sprints.last?.name {
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

    /// A lane's name and how many records it holds.
    private func groupHeader(_ group: SprintGroup) -> some View {
        HStack(spacing: 8) {
            Text(group.lane)
                .font(.system(size: 11.5, weight: .semibold))
                .tracking(0.4)
                .foregroundStyle(palette.inkDim)
            Text("\(group.sprints.count)")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(palette.inkDim.opacity(0.7))
            Spacer(minLength: 8)
        }
        .padding(.horizontal, Space.gutter)
        .frame(height: 30)
        .padding(.top, 6)
    }

    /// One record: the lane and the sprint's handle, the mode, the batch
    /// count, the record's own state, and the day it opened.
    private func row(_ sprint: Sprint) -> some View {
        Button {
            open(sprint)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(sprint.lane)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.ink)
                    Text("·")
                        .font(.system(size: 12.5))
                        .foregroundStyle(palette.inkDim)
                    Text(sprint.name)
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    stateChip(sprint)
                }
                HStack(spacing: 5) {
                    Text(sprint.mode.rawValue)
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                    Text("·")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                    Text("\(sprint.batch.count) ticket\(sprint.batch.count == 1 ? "" : "s")")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                    Spacer(minLength: 8)
                    Text(sprint.openedAt.formatted(
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
        .help("Open this sprint's record")
    }

    /// The record's state, in the record's words: `open` or `closed`.
    ///
    /// The two states read differently at a glance — an open sprint carries a
    /// live dot and the rust accent, a closed one is quiet — so a reader sees
    /// which sprints are still going without opening any of them.
    private func stateChip(_ sprint: Sprint) -> some View {
        HStack(spacing: 4) {
            if sprint.status == .open {
                Circle().fill(palette.ember).frame(width: 5, height: 5)
            }
            Text(sprint.status.rawValue)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(sprint.status == .open ? palette.emberDeep
                                                        : palette.inkDim)
        }
    }

    /// Opens a record: the sheet gets the record and its batch resolved
    /// against the store now (read time), not read off the record later.
    private func open(_ sprint: Sprint) {
        open = OpenSprint(sprint: sprint, batch: batch(for: sprint))
    }

    /// The batch's tickets, resolved against the store. A ticket deleted
    /// after the write stays in the batch as its id — the sheet says "not on
    /// the board", and no error is raised for a link that points at nothing.
    private func batch(for sprint: Sprint) -> [SprintBatchTicket] {
        sprint.batch.map { id in
            if let ticket = model.ticket(with: id) { return .carried(ticket) }
            return .missing(id)
        }
    }
}

// MARK: - The run entries

/// The run log's entries: `"<ISO-8601 UTC> — <note>"`, dated by the core. The
/// stamp and the note are split here for drawing — the local time shown, the
/// record's own UTC stamp in the tooltip — and an entry that does not split
/// (a hand-edited line, a future shape) is drawn whole rather than dropped.
enum SprintRunEntry {
    case dated(at: Date, note: String)
    case undated(String)

    init(_ line: String) {
        guard let range = line.range(of: " — ") else {
            self = .undated(line)
            return
        }
        let stamp = String(line[..<range.lowerBound])
        let note = String(line[range.upperBound...])
        guard let date = SprintRunEntry.decode(stamp) else {
            self = .undated(line)
            return
        }
        self = .dated(at: date, note: note)
    }

    /// The core stamps entries with its own encoder; the file says ISO-8601
    /// UTC, so the reader parses the same shape.
    private static func decode(_ stamp: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        return formatter.date(from: stamp)
    }
}

// MARK: - The sheet

/// One sprint's record, whole: the batch (each ticket opens its ticket
/// sheet), the runs in order, the close, the opening session (opens that
/// session's sheet — the records link, and the link is the point), and the
/// times.
///
/// Read-only by design — the app never opens, runs or closes a sprint, so
/// there is nothing here to save. What the sheet shows is what the record
/// says.
struct SprintSheet: View {
    var sprint: Sprint
    /// The batch's tickets, resolved against the store when the record opened.
    var batch: [SprintBatchTicket]
    var model: TandemModel
    var bikeshed: Bikeshed
    var palette: Palette
    /// Opens a note in the Bikeshed tab — the window owns the tab switch.
    var onOpenLink: (String) -> Void
    var onClose: () -> Void

    /// The ticket open in the ticket sheet. The sheet owns it, so the two
    /// never race for the window's one modal slot.
    @State private var ticketFor: Ticket?

    /// The opening session's record open in the session sheet — a sheet over
    /// a sheet, the way the session sheet holds a ticket sheet. The session's
    /// notes and tickets are derived when it opens (read time), one value.
    @State private var sessionFor: OpenSession?

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
        .sheet(item: $sessionFor) { item in
            SessionSheet(session: item.session,
                         notes: item.notes,
                         tickets: sessionTickets(for: item.session),
                         model: model,
                         bikeshed: bikeshed,
                         palette: palette,
                         onOpenLink: onOpenLink,
                         onClose: { sessionFor = nil })
        }
    }

    // MARK: title

    private var title: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                Text(sprint.name)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(palette.ink)
                    .lineLimit(1)
                Spacer(minLength: 8)
                chip
            }
            HStack(spacing: 8) {
                Text(sprint.lane)
                    .font(.system(size: 11.5, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                Text("·")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                Text(sprint.mode.rawValue)
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 0)
            }
        }
    }

    /// The record's state, in the record's words: `open` or `closed`.
    private var chip: some View {
        HStack(spacing: 4) {
            if sprint.status == .open {
                Circle().fill(palette.ember).frame(width: 5, height: 5)
            }
            Text(sprint.status.rawValue)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(sprint.status == .open ? palette.emberDeep
                                                        : palette.inkDim)
        }
    }

    // MARK: sections

    private var sections: some View {
        VStack(alignment: .leading, spacing: 14) {
            section("Batch (\(batch.count))") {
                if batch.isEmpty {
                    Text("This sprint carried no tickets.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(batch) { item in
                            ticketRow(item)
                        }
                    }
                }
            }
            section("Runs (\(sprint.runs.count))") {
                if sprint.runs.isEmpty {
                    Text("This sprint has no runs.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(sprint.runs, id: \.self) { line in
                            runRow(line)
                        }
                    }
                }
            }
            section("Close") {
                Text(closeText)
                    .font(.system(size: 12.5))
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            section("Opening session") {
                if let session = sprint.session {
                    Button {
                        openSession(session)
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "link")
                                .font(.system(size: 10))
                                .foregroundStyle(palette.accent)
                            Text(session)
                                .font(.system(size: 11.5, design: .monospaced))
                                .foregroundStyle(palette.accent)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help("Open this session's record")
                } else {
                    Text("No session named.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                }
            }
            section("Times") {
                VStack(alignment: .leading, spacing: 4) {
                    timeRow("Opened", sprint.openedAt)
                    timeRow("Closed", sprint.closedAt)
                }
            }
        }
    }

    /// The close, in the record's words. A close is written once, by the
    /// session that closed the sprint; an open sprint has none yet, and a
    /// closed one that named no text keeps the stamp alone.
    private var closeText: String {
        if let close = sprint.close { return close }
        return sprint.status == .open ? "This sprint has not closed."
                                      : "Closed without a note."
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

    /// One batch ticket: its title, opening the ticket sheet. A ticket the
    /// store no longer holds shows its id alone — the link is there, and the
    /// record says it is not on the board.
    @ViewBuilder
    private func ticketRow(_ item: SprintBatchTicket) -> some View {
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
                Text("not on the board")
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim.opacity(0.8))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// One run entry, in stored order (the order the runs landed). The stamp
    /// is drawn locally with the record's own UTC stamp in the tooltip; a
    /// line that does not split is drawn whole.
    private func runRow(_ line: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            switch SprintRunEntry(line) {
            case .dated(let at, let note):
                Text(note)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                Text(at.formatted(
                        .dateTime.year().month(.abbreviated).day().hour().minute()))
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                    .help(SprintSheet.utc(at))
            case .undated(let raw):
                Text(raw)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Opens the opening session's record: the sheet gets the session and the
    /// capture notes it filed, derived now (read time) — the same derivation
    /// the Sessions face does, so the two faces read a session the same way.
    private func openSession(_ sessionID: String) {
        // The store's session records are already read; the id names one or
        // the sheet says nothing was there.
        guard let session = model.sessions.first(where: { $0.id == sessionID }) else { return }
        let notes = SessionNotes.captureNotes(in: bikeshed, sessionID: session.id)
        sessionFor = OpenSession(session: session, notes: notes)
    }

    /// The tickets the session carried, resolved against the store — the
    /// session sheet's own derivation.
    private func sessionTickets(for session: Session) -> [SessionTicket] {
        session.tickets.map { id in
            if let ticket = model.ticket(with: id) { return .carried(ticket) }
            return .missing(id)
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
                    .help(SprintSheet.utc(date))
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