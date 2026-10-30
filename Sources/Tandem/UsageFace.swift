import SwiftUI
import Foundation
import TandemCore

// The Usage face (TAN-234): the spend the agents report on their records, read
// two ways: rolled up into one table per dimension, and rolled down into the
// records behind a row.
//
// **No arithmetic.** `UsageReport` (TAN-208) is the derivation: every number
// drawn here is one of its fields or one of its rows', formatted. The one
// quantity this file derives is a bar's width, scaled to the largest row of the
// table being drawn: layout, not a total.
//
// **Absent is absent.** A record that reported no cost draws the absent mark,
// never `$0.00`;
// the report's own rule, and the reason the header states its coverage.
//
// **Read-only.** A record's usage is stamped by the `end` that writes it,
// through the CLI; the store watcher reloads this face with every change.

/// The four row sets of the report, one table at a time, so a two-hundred-row
/// ticket table cannot bury the other three dimensions.
enum UsageDimension: String, CaseIterable, Identifiable {
    case day = "Day"
    case model = "Model"
    case agent = "Agent"
    case ticket = "Ticket"

    var id: String { rawValue }

    /// The report's rows for this dimension, in the report's own order.
    func rows(in report: UsageReport) -> [UsageRow] {
        switch self {
        case .day: return report.day
        case .model: return report.model
        case .agent: return report.agent
        case .ticket: return report.ticket
        }
    }
}

/// The table's columns, so the header and every row line up. The label column
/// is the flexible one.
private enum UsageColumn {
    static let records: CGFloat = 58
    static let tokensIn: CGFloat = 78
    static let tokensOut: CGFloat = 78
    static let cost: CGFloat = 74
    static let bar: CGFloat = 84
}

// MARK: - The face

/// The Usage tab: the reported spend, rolled up and down.
struct UsageFaceView: View {
    /// The one report the face draws (TAN-208). It is computed by the window,
    /// so this face formats and lays out and never sums.
    var report: UsageReport
    /// The board, for resolving a record's carried ticket to its sheet.
    var tickets: [Ticket]
    var palette: Palette
    /// Opens a ticket's sheet, the window's own `ticketFor` plumbing.
    var onOpenTicket: (Ticket) -> Void

