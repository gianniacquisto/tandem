//! The ticket's two labels, ported from `TicketLabelTests.swift`: the status word
//! and the assignee label. One rule each, so the faces cannot drift apart.

mod support;

use tandem_core::domain::{AssigneeLabel, Status};

#[test]
fn the_status_words_in_workflow_order() {
    let words: Vec<&str> = Status::WORKFLOW.iter().map(Status::display_name).collect();
    assert_eq!(
        words,
        [
            "Backlog",
            "On hold",
            "Ready",
            "In progress",
            "In Review",
            "Done"
        ]
    );
}

#[test]
fn the_assignee_label_is_nil_the_reserved_word_and_an_agents_name_as_stored() {
    assert_eq!(AssigneeLabel::of(None), None);
    assert_eq!(AssigneeLabel::of(Some("human")), Some("human".to_string()));
    assert_eq!(
        AssigneeLabel::of(Some("builder")),
        Some("@builder".to_string())
    );
    assert_eq!(
        AssigneeLabel::of(Some("Builder")),
        Some("@Builder".to_string())
    );
}
