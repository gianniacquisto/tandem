import Foundation
import Testing
@testable import TandemCore

/// The sheet's assignee picker rule, in the core because the app face has no
/// test harness (the rule ticket 4B7CD193): the options are the roster +
/// **Unassigned**, plus the ticket's current assignee when it is off-roster —
/// an existing value is never an un-reselectable stranger. `human` and
/// Unassigned are constants of the picker, not roster-derived: they survive
/// any Settings edit.
@Suite("Assignee picker")
struct AssigneePickerTests {

    @Test("absent roster and no current → the seed roster plus Unassigned")
    func seedRosterPlusUnassigned() {
        let options = AssigneePicker.options(roster: TandemSettings.defaultRoster, current: nil)
        #expect(options == [nil, "human", "builder", "refiner", "reviewer",
                            "consolidator", "sprint"])
    }

    @Test("an off-roster current assignee is listed, after the roster")
    func offRosterCurrent() {
        let options = AssigneePicker.options(roster: ["human", "builder"],
                                             current: "poet")
        #expect(options == [nil, "human", "builder", "poet"])
    }

    @Test("an on-roster current assignee is listed once, not twice")
    func onRosterCurrentNotDuplicated() {
        let options = AssigneePicker.options(roster: ["human", "builder"],
                                             current: "builder")
        #expect(options == [nil, "human", "builder"])
    }

    @Test("`human` survives a roster edit that drops it")
    func humanNeverDropped() {
        let options = AssigneePicker.options(roster: ["builder", "reviewer"],
                                             current: nil)
        #expect(options.contains("human"))
        #expect(options.first! == nil, "Unassigned is always offered")
    }

    @Test("an edited roster is the order the picker lists, after the constants")
    func editedRosterOrder() {
        let options = AssigneePicker.options(roster: ["poet", "sprint"], current: nil)
        #expect(options == [nil, "human", "poet", "sprint"])
    }

    @Test("a current assignee of `human` is the roster's own entry, not a stranger")
    func humanCurrentIsNotAppended() {
        let options = AssigneePicker.options(roster: TandemSettings.defaultRoster,
                                             current: "human")
        #expect(options == [nil, "human", "builder", "refiner", "reviewer",
                            "consolidator", "sprint"])
    }

    @Test("a nil current assignee adds no stranger")
    func nilCurrentAddsNothing() {
        let options = AssigneePicker.options(roster: ["human"], current: nil)
        #expect(options == [nil, "human"])
    }

    @Test("blank roster entries are dropped; duplicates collapse to one")
    func blanksAndDuplicatesDropped() {
        let options = AssigneePicker.options(roster: ["  poet  ", "", "poet", "   "],
                                             current: nil)
        #expect(options == [nil, "human", "poet"])
    }

    @Test("a whitespace-padded current assignee is trimmed into the list")
    func currentTrimmed() {
        let options = AssigneePicker.options(roster: ["human"], current: "  poet ")
        #expect(options == [nil, "human", "poet"])
    }
}