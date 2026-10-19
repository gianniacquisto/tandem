import Foundation
import Testing
@testable import TandemCore

/// The query (ADR 0017): one predicate over the ticket axes, shared by the CLI's flags and
/// the app's filter row. Proved as a pure function of a ticket list — the cheapest and most
/// complete seam in the change — never through the binary or the view.
@Suite("Todo query")
struct QueryTests {

    private func ticket(
        _ title: String,
        status: Status = .backlog,
        archived: Bool = false,
        description: String? = nil,
        assignee: String? = nil,
        lane: String? = nil
    ) -> Todo {
        Todo(
            title: title,
            status: status,
            archived: archived,
            description: description,
            assignee: assignee,
            lane: lane)
    }

    /// A board with one ticket per interesting corner, in stored order.
    private var board: [Todo] {
        [
            ticket("capture the idea"),
            ticket("refined work", status: .refined, assignee: "core", lane: "Core"),
            ticket("ready work", status: .ready, assignee: "ui", lane: "Core"),
            ticket("in flight", status: .inProgress, description: "Parse the flags in the core",
                   assignee: "ui", lane: "CLI"),
            ticket("finished", status: .done, lane: "CLI"),
            ticket("finished and filed away", status: .done, archived: true, lane: "CLI"),
            ticket("archived but not done", archived: true),
        ]
    }

    private func titles(_ query: TodoQuery) -> [String] {
        query.apply(to: board).map(\.title)
    }

    // MARK: - The axes

    @Test("the default query hides the archive and nothing else")
    func defaultHidesArchived() {
        #expect(titles(TodoQuery()) == [
            "capture the idea", "refined work", "ready work", "in flight", "finished",
        ])
    }

    @Test("the status axis selects exactly the named statuses; empty means every status")
    func statusAxis() {
        #expect(titles(TodoQuery(statuses: [.ready])) == ["ready work"])
        #expect(titles(TodoQuery(statuses: [.ready, .inProgress])) == ["ready work", "in flight"])
        #expect(titles(TodoQuery(statuses: [])) == titles(TodoQuery()))
    }

    @Test("the assignee axis selects a name, or the unowned tickets")
    func assigneeAxis() {
        #expect(titles(TodoQuery(assignee: .named("ui"))) == ["ready work", "in flight"])
        #expect(titles(TodoQuery(assignee: .unassigned)) == ["capture the idea", "finished"])
        #expect(titles(TodoQuery(assignee: .any)) == titles(TodoQuery()))
        #expect(titles(TodoQuery(assignee: .named("nobody"))) == [])
    }

    @Test("the lane axis selects a name, or the unnamed default area")
    func laneAxis() {
        #expect(titles(TodoQuery(lane: .named("Core"))) == ["refined work", "ready work"])
        #expect(titles(TodoQuery(lane: .unnamed)) == ["capture the idea"])
        #expect(titles(TodoQuery(lane: .any)) == titles(TodoQuery()))
    }

    @Test("the text axis matches the title and the description")
    func textAxis() {
        #expect(titles(TodoQuery(text: "FLIGHT")) == ["in flight"])
        #expect(titles(TodoQuery(text: "flags")) == ["in flight"], "matched inside a description")
        #expect(titles(TodoQuery(text: "  ")) == titles(TodoQuery()), "an empty box filters nothing")
        #expect(titles(TodoQuery(text: "nothing here")) == [], "no match is an empty success")
    }

    @Test("the text axis ignores case and accents")
    func textIgnoresCaseAndAccents() {
        let list = [ticket("Über die Brücke"), ticket("plain")]
        #expect(TodoQuery(text: "uber").apply(to: list).map(\.title) == ["Über die Brücke"])
        #expect(TodoQuery(text: "brücke").apply(to: list).map(\.title) == ["Über die Brücke"])
    }

    @Test("the archive axis is three-valued: excluded, included, only")
    func archiveAxis() {
        #expect(titles(TodoQuery(archived: .included)).count == 7)
        #expect(titles(TodoQuery(archived: .only)) == [
            "finished and filed away", "archived but not done",
        ])
    }

    @Test("the axes are ANDed, and an empty result is an empty list")
    func axesCombine() {
        let query = TodoQuery(
            text: "work",
            statuses: [.ready],
            assignee: .named("ui"),
            lane: .named("Core"))
        #expect(titles(query) == ["ready work"])

        // Every axis holds in the first case; one that cannot hold empties the result.
        #expect(titles(TodoQuery(statuses: [.ready], lane: .named("CLI"))).isEmpty)
    }

    // MARK: - The shipped filter (sugar)

    @Test("the shipped filter keeps meaning what it means: open, done, all")
    func filterSugar() {
        #expect(Filter.open.apply(to: board).map(\.title) == [
            "capture the idea", "refined work", "ready work", "in flight",
        ])
        #expect(Filter.done.apply(to: board).map(\.title) == ["finished"])
        #expect(Filter.all.apply(to: board).count == 7, "all is everything, the archive included")
    }

    // MARK: - The boundary (raw values in)

    @Test("parse defaults to the shipped open filter")
    func parseDefault() throws {
        let query = try TodoQuery.parse()
        #expect(query.archived == .excluded)
        #expect(query.statuses == Set(Status.workflow.filter { $0 != .done }))
        #expect(query.assignee == .any)
        #expect(query.lane == .any)
        #expect(query.text == nil)
    }

    @Test("parse takes comma-separated statuses — a repeated flag keeps only the last")
    func parseStatuses() throws {
        #expect(try TodoQuery.parse(status: "ready,in-progress").statuses == [.ready, .inProgress])
        #expect(try TodoQuery.parse(status: " ready , refined ").statuses == [.ready, .refined])
        #expect(try TodoQuery.parse(status: "ready").statuses == [.ready])
    }

    @Test("parse rejects an unknown status by name, listing the five")
    func parseInvalidStatus() {
        do {
            _ = try TodoQuery.parse(status: "ready,blocked")
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

    @Test("parse treats an empty value as the empty field")
    func parseEmptyValues() throws {
        #expect(try TodoQuery.parse(assignee: "").assignee == .unassigned)
        #expect(try TodoQuery.parse(assignee: "ui").assignee == .named("ui"))
        #expect(try TodoQuery.parse(lane: "").lane == .unnamed)
        #expect(try TodoQuery.parse(lane: "Core").lane == .named("Core"))
        #expect(try TodoQuery.parse(text: "  ").normalizedText == nil)
    }

    @Test("parse keeps the shipped filter as sugar, and an explicit axis overrides it")
    func parseFilterSugar() throws {
        #expect(try TodoQuery.parse(filter: "all").archived == .included)
        #expect(try TodoQuery.parse(filter: "all", archived: "excluded").archived == .excluded)
        #expect(try TodoQuery.parse(filter: "done").statuses == [.done])
        #expect(try TodoQuery.parse(filter: "open").statuses == [.backlog, .refined, .ready, .inProgress])
    }

    @Test("parse rejects a bad filter, status, or archive value by name")
    func parseBadValues() {
        #expect(throws: DomainError.invalidFilter("bogus")) {
            _ = try TodoQuery.parse(filter: "bogus")
        }
        #expect(throws: DomainError.invalidArchived("maybe")) {
            _ = try TodoQuery.parse(archived: "maybe")
        }
        #expect(throws: DomainError.invalidStatus("")) {
            _ = try TodoQuery.parse(status: "")
        }
    }
}
