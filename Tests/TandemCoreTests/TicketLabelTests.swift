import Foundation
import Testing
@testable import TandemCore

/// The ticket's two labels, in the core because the app's faces have no test harness:
/// the Status word (the board's column headers, the sheet's picker, the list row) and the
/// assignee label (the board's tile, the sheet's picker, the list row). One rule each, so
/// the faces cannot drift apart (GLOSSARY: the list and the board differ in layout, never
/// in data).
@Suite("Ticket labels")
struct TicketLabelTests {

    @Test("the Status words, in workflow order")
    func statusWords() {
        #expect(Status.workflow.map(\.displayName)
                == ["Backlog", "On hold", "Ready", "In progress", "In Review", "Done"])
    }

    @Test("the assignee label: nil, the reserved word, and an agent's name as stored")
    func assigneeLabel() {
        #expect(AssigneeLabel.of(nil) == nil)
        #expect(AssigneeLabel.of("human") == "human")
        #expect(AssigneeLabel.of("builder") == "@builder")
        #expect(AssigneeLabel.of("Builder") == "@Builder")
    }
}
