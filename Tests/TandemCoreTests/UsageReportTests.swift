import Foundation
import Testing
import TandemCore

/// `usageReport` (TAN-208): one pure derivation over the records and tickets
/// the app already loads, so the Usage face is arithmetic-free.
///
/// The fixture records are built in memory and the calendar is passed in, so
/// nothing here touches a home, the store or the clock.
@Suite("Usage report")
struct UsageReportTests {

    // MARK: - fixtures

    /// UTC, so a fixture's ISO stamp is its own day unless a test says
    /// otherwise.
    private var utc: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        return calendar
    }

    /// The day test's calendar: UTC+2, where 23:30Z is already tomorrow.
    private var plusTwo: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 2 * 3600)!
        return calendar
    }

    private func date(_ iso: String) -> Date {
        guard let date = ISO8601DateFormatter().date(from: iso) else {
            fatalError("bad fixture date \(iso)")
        }
        return date
    }

    private func session(
        _ id: String,
        agent: String? = nil,
        tickets: [UUID] = [],
        startedAt: String = "2026-10-30T12:00:00Z",
        endedAt: String? = nil,
        tokens: SessionTokens? = nil,
        cost: Double? = nil,
        model: String? = nil
    ) -> Session {
        Session(
            id: id,
            project: "tandem",
            agent: agent,
            brief: "Work on \(id).",
            status: .ended,
            tickets: tickets,
            startedAt: date(startedAt),
            endedAt: endedAt.map(date),
            tokens: tokens,
            cost: cost,
            model: model)
    }

    private func ticket(_ id: UUID, number: Int, title: String) -> Ticket {
        Ticket(id: id, title: title, lane: "Tandem", number: number, ref: "TAN-\(number)")
    }

    private func expectClose(
        _ actual: Double, _ expected: Double, _ comment: Testing.Comment? = nil,
        sourceLocation: SourceLocation = #_sourceLocation
    ) {
        #expect(abs(actual - expected) < 1e-9, comment ?? "expected \(expected), got \(actual)", sourceLocation: sourceLocation)
    }

    // MARK: - totals

    @Test("every record counts and only the reported ones sum")
    func everyRecordCounts() {
        let sessions = [
            session("a", tokens: SessionTokens(input: 100, output: 10), cost: 0.5, model: "m"),
            session("b", tokens: SessionTokens(input: 200, output: 20), cost: 1.5, model: "m"),
            session("c"),
        ]

        let report = usageReport(sessions: sessions, tickets: [], calendar: utc)

        #expect(report.records == 3)
        #expect(report.reported == 2)
        #expect(report.cost == 2.0)
        #expect(report.tokens.input == 300)
        #expect(report.tokens.output == 30)
        #expect(report.tokens.reasoning == nil)
        #expect(report.tokens.cacheRead == nil)
        #expect(report.tokens.cacheWrite == nil)
    }

    @Test("a reported zero stays in the rows")
    func reportedZeroStays() throws {
        let zero = session(
            "zero", tokens: SessionTokens(input: 0, output: 0), cost: 0.0, model: "m")
        let paid = session("paid", cost: 1.0, model: "m")

        let report = usageReport(sessions: [zero, paid], tickets: [], calendar: utc)
        let row = try #require(report.model.first { $0.id == "m" })

        #expect(report.reported == 2)
        #expect(report.cost == 1.0)
        #expect(row.reported == 2)
        #expect(row.cost == 1.0)
        #expect(row.tokens.input == 0)
    }

    // MARK: - grouping

    @Test("the model rows group by the exact string and name the unreported")
    func modelRowsGroupExactly() {
        let sessions = [
            session("a", cost: 1.0, model: "glm-5"),
            session("b", cost: 1.0, model: "glm-5"),
            session("c", cost: 2.5, model: "z-ai/glm-5"),
            session("d", cost: 0.5, model: "qwen"),
            session("e", cost: 0.25),
        ]

        let report = usageReport(sessions: sessions, tickets: [], calendar: utc)

        #expect(report.model.map(\.label) == ["z-ai/glm-5", "glm-5", "qwen", "unreported"])
        #expect(report.model.map(\.cost) == [2.5, 2.0, 0.5, 0.25])
        #expect(report.model.first { $0.label == "glm-5" }?.records == 2)
        #expect(report.model.first { $0.label == "unreported" }?.records == 1)
    }

    @Test("the agent rows name the unattributed")
    func agentRowsNameUnattributed() {
        let sessions = [
            session("a", agent: "builder", cost: 1.0, model: "m"),
            session("b", cost: 0.5, model: "m"),
        ]

        let report = usageReport(sessions: sessions, tickets: [], calendar: utc)

        #expect(report.agent.map(\.label) == ["builder", "unattributed"])
        #expect(report.agent.map(\.records) == [1, 1])
    }

    @Test("a day is the local calendar day the record started")
    func dayIsLocalStartDay() {
        let late = session(
            "late", startedAt: "2026-10-30T23:30:00Z", tokens: SessionTokens(input: 1),
            cost: 1.0, model: "m")
        let crossed = session(
            "crossed", startedAt: "2026-10-30T21:00:00Z", endedAt: "2026-10-30T23:30:00Z",
            tokens: SessionTokens(input: 1), cost: 1.0, model: "m")

        let report = usageReport(sessions: [late, crossed], tickets: [], calendar: plusTwo)

        #expect(report.day.map(\.label) == ["2026-10-31", "2026-10-30"])
        #expect(report.day[0].sessions.map(\.id) == ["late"])
        #expect(report.day[1].sessions.map(\.id) == ["crossed"])
    }

    // MARK: - tickets

    @Test("a record's usage lands whole on every ticket it carried")
    func usageLandsWholeOnEveryTicket() {
        let one = UUID()
        let two = UUID()
        let three = UUID()
        let four = UUID()
        let tickets = [
            ticket(one, number: 1, title: "first"),
            ticket(two, number: 2, title: "second"),
            ticket(three, number: 3, title: "third"),
            ticket(four, number: 4, title: "fourth"),
        ]
        let sessions = [
            session(
                "multi", tickets: [one, two, three],
                tokens: SessionTokens(input: 600, output: 60), cost: 0.6, model: "m"),
            session(
                "single", tickets: [four],
                tokens: SessionTokens(input: 300, output: 30), cost: 0.3, model: "m"),
        ]

        let report = usageReport(sessions: sessions, tickets: tickets, calendar: utc)

        #expect(report.ticket.count == 4)
        for row in report.ticket {
            let expectedCost = row.id == four.uuidString ? 0.3 : 0.6
            expectClose(row.cost, expectedCost)
            #expect(row.records == 1)
            #expect(row.tokens.input == (row.id == four.uuidString ? 300 : 600))
            #expect(row.tokens.output == (row.id == four.uuidString ? 30 : 60))
        }
        #expect(report.ticket.first { $0.id == one.uuidString }?.label == "TAN-1: first")
    }

    @Test("a record that carried no ticket lands in the no-ticket row")
    func ticketlessRecordGetsItsRow() {
        let linked = UUID()
        let sessions = [
            session("linked", tickets: [linked], cost: 1.0, model: "m"),
            session("loose", cost: 0.27, model: "m"),
        ]

        let report = usageReport(
            sessions: sessions, tickets: [ticket(linked, number: 9, title: "linked")],
            calendar: utc)

        let loose = report.ticket.last
        #expect(report.records == 2)
        expectClose(report.cost, 1.27)
        #expect(loose?.label == "no ticket")
        #expect(loose?.records == 1)
        expectClose(loose?.cost ?? -1, 0.27)
        #expect(report.ticket.first { $0.id == linked.uuidString }?.records == 1)
        #expect(report.ticket.first { $0.id == linked.uuidString }?.cost == 1.0)
    }

    @Test("a carried ticket the store no longer holds is named by its id")
    func vanishedTicketFallsBackToID() {
        let gone = UUID()
        let sessions = [session("a", tickets: [gone], cost: 0.5, model: "m")]

        let report = usageReport(sessions: sessions, tickets: [], calendar: utc)

        #expect(report.ticket.map(\.label) == [gone.uuidString])
        #expect(report.ticket.first?.records == 1)
        expectClose(report.ticket.first?.cost ?? -1, 0.5)
    }

    @Test("the report states the overlap its ticket rows create")
    func overlapStatesTicketExcess() {
        let one = UUID()
        let two = UUID()
        let three = UUID()
        let tickets = [
            ticket(one, number: 1, title: "first"),
            ticket(two, number: 2, title: "second"),
            ticket(three, number: 3, title: "third"),
        ]
        let multi = usageReport(
            sessions: [
                session("a", tickets: [one, two, three], cost: 0.6, model: "m"),
            ],
            tickets: tickets, calendar: utc)

        expectClose(multi.overlap, 1.2)
        expectClose(multi.ticket.reduce(0) { $0 + $1.cost }, 1.8)

        let single = usageReport(
            sessions: [
                session("a", tickets: [one], cost: 0.6, model: "m"),
                session("b", tickets: [two], cost: 0.4, model: "m"),
            ],
            tickets: tickets, calendar: utc)
        expectClose(single.overlap, 0)
    }

    // MARK: - the views agree

    @Test("the day, model and agent views are one number")
    func viewsAreOneNumber() {
        let one = UUID()
        let two = UUID()
        let sessions = [
            session("a", agent: "builder", tickets: [one, two], cost: 0.6, model: "m"),
            session("b", agent: "builder", tickets: [one], cost: 0.4, model: "m"),
            session("c", tickets: [], cost: 0.27, model: "other"),
            session("d"),
        ]
        let tickets = [
            ticket(one, number: 1, title: "first"),
            ticket(two, number: 2, title: "second"),
        ]

        let report = usageReport(sessions: sessions, tickets: tickets, calendar: utc)

        for rows in [report.day, report.model, report.agent] {
            expectClose(rows.reduce(0) { $0 + $1.cost }, report.cost)
            #expect(rows.reduce(0) { $0 + $1.records } == report.records)
        }
        expectClose(report.ticket.reduce(0) { $0 + $1.cost }, report.cost + report.overlap)
    }

    @Test("a row carries the records behind it, newest first")
    func rowCarriesRecordsNewestFirst() throws {
        let sessions = [
            session("older", startedAt: "2026-10-01T10:00:00Z", cost: 0.1, model: "m"),
            session("newer", startedAt: "2026-10-03T10:00:00Z", cost: 0.2, model: "m"),
            session("middle", startedAt: "2026-10-02T10:00:00Z", cost: 0.3, model: "m"),
        ]

        let report = usageReport(sessions: sessions, tickets: [], calendar: utc)
        let row = try #require(report.model.first { $0.id == "m" })

        #expect(row.sessions.map(\.id) == ["newer", "middle", "older"])
    }

    @Test("an empty record set is a zeroed report")
    func emptyIsZeroed() {
        let report = usageReport(sessions: [], tickets: [], calendar: utc)

        #expect(report.records == 0)
        #expect(report.reported == 0)
        #expect(report.cost == 0)
        #expect(report.overlap == 0)
        #expect(report.tokens == SessionTokens())
        #expect(report.day.isEmpty)
        #expect(report.model.isEmpty)
        #expect(report.agent.isEmpty)
        #expect(report.ticket.isEmpty)
    }
}
