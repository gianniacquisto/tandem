import Foundation

/// The reported usage, rolled up (TAN-208): one pure derivation over the
/// session records and the board's tickets the app already loads.
///
/// **No store read, no clock.** The `Calendar` is passed in, so a test builds
/// its fixtures in memory and the calendar decides what "a day" is; the face
/// that draws the report (TAN-234) does no arithmetic of its own.
///
/// **Absent is absent, never zero.** The record's own rule (`Session`): a
/// record that reports no usage adds to `records` and to no sum, and a record
/// that reported `$0.00` is a reported zero that counts and keeps its rows. A
/// token piece is the sum of the records that reported that piece, and stays
/// `nil` when none did.
///
/// **Three partitions and one overlap.** The day, model and agent rows are
/// three views of one number: each covers every record once, and their costs
/// sum to `cost`. A ticket row is different: every ticket a record carried
/// gets the record's whole cost and whole tokens, exactly what the record's
/// `usage` comment on that ticket states, so the ticket rows sum to
/// `cost + overlap` rather than to `cost`. The face says the overlap once.
public struct UsageReport: Sendable, Equatable {
    /// Every record read, running or ended.
    public let records: Int
    /// The records that carry any usage.
    public let reported: Int
    /// The sum of the reported records' own cost.
    public let cost: Double
    /// The summed token counts, each piece `nil` when no record reported it.
    public let tokens: SessionTokens
    /// By local calendar day, newest first.
    public let day: [UsageRow]
    /// By model string, verbatim, cost descending then label ascending.
    public let model: [UsageRow]
    /// By agent, verbatim, cost descending then label ascending.
    public let agent: [UsageRow]
    /// By carried ticket, cost descending then label ascending, `no ticket`
    /// last. Not a partition: a multi-ticket record lands whole in each row.
    public let ticket: [UsageRow]
    /// The ticket rows' excess over `cost`: `(n-1) x cost` for every record
    /// that carried `n >= 2` tickets.
    public let overlap: Double

    /// What the ticket table's rows add up to, `cost + overlap`: every ticket
    /// row and the ticketless row, whole. The face states it beside `cost`, so
    /// the report keeps the sum it describes (TAN-234).
    public var ticketTotal: Double { cost + overlap }
}

/// One line of the report: a key, its label, the records behind it and what
/// they reported.
public struct UsageRow: Identifiable, Sendable, Equatable {
    /// The row's key, stable across a reload: the day, the model or agent
    /// string (empty for the unnamed row), the ticket's id, or `no-ticket`.
    public let id: String
    /// What the face draws: the day, the exact string, the ticket's ref and
    /// title, or the unnamed row's word.
    public let label: String
    /// Every record behind the row.
    public let records: Int
    /// How many of those records carry usage.
    public let reported: Int
    /// The reported records' summed cost.
    public let cost: Double
    /// The reported records' summed token counts.
    public let tokens: SessionTokens
    /// The records behind the row, newest `startedAt` first.
    public let sessions: [Session]
}

/// The rolling key of the ticketless row: every record that carried no ticket
/// lands here, so the ticket table accounts for the spend it cannot attribute.
public let usageReportNoTicketID = "no-ticket"

/// The ticketless row's label.
public let usageReportNoTicketLabel = "no ticket"

