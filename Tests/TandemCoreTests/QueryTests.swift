import Foundation
import Testing
@testable import TandemCore

/// The query (ADR 0017): one predicate over the ticket axes, shared by the CLI's flags and
/// the app's filter row. Proved as a pure function of a ticket list — the cheapest and most
/// complete seam in the change — never through the binary or the view.
@Suite("Ticket query")
struct QueryTests {

    private func ticket(
        _ title: String,
        status: Status = .backlog,
        archived: Bool = false,
        description: String? = nil,
        assignee: String? = nil,
        lane: String? = nil
    ) -> Ticket {
        Ticket(
            title: title,
            status: status,
            archived: archived,
            description: description,
            assignee: assignee,
            lane: lane)
    }

    /// A board with one ticket per interesting corner, in stored order.
    private var board: [Ticket] {
        [
            ticket("capture the idea"),
            ticket("parked work", status: .onHold, assignee: "core", lane: "Core"),
            ticket("ready work", status: .ready, assignee: "ui", lane: "Core"),
            ticket("in flight", status: .inProgress, description: "Parse the flags in the core",
                   assignee: "ui", lane: "CLI"),
            ticket("finished", status: .done, lane: "CLI"),
            ticket("finished and filed away", status: .done, archived: true, lane: "CLI"),
            ticket("archived but not done", archived: true),
        ]
    }

    private func titles(_ query: TicketQuery) -> [String] {
        query.apply(to: board).map(\.title)
    }

    // MARK: - The axes

