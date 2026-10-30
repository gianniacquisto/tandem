//! The two labels the ticket faces draw (`TicketLabelTests.swift`,
//! `AssigneeLabel.swift`): the status word and the assignee label. One rule each,
//! so the list and the board cannot label the same ticket differently.

use super::query::HUMAN;

/// A ticket's assignee as every face draws it: `None` for unassigned, `human` for
/// the reserved word, `@name` for an agent. The name is the stored one, never
/// normalised.
pub struct AssigneeLabel;

impl AssigneeLabel {
    pub fn of(assignee: Option<&str>) -> Option<String> {
        let assignee = assignee?;
        if assignee == HUMAN {
            Some(HUMAN.to_string())
        } else {
            Some(format!("@{assignee}"))
        }
    }
}