/// Roll the records up (TAN-208).
///
/// The rules are `UsageReport`'s; this is the derivation. An empty record set
/// is a zeroed report, never an error.
public func usageReport(
    sessions: [Session],
    tickets: [Ticket],
    calendar: Calendar
) -> UsageReport {
    let total = summarize(sessions)

    var dayBuckets: [String: [Session]] = [:]
    var modelBuckets: [String: [Session]] = [:]
    var agentBuckets: [String: [Session]] = [:]
    var ticketBuckets: [UUID: [Session]] = [:]
    var ticketless: [Session] = []

    for session in sessions {
        // A day is the local calendar day the record started, whatever day it
        // ended.
        dayBuckets[dayKey(session.startedAt, calendar), default: []].append(session)
        // Grouped by what the record says, verbatim: two spellings are two
        // rows, and the missing value gets its own row rather than vanishing.
        modelBuckets[session.model ?? "", default: []].append(session)
        agentBuckets[session.agent ?? "", default: []].append(session)
        if session.tickets.isEmpty {
            ticketless.append(session)
        } else {
            for ticket in session.tickets {
                ticketBuckets[ticket, default: []].append(session)
            }
        }
    }

    let board = Dictionary(
        tickets.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })

    let modelRows = modelBuckets
        .map { key, records in
            row(id: key, label: key.isEmpty ? "unreported" : key, sessions: records)
        }
        .sorted(by: costThenLabel)

    let agentRows = agentBuckets
        .map { key, records in
            row(id: key, label: key.isEmpty ? "unattributed" : key, sessions: records)
        }
        .sorted(by: costThenLabel)

    var ticketRows = ticketBuckets
        .map { id, records in
            let label: String
            if let ticket = board[id] {
                label = ticket.ref.map { "\($0): \(ticket.title)" } ?? ticket.title
            } else {
                label = id.uuidString
            }
            return row(id: id.uuidString, label: label, sessions: records)
        }
        .sorted(by: costThenLabel)

    // The ticketless row is last, whatever it costs, so the table's bottom
    // line is the spend the tickets cannot account for.
    if !ticketless.isEmpty {
        ticketRows.append(
            row(id: usageReportNoTicketID, label: usageReportNoTicketLabel, sessions: ticketless))
    }

    // Day rows read newest first; the key is a zero-padded local date.
    let dayRows = dayBuckets.keys.sorted(by: >).map { key in
        row(id: key, label: key, sessions: dayBuckets[key] ?? [])
    }

    var overlap = 0.0
    for session in sessions where session.tickets.count >= 2 && carriesUsage(session) {
        overlap += Double(session.tickets.count - 1) * (session.cost ?? 0)
    }

    return UsageReport(
        records: total.records,
        reported: total.reported,
        cost: total.cost,
        tokens: total.tokens,
        day: dayRows,
        model: modelRows,
        agent: agentRows,
        ticket: ticketRows,
        overlap: overlap)
}

// MARK: - private

/// Whether a record carries any usage. The record's usage is the triple
/// `end` stamps together; the model alone is usage the Sessions face draws
/// too, so a reported model with no cost still counts as reported.
private func carriesUsage(_ session: Session) -> Bool {
    if session.cost != nil || session.model != nil { return true }
    guard let tokens = session.tokens else { return false }
    return tokens.input != nil || tokens.output != nil || tokens.reasoning != nil
        || tokens.cacheRead != nil || tokens.cacheWrite != nil
}

/// The summed record count, reported count, cost and tokens.
private struct Summary {
    var records = 0
    var reported = 0
    var cost = 0.0
    var input: Int?
    var output: Int?
    var reasoning: Int?
    var cacheRead: Int?
    var cacheWrite: Int?

    var tokens: SessionTokens {
        SessionTokens(
            input: input, output: output, reasoning: reasoning,
            cacheRead: cacheRead, cacheWrite: cacheWrite)
    }
}

private func summarize(_ sessions: [Session]) -> Summary {
    var summary = Summary()
    for session in sessions {
        summary.records += 1
        guard carriesUsage(session) else { continue }
        summary.reported += 1
        summary.cost += session.cost ?? 0
        if let tokens = session.tokens {
            // A piece sums the records that reported it, and a reported zero
            // adds zero; a piece no record reported stays `nil`.
            if let value = tokens.input { summary.input = (summary.input ?? 0) + value }
            if let value = tokens.output { summary.output = (summary.output ?? 0) + value }
            if let value = tokens.reasoning { summary.reasoning = (summary.reasoning ?? 0) + value }
            if let value = tokens.cacheRead { summary.cacheRead = (summary.cacheRead ?? 0) + value }
            if let value = tokens.cacheWrite { summary.cacheWrite = (summary.cacheWrite ?? 0) + value }
        }
    }
    return summary
}

private func row(id: String, label: String, sessions: [Session]) -> UsageRow {
    let summary = summarize(sessions)
    return UsageRow(
        id: id,
        label: label,
        records: summary.records,
        reported: summary.reported,
        cost: summary.cost,
        tokens: summary.tokens,
        sessions: newestFirst(sessions))
}

/// Cost descending, ties by label ascending, so two reads agree.
private func costThenLabel(_ a: UsageRow, _ b: UsageRow) -> Bool {
    if a.cost != b.cost { return a.cost > b.cost }
    return a.label < b.label
}

/// Newest `startedAt` first; records that share a timestamp keep the order
/// they were read in, so a row does not reshuffle between two reads.
private func newestFirst(_ sessions: [Session]) -> [Session] {
    sessions.enumerated()
        .sorted { a, b in
            if a.element.startedAt != b.element.startedAt {
                return a.element.startedAt > b.element.startedAt
            }
            return a.offset < b.offset
        }
        .map(\.element)
}

/// The local calendar day of a start, as a zero-padded `YYYY-MM-DD` key.
private func dayKey(_ date: Date, _ calendar: Calendar) -> String {
    let parts = calendar.dateComponents([.year, .month, .day], from: date)
    return String(
        format: "%04d-%02d-%02d",
        Int32(parts.year ?? 0), Int32(parts.month ?? 0), Int32(parts.day ?? 0))
}