    @Test("the default query hides the archive and nothing else")
    func defaultHidesArchived() {
        #expect(titles(TicketQuery()) == [
            "capture the idea", "parked work", "ready work", "in flight", "finished",
        ])
    }

    @Test("the status axis selects exactly the named statuses; empty means every status")
    func statusAxis() {
        #expect(titles(TicketQuery(statuses: [.ready])) == ["ready work"])
        #expect(titles(TicketQuery(statuses: [.ready, .inProgress])) == ["ready work", "in flight"])
        #expect(titles(TicketQuery(statuses: [])) == titles(TicketQuery()))
    }

    @Test("the assignee axis selects a name, or the unowned tickets")
    func assigneeAxis() {
        #expect(titles(TicketQuery(assignee: .named("ui"))) == ["ready work", "in flight"])
        #expect(titles(TicketQuery(assignee: .unassigned)) == ["capture the idea", "finished"])
        #expect(titles(TicketQuery(assignee: .any)) == titles(TicketQuery()))
        #expect(titles(TicketQuery(assignee: .named("nobody"))) == [])
    }

    @Test("the lane axis selects a name, or the unnamed default area")
    func laneAxis() {
        #expect(titles(TicketQuery(lane: .named("Core"))) == ["parked work", "ready work"])
        #expect(titles(TicketQuery(lane: .unnamed)) == ["capture the idea"])
        #expect(titles(TicketQuery(lane: .any)) == titles(TicketQuery()))
    }

    @Test("the text axis matches the title and the description")
    func textAxis() {
        #expect(titles(TicketQuery(text: "FLIGHT")) == ["in flight"])
        #expect(titles(TicketQuery(text: "flags")) == ["in flight"], "matched inside a description")
        #expect(titles(TicketQuery(text: "  ")) == titles(TicketQuery()), "an empty box filters nothing")
        #expect(titles(TicketQuery(text: "nothing here")) == [], "no match is an empty success")
    }

    @Test("the text axis ignores case and accents")
    func textIgnoresCaseAndAccents() {
        let list = [ticket("Über die Brücke"), ticket("plain")]
        #expect(TicketQuery(text: "uber").apply(to: list).map(\.title) == ["Über die Brücke"])
        #expect(TicketQuery(text: "brücke").apply(to: list).map(\.title) == ["Über die Brücke"])
    }

    @Test("the archive axis is three-valued: excluded, included, only")
    func archiveAxis() {
        #expect(titles(TicketQuery(archived: .included)).count == 7)
        #expect(titles(TicketQuery(archived: .only)) == [
            "finished and filed away", "archived but not done",
        ])
    }

    @Test("the axes are ANDed, and an empty result is an empty list")
    func axesCombine() {
        let query = TicketQuery(
            text: "work",
            statuses: [.ready],
            assignee: .named("ui"),
            lane: .named("Core"))
        #expect(titles(query) == ["ready work"])

        // Every axis holds in the first case; one that cannot hold empties the result.
        #expect(titles(TicketQuery(statuses: [.ready], lane: .named("CLI"))).isEmpty)
    }

    // MARK: - The shipped filter (sugar)

    @Test("the shipped filter keeps meaning what it means: open, done, all")
    func filterSugar() {
        #expect(Filter.open.apply(to: board).map(\.title) == [
            "capture the idea", "parked work", "ready work", "in flight",
        ])
        #expect(Filter.done.apply(to: board).map(\.title) == ["finished"])
        #expect(Filter.all.apply(to: board).count == 7, "all is everything, the archive included")
    }

    // MARK: - The boundary (raw values in)

    @Test("parse defaults to the shipped open filter")
    func parseDefault() throws {
        let query = try TicketQuery.parse()
        #expect(query.archived == .excluded)
        #expect(query.statuses == Set(Status.allCases.filter { $0 != .done }),
                "open is every value the domain accepts but done")
        #expect(query.assignee == .any)
        #expect(query.lane == .any)
        #expect(query.text == nil)
    }

    @Test("parse takes comma-separated statuses — a repeated flag keeps only the last")
    func parseStatuses() throws {
        #expect(try TicketQuery.parse(status: "ready,in-progress").statuses == [.ready, .inProgress])
        #expect(try TicketQuery.parse(status: " ready , on-hold ").statuses == [.ready, .onHold])
        #expect(try TicketQuery.parse(status: "ready").statuses == [.ready])
    }

    @Test("parse rejects an unknown status by name, listing the five")
    func parseInvalidStatus() {
        do {
            _ = try TicketQuery.parse(status: "ready,blocked")
            Issue.record("Expected invalidStatus to be thrown")
        } catch let error as DomainError {
            guard case .invalidStatus(let bad) = error else {
                Issue.record("Expected invalidStatus, got \(error)")
                return
            }
            #expect(bad == "blocked")
            for value in Status.wireValues {
                #expect(error.message.contains(value))
            }
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    @Test("the retired word is not a status the query accepts")
    func retiredWordRefused() {
        do {
            _ = try TicketQuery.parse(status: "refined")
            Issue.record("Expected invalidStatus to be thrown")
        } catch let error as DomainError {
            guard case .invalidStatus(let bad) = error else {
                Issue.record("Expected invalidStatus, got \(error)")
                return
            }
            #expect(bad == "refined")
            #expect(error.message.contains("on-hold"), "the refusal lists the live values")
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    @Test("parse treats an empty value as the empty field")
    func parseEmptyValues() throws {
        #expect(try TicketQuery.parse(assignee: "").assignee == .unassigned)
        #expect(try TicketQuery.parse(assignee: "ui").assignee == .named("ui"))
        #expect(try TicketQuery.parse(lane: "").lane == .unnamed)
        #expect(try TicketQuery.parse(lane: "Core").lane == .named("Core"))
        #expect(try TicketQuery.parse(text: "  ").normalizedText == nil)
    }

    @Test("parse keeps the shipped filter as sugar, and the CLI's filters never carry the archive")
    func parseFilterSugar() throws {
        #expect(try TicketQuery.parse(filter: "all").archived == .excluded)
        #expect(try TicketQuery.parse(filter: "done").statuses == [.done])
        #expect(try TicketQuery.parse(filter: "open").statuses == [.backlog, .onHold, .ready, .inProgress, .inReview])
    }

    // MARK: - the CLI's archive axis

    @Test("the CLI's three filters never carry the archive")
    func cliFiltersHideTheArchive() throws {
        for filter in [nil, "open", "done", "all"] {
            #expect(try TicketQuery.parse(filter: filter).archived == .excluded)
        }
        #expect(titles(try TicketQuery.parse()) == [
            "capture the idea", "parked work", "ready work", "in flight",
        ])
    }

    @Test("`--archived` alone is the archive: statuses ignored, the app's Archived segment")
    func archivedAlone() throws {
        let query = try TicketQuery.parse(archived: true)
        #expect(query.archived == .only)
        #expect(query.statuses.isEmpty)
        #expect(titles(query) == ["finished and filed away", "archived but not done"])
    }

    @Test("`--archived` named with a filter joins that filter")
    func archivedJoinsTheFilter() throws {
        let all = try TicketQuery.parse(filter: "all", archived: true)
        #expect(all.archived == .included)
        #expect(titles(all).count == 7, "every ticket, the archive included")

        let done = try TicketQuery.parse(filter: "done", archived: true)
        #expect(done.archived == .included)
        #expect(titles(done) == ["finished", "finished and filed away"])
    }

    @Test("an explicit status narrows the archive it was named with")
    func archivedWithStatus() throws {
        let query = try TicketQuery.parse(status: "backlog", archived: true)
        #expect(query.statuses == [.backlog])
        #expect(titles(query) == ["archived but not done"])
    }

    // MARK: - the human's inbox

    @Test("the inbox is the open, non-archived tickets assigned to human")
    func inboxView() {
        let tickets = [
            ticket("asked of the person", status: .ready, assignee: "human"),
            ticket("asked and done", status: .done, assignee: "human"),
            ticket("asked and filed away", archived: true, assignee: "human"),
            ticket("agent work", status: .ready, assignee: "core"),
            ticket("nobody's", status: .backlog),
        ]
        #expect(TicketQuery.inbox.apply(to: tickets).map(\.title) == ["asked of the person"])
    }

    @Test("the inbox is empty when nothing is assigned, not an error")
    func emptyInbox() {
        #expect(TicketQuery.inbox.apply(to: [ticket("agent work", assignee: "ui")]).isEmpty)
    }

    @Test("an answered ticket leaves the inbox; an unanswered one stays")
    func inboxExcludesAnswered() {
        let tickets = [
            ticket("still asking", status: .onHold,
                   description: "## Why\n\nwhich option?", assignee: "human"),
            ticket("decided", status: .onHold,
                   description: "## Why\n\nwhich option?\n\n## Answer — 2026-10-26\nA",
                   assignee: "human"),
            ticket("inline mention", status: .onHold,
                   description: "The `## Answer` is recorded on `F1CEFA57`", assignee: "human"),
        ]
        #expect(TicketQuery.inbox.apply(to: tickets).map(\.title) == [
            "still asking", "inline mention",
        ])
    }

    @Test("only the inbox hides answered tickets — the agents' read keeps them as debt")
    func answeredStaysInTheAgentsRead() {
        let answered = ticket("decided", status: .onHold,
                              description: "## Answer — 2026-10-26\nA",
                              assignee: "human")
        #expect(TicketQuery(assignee: .named("human")).apply(to: [answered]).count == 1)
        #expect(TicketQuery().apply(to: [answered]).count == 1)
    }

    @Test("parse rejects a bad filter or status by name")
    func parseBadValues() {
        #expect(throws: DomainError.invalidFilter("bogus")) {
            _ = try TicketQuery.parse(filter: "bogus")
        }
        #expect(throws: DomainError.invalidStatus("")) {
            _ = try TicketQuery.parse(status: "")
        }
    }
}
