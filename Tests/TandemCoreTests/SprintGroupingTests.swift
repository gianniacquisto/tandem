import Foundation
import Testing
@testable import TandemCore

/// The sprints face's grouping (spec `4F746EFE`, R3): the records grouped by
/// lane for display. The rule lives in the core because the core is the only
/// test seam — a second reader (a CLI list, a later face) reaches the same
/// grouping rather than a lookalike (the `QuickOpen.rank` precedent).
///
/// The input is the core's own list order — newest `openedAt` first — and the
/// grouping must keep it, not re-sort it.
@Suite("Sprint grouping")
struct SprintGroupingTests {

    /// Three lanes interleaved in newest-first order: the groups come out in
    /// first-appearance order (the newest sprint's lane first), and each group
    /// keeps the records' own order.
    @Test func groupsByLaneInFirstAppearanceOrder() {
        let tandemOld = Sprint(lane: "Tandem", number: 1, openedAt: date(1))
        let codeRank = Sprint(lane: "code-rank", number: 1, openedAt: date(2))
        let tandemNew = Sprint(lane: "Tandem", number: 2, openedAt: date(3))
        // Newest first — the order `Sprints.list()` hands over.
        let groups = Sprints.groupedByLane([tandemNew, codeRank, tandemOld])
        #expect(groups.map(\.lane) == ["Tandem", "code-rank"])
        #expect(groups[0].sprints == [tandemNew, tandemOld])
        #expect(groups[1].sprints == [codeRank])
    }

    /// No records: no groups.
    @Test func emptyInputGivesNoGroups() {
        #expect(Sprints.groupedByLane([]).isEmpty)
    }

    /// One lane, one record: one group, the record in it.
    @Test func singleRecordIsItsOwnGroup() {
        let sprint = Sprint(lane: "Tandem", number: 1, openedAt: date(1))
        let groups = Sprints.groupedByLane([sprint])
        #expect(groups.count == 1)
        #expect(groups[0].lane == "Tandem")
        #expect(groups[0].sprints == [sprint])
    }

    private func date(_ hour: Int) -> Date {
        Date(timeIntervalSince1970: TimeInterval(1_760_000_000 + hour * 3600))
    }
}