    /// The dimension on screen, and the row rolled down under it (if any).
    @State private var dimension: UsageDimension = .day
    @State private var selected: String?

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            if report.records == 0 {
                // A home with no record at all: the quiet state, not an error and
                // not a blank sheet. A home whose records all predate usage still
                // has records, so it draws the zeros with a coverage of 0.
                LigneEmptyState(palette: palette,
                                title: "Nothing reported yet",
                                subtitle: "A session reports what it spent when it ends; its record lands here.",
                                hero: true)
            } else {
                HStack(spacing: 0) {
                    picker
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, Space.gutter)
                .padding(.vertical, Space.lane)
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                table
            }
        }
        .onChange(of: dimension) { _, _ in
            // A row's id only means something inside its own table.
            selected = nil
        }
    }

    // MARK: header

    /// The reported spend, the coverage, and the ticket dimension's own
    /// arithmetic, said once and quietly: the records with nothing reported are
    /// stated rather than missing.
    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Usage")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(palette.ink)
                Text("reported by \(report.reported) of \(report.records) records")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 8)
                Text(UsageFormat.money(report.cost))
                    .font(.system(size: 18, weight: .semibold))
                    .monospacedDigit()
                    .foregroundStyle(palette.ink)
            }
            if !report.ticket.isEmpty {
                Text(ticketArithmetic)
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, Space.gutter)
        .padding(.top, Space.lane)
        .padding(.bottom, Space.lane)
    }

    /// The ticket table's arithmetic in one line: what its rows total, the
    /// overlap that total carries, and the spend no ticket can claim. Every
    /// figure is a report field.
    private var ticketArithmetic: String {
        var parts = ["Ticket rows total \(UsageFormat.money(report.ticketTotal))"]
        if report.overlap > 0 {
            parts.append(
                "\(UsageFormat.money(report.overlap)) of that is counted on each ticket a shared record carried")
        }
        if let loose = report.ticket.first(where: { $0.id == usageReportNoTicketID }) {
            parts.append(
                "\(UsageFormat.records(loose.records)) carried none, \(UsageFormat.money(loose.cost))")
        }
        return parts.joined(separator: " · ")
    }

    // MARK: picker

    /// The app's segmented control (the Tickets face's filter, same chrome):
    /// one table at a time.
    private var picker: some View {
        HStack(spacing: 0) {
            ForEach(UsageDimension.allCases) { candidate in
                segment(candidate)
            }
        }
        .frame(width: 296, height: Space.control)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }

    private func segment(_ candidate: UsageDimension) -> some View {
        let chosen = candidate == dimension
        return Button {
            withAnimation(Motion.toggle) { dimension = candidate }
        } label: {
            Text(candidate.rawValue)
                .font(.system(size: 12.5, weight: chosen ? .semibold : .regular))
                .foregroundStyle(chosen ? palette.ink : palette.inkDim)
                .frame(maxWidth: .infinity)
                .frame(height: Space.control)
                .background(chosen ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: table

    /// The selected dimension's rows, in the report's order: no row is dropped
    /// for being small.
    private var rows: [UsageRow] { dimension.rows(in: report) }

    /// The largest cost in the table being drawn: the bar's scale, and nothing
    /// else.
    private var largestCost: Double { rows.map(\.cost).max() ?? 0 }

    private var table: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                columnHeader
                ForEach(rows) { row in
                    rowView(row)
                    if row.id == selected {
                        UsageRecordsView(row: row,
                                         tickets: tickets,
                                         palette: palette,
                                         onOpenTicket: onOpenTicket)
                    }
                    Rectangle().fill(palette.sand)
                        .frame(height: Line.hairline)
                        .padding(.leading, Space.gutter)
                }
            }
            .padding(.vertical, 6)
        }
    }

    private var columnHeader: some View {
        HStack(spacing: 8) {
            Text(dimension.rawValue.uppercased())
                .tracking(0.4)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text("Records").frame(width: UsageColumn.records, alignment: .trailing)
            Text("In").frame(width: UsageColumn.tokensIn, alignment: .trailing)
            Text("Out").frame(width: UsageColumn.tokensOut, alignment: .trailing)
            Text("Cost").frame(width: UsageColumn.cost, alignment: .trailing)
            Text("").frame(width: UsageColumn.bar)
        }
        .font(.system(size: 10))
        .foregroundStyle(palette.inkDim)
        .padding(.horizontal, Space.gutter)
        .padding(.bottom, 6)
    }

    /// One row: its label, its record count, its tokens, its cost and its bar.
    /// Selecting it rolls the records down beneath it.
    private func rowView(_ row: UsageRow) -> some View {
        let chosen = row.id == selected
        return Button {
            withAnimation(Motion.toggle) { selected = chosen ? nil : row.id }
        } label: {
            HStack(spacing: 8) {
                HStack(spacing: 6) {
                    Image(systemName: chosen ? "chevron.down" : "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 10)
                    Text(row.label)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.ink)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Text("\(row.records)")
                    .font(.system(size: 11.5))
                    .monospacedDigit()
                    .foregroundStyle(palette.inkDim)
                    .frame(width: UsageColumn.records, alignment: .trailing)
                Text(UsageFormat.tokens(row.tokens.input))
                    .frame(width: UsageColumn.tokensIn, alignment: .trailing)
                Text(UsageFormat.tokens(row.tokens.output))
                    .frame(width: UsageColumn.tokensOut, alignment: .trailing)
                Text(row.reported == 0 ? UsageFormat.absent : UsageFormat.money(row.cost))
                    .font(.system(size: 11.5, weight: .medium))
                    .monospacedDigit()
                    .foregroundStyle(palette.ink)
                    .frame(width: UsageColumn.cost, alignment: .trailing)
                bar(row.cost, scaledTo: largestCost)
                    .frame(width: UsageColumn.bar)
            }
            .font(.system(size: 11.5))
            .foregroundStyle(palette.inkDim)
            .padding(.horizontal, Space.gutter)
            .padding(.vertical, 8)
            .background(chosen ? palette.parchment : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Show the \(UsageFormat.records(row.records)) behind this row")
    }

    /// The proportional bar: `accent` on a `sand` track, scaled to the largest
    /// row of the table being drawn.
    private func bar(_ cost: Double, scaledTo largest: Double) -> some View {
        let fraction = largest > 0 ? cost / largest : 0
        return GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(palette.sand)
                Capsule()
                    .fill(palette.accent)
                    .frame(width: max(0, geometry.size.width * fraction))
            }
        }
        .frame(height: 6)
    }
}

// MARK: - The roll-down

/// The records behind one row, newest `startedAt` first: the report's own
/// order and one pattern for all four dimensions. On a ticket row this is the
/// tickets' own record too, and a carried ticket opens its sheet.
struct UsageRecordsView: View {
    var row: UsageRow
    var tickets: [Ticket]
    var palette: Palette
    var onOpenTicket: (Ticket) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(row.sessions) { session in
                record(session)
                if session.id != row.sessions.last?.id {
                    Rectangle().fill(palette.sand).frame(height: Line.hairline)
                }
            }
        }
        .padding(.leading, 30)
        .padding(.trailing, Space.gutter)
        .padding(.vertical, 4)
        .background(palette.parchment)
    }

    /// One record: its session id, agent, model, local start time, cost, the
    /// tokens it reported and the tickets it carried.
    private func record(_ session: Session) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(session.id)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.ink)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("·").foregroundStyle(palette.inkDim)
                Text(session.agent ?? "unattributed")
                    .foregroundStyle(palette.inkDim)
                Text("·").foregroundStyle(palette.inkDim)
                Text(session.model ?? "unreported")
                    .foregroundStyle(palette.inkDim)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Text(session.startedAt.formatted(
                    .dateTime.month(.abbreviated).day().hour().minute()))
                    .monospacedDigit()
                    .foregroundStyle(palette.inkDim)
                    .help(SessionSheet.utc(session.startedAt))
                Text(session.cost.map(UsageFormat.money) ?? UsageFormat.absent)
                    .font(.system(size: 11.5, weight: .medium))
                    .monospacedDigit()
                    .foregroundStyle(palette.ink)
            }
            .font(.system(size: 11.5))

            HStack(spacing: 6) {
                Text(tokenLine(session))
                    .foregroundStyle(palette.inkDim)
                Spacer(minLength: 8)
                ForEach(session.tickets, id: \.self) { id in
                    ticketEntry(id)
                }
            }
            .font(.system(size: 10.5))
        }
        .padding(.vertical, 6)
    }

    /// The tokens the record reported, each piece by its own word; a record
    /// with none says so instead of drawing zeros.
    private func tokenLine(_ session: Session) -> String {
        var parts: [String] = []
        if let tokens = session.tokens {
            if let value = tokens.input { parts.append("\(UsageFormat.tokens(value)) in") }
            if let value = tokens.output { parts.append("\(UsageFormat.tokens(value)) out") }
            if let value = tokens.cacheRead { parts.append("\(UsageFormat.tokens(value)) cache read") }
            if let value = tokens.cacheWrite { parts.append("\(UsageFormat.tokens(value)) cache write") }
            if let value = tokens.reasoning { parts.append("\(UsageFormat.tokens(value)) reasoning") }
        }
        return parts.isEmpty ? "no tokens reported" : parts.joined(separator: " · ")
    }

    /// A ticket the record carried: its ref (or title), opening the ticket
    /// sheet. A ticket the store no longer holds shows its id alone, so the
    /// link is visible rather than silently dropped.
    @ViewBuilder
    private func ticketEntry(_ id: UUID) -> some View {
        if let ticket = tickets.first(where: { $0.id == id }) {
            Button {
                onOpenTicket(ticket)
            } label: {
                Text(ticket.ref ?? ticket.title)
                    .font(.system(size: 10.5, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.accent)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(RoundedRectangle(cornerRadius: 4).fill(palette.paper))
                    .overlay(RoundedRectangle(cornerRadius: 4)
                        .strokeBorder(palette.line, lineWidth: Line.hairline))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Open \(ticket.ref ?? ticket.title)")
        } else {
            Text(id.uuidString)
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(palette.inkDim)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }
}

// MARK: - Formatting

/// The face's formatting, and nothing else: money, tokens, and the mark for a
/// piece no record reported.
enum UsageFormat {
    /// A dollar figure with two decimals, the shape the usage comment on a
    /// ticket's log carries.
    static func money(_ cost: Double) -> String {
        String(format: "$%.2f", cost)
    }

    /// A token count with thousands separators, en_US grouping; a piece no
    /// record reported draws the absent mark.
    static func tokens(_ count: Int?) -> String {
        guard let count else { return absent }
        return count.formatted(.number.locale(Locale(identifier: "en_US")))
    }

    /// A count a record did not report.
    static let absent = "—"

    /// A record count with its noun: "1 record", "8 records".
    static func records(_ count: Int) -> String {
        count == 1 ? "1 record" : "\(count) records"
    }
}
