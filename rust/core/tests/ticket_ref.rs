//! Ticket references (ticket 6598E091, reshaped by TAN-339, ADR 0053): a project
//! key plus a project-wide number, rendered `KEY-N` (`TAN-42`) and accepted
//! wherever an id is.
//!
//! The label is an optional grouping inside the project: it owns no key and no
//! counter, so a move between labels of one project never changes a ref, and a
//! ticket never changes project.

mod support;

use support::Sandbox;
use tandem_core::domain::{AddTicket, Board, DomainError, Filter, Status, TicketRef};

fn in_project<'a>(project: &'a str) -> AddTicket<'a> {
    AddTicket {
        project: Some(project),
        ..AddTicket::default()
    }
}

fn in_label<'a>(label: &'a str) -> AddTicket<'a> {
    AddTicket {
        label: Some(label),
        ..AddTicket::default()
    }
}

/// A project `Probe` (PRB) with the labels `alpha` and `beta`, and a second
/// project `Retro` (RET) with the label `gamma`.
fn two_projects(board: &Board) {
    board.add_project("Probe", Some("PRB")).unwrap();
    board.add_label("alpha", Some("Probe")).unwrap();
    board.add_label("beta", Some("Probe")).unwrap();
    board.add_project("Retro", Some("RET")).unwrap();
    board.add_label("gamma", Some("Retro")).unwrap();
}

#[test]
fn a_projects_tickets_are_numbered_and_the_ref_is_on_add_and_list() {
    let sandbox = Sandbox::new("ref-numbers");
    let board = sandbox.empty_board();
    board.add_project("Probe", Some("PRB")).unwrap();
    let first = board.add("first", in_project("Probe")).unwrap();
    let second = board.add("second", in_project("Probe")).unwrap();

    assert_eq!(first.number, Some(1));
    assert_eq!(first.reference.as_deref(), Some("PRB-1"));
    assert_eq!(second.number, Some(2));
    assert_eq!(second.reference.as_deref(), Some("PRB-2"));
    assert_eq!(first.project, "Probe");
    assert_eq!(first.label, None, "a ticket needs no label");

    let mut refs: Vec<String> = board
        .list_filter(Filter::All)
        .unwrap()
        .into_iter()
        .filter_map(|ticket| ticket.reference)
        .collect();
    refs.sort();
    assert_eq!(refs, ["PRB-1", "PRB-2"], "the list rows carry the ref too");
}

#[test]
fn two_labels_in_one_project_issue_one_sequence() {
    let sandbox = Sandbox::new("ref-one-sequence");
    let board = sandbox.empty_board();
    two_projects(&board);
    let a = board.add("in alpha", in_label("alpha")).unwrap();
    let b = board.add("in beta", in_label("beta")).unwrap();
    let c = board.add("in gamma", in_label("gamma")).unwrap();
    assert_eq!(a.reference.as_deref(), Some("PRB-1"));
    assert_eq!(b.reference.as_deref(), Some("PRB-2"));
    assert_eq!(c.reference.as_deref(), Some("RET-1"));
    assert_eq!(a.project, "Probe", "a label alone implies its project");
}

#[test]
fn an_absent_key_is_the_first_three_of_the_name_separators_stripped() {
    let sandbox = Sandbox::new("ref-keys");
    let board = sandbox.empty_board();
    board.add_project("Tandem", None).unwrap();
    board.add_project("Retro", None).unwrap();
    board.add_project("code-rank", None).unwrap();

    let reference = |project: &str| board.add("t", in_project(project)).unwrap().reference;
    assert_eq!(reference("Tandem").as_deref(), Some("TAN-1"));
    assert_eq!(reference("Retro").as_deref(), Some("RET-1"));
    assert_eq!(reference("code-rank").as_deref(), Some("COD-1"));
}

#[test]
fn a_key_already_in_use_is_refused_case_insensitively() {
    let sandbox = Sandbox::new("ref-duplicate");
    let board = sandbox.empty_board();
    board.add_project("Probe", Some("PRB")).unwrap();
    assert_eq!(
        board.add_project("Pilot", Some("prb")),
        Err(DomainError::DuplicateProjectKey("PRB".to_string()))
    );
    assert!(
        DomainError::DuplicateProjectKey("PRB".to_string())
            .message()
            .contains("PRB"),
        "the failure names the key it cannot use"
    );
    assert_eq!(
        board.add_project("Bad", Some("P-R")),
        Err(DomainError::InvalidProjectKey("P-R".to_string())),
        "a key is alphanumerics only"
    );
}

#[test]
fn show_resolves_a_ref_in_either_case_and_an_unissued_number_names_the_projects_range() {
    let sandbox = Sandbox::new("ref-resolve");
    let board = sandbox.empty_board();
    board.add_project("Probe", Some("PRB")).unwrap();
    let first = board.add("first", in_project("Probe")).unwrap();
    let second = board.add("second", in_project("Probe")).unwrap();

    for reference in ["PRB-2", "prb-2"] {
        assert_eq!(
            board.ticket(reference).unwrap().id,
            second.id,
            "{reference} must resolve"
        );
    }
    assert_eq!(board.ticket(&second.id.to_string()).unwrap().id, second.id);

    match board.ticket("PRB-99").unwrap_err() {
        DomainError::UnknownRef { reference, hint } => {
            assert_eq!(reference, "PRB-99");
            assert!(
                hint.contains("Project \"Probe\" (PRB)") && hint.contains("PRB-1 to PRB-2"),
                "the failure names the project's real range: {hint}"
            );
        }
        other => panic!("expected unknownRef, got {other:?}"),
    }
    match board.ticket("ZZZ-1").unwrap_err() {
        DomainError::UnknownRef { hint, .. } => assert!(hint.contains("ZZZ")),
        other => panic!("expected unknownRef, got {other:?}"),
    }
    assert_eq!(board.ticket("PRB-1").unwrap().id, first.id);
}

#[test]
fn a_label_rename_keeps_every_ref() {
    let sandbox = Sandbox::new("ref-rename");
    let board = sandbox.empty_board();
    two_projects(&board);
    board.add("first", in_label("alpha")).unwrap();

    board.rename_label("alpha", "alpha two", None).unwrap();
    let shown = board.ticket("PRB-1").unwrap();
    assert_eq!(shown.label.as_deref(), Some("alpha two"));
    assert_eq!(shown.reference.as_deref(), Some("PRB-1"));
}

#[test]
fn a_project_key_is_frozen_once_a_number_is_issued() {
    let sandbox = Sandbox::new("ref-frozen");
    let board = sandbox.empty_board();
    board.add_project("Probe", Some("PRB")).unwrap();
    board.add("first", in_project("Probe")).unwrap();

    match board
        .rename_project("Probe", "Probe Two", Some("PBX"))
        .unwrap_err()
    {
        DomainError::FrozenProjectKey { key, project } => {
            assert_eq!(key, "PRB");
            assert_eq!(project, "Probe");
        }
        other => panic!("expected frozenProjectKey, got {other:?}"),
    }
    assert_eq!(
        board.ticket("PRB-1").unwrap().project,
        "Probe",
        "nothing changed"
    );

    // A project that has issued nothing may move its key.
    board.add_project("Empty", None).unwrap();
    let listed = board
        .rename_project("Empty", "Empty Two", Some("EMP"))
        .unwrap();
    assert!(
        listed
            .iter()
            .any(|entry| entry.name == "Empty Two" && entry.key == "EMP")
    );

    // A rename without a key keeps the key, and the tickets follow the name.
    board.rename_project("Probe", "Probe Two", None).unwrap();
    let kept = board.ticket("PRB-1").unwrap();
    assert_eq!(kept.project, "Probe Two");
    assert_eq!(kept.reference.as_deref(), Some("PRB-1"));
}

#[test]
fn a_move_between_labels_of_one_project_keeps_the_number_and_the_ref() {
    let sandbox = Sandbox::new("ref-move-inside");
    let board = sandbox.empty_board();
    two_projects(&board);
    let ticket = board.add("moving", in_label("alpha")).unwrap();
    assert_eq!(ticket.reference.as_deref(), Some("PRB-1"));

    let moved = board
        .move_ticket("PRB-1", Some("beta"), usize::MAX)
        .unwrap();
    assert_eq!(moved.reference.as_deref(), Some("PRB-1"));
    assert_eq!(moved.number, Some(1));
    assert_eq!(moved.label.as_deref(), Some("beta"));

    // A move to no label keeps the project, the number and the ref.
    let loose = board.move_ticket("PRB-1", None, usize::MAX).unwrap();
    assert_eq!(loose.reference.as_deref(), Some("PRB-1"));
    assert_eq!(loose.project, "Probe");
    assert_eq!(loose.label, None);

    // The TAN-316 case: a write with a ref held before the move still lands.
    board
        .set(
            "PRB-1",
            tandem_core::domain::SetFields {
                title: tandem_core::domain::Edit::Set("renamed"),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        board.ticket(&ticket.id.to_string()).unwrap().title,
        "renamed"
    );
}

#[test]
fn a_move_to_another_projects_label_is_refused_naming_both_projects() {
    let sandbox = Sandbox::new("ref-move-across");
    let board = sandbox.empty_board();
    two_projects(&board);
    board.add("moving", in_label("alpha")).unwrap();

    match board
        .move_ticket("PRB-1", Some("gamma"), usize::MAX)
        .unwrap_err()
    {
        error @ DomainError::CrossProjectMove { .. } => {
            let message = error.message();
            assert!(
                message.contains("Probe") && message.contains("Retro"),
                "the refusal names the two projects: {message}"
            );
        }
        other => panic!("expected crossProjectMove, got {other:?}"),
    }
    let kept = board.ticket("PRB-1").unwrap();
    assert_eq!(kept.label.as_deref(), Some("alpha"), "nothing moved");

    // The board's drag is the same rule.
    assert!(matches!(
        board.place("PRB-1", Some("gamma"), Status::Ready, 0),
        Err(DomainError::CrossProjectMove { .. })
    ));
}

#[test]
fn add_without_a_project_or_label_is_refused_and_lists_the_projects() {
    let sandbox = Sandbox::new("ref-add-refused");
    let board = sandbox.empty_board();

    // A home with no project at all says how to make one.
    let error = board.add("loose", AddTicket::default()).unwrap_err();
    assert!(
        error.message().contains("project_add"),
        "an empty home names the recovery: {}",
        error.message()
    );

    two_projects(&board);
    let error = board.add("loose", AddTicket::default()).unwrap_err();
    assert!(
        matches!(error, DomainError::MissingProject { .. }),
        "got {error:?}"
    );
    assert_eq!(
        error.message(),
        "Name a project: Probe, Retro (or a label).",
        "no implicit project, even when one would do"
    );

    // One project on the home does not make it implicit either.
    let sandbox = Sandbox::new("ref-add-refused-one");
    let board = sandbox.empty_board();
    board.add_project("Only", None).unwrap();
    assert!(matches!(
        board.add("loose", AddTicket::default()),
        Err(DomainError::MissingProject { .. })
    ));
    assert!(matches!(
        board.add("loose", in_project("Nope")),
        Err(DomainError::UnknownProjectName { .. })
    ));
}

#[test]
fn a_project_and_a_label_from_another_project_are_refused() {
    let sandbox = Sandbox::new("ref-add-mismatch");
    let board = sandbox.empty_board();
    two_projects(&board);
    let error = board
        .add(
            "t",
            AddTicket {
                project: Some("Retro"),
                label: Some("alpha"),
                ..AddTicket::default()
            },
        )
        .unwrap_err();
    match error {
        DomainError::LabelProjectMismatch { label_project, .. } => {
            assert_eq!(
                label_project, "Probe",
                "the refusal names the label's project"
            );
        }
        other => panic!("expected labelProjectMismatch, got {other:?}"),
    }
    // The two agreeing is fine.
    let ok = board
        .add(
            "t",
            AddTicket {
                project: Some("Probe"),
                label: Some("alpha"),
                ..AddTicket::default()
            },
        )
        .unwrap();
    assert_eq!(ok.reference.as_deref(), Some("PRB-1"));
}

#[test]
fn the_ref_parser_is_case_insensitive_on_the_key_and_digit_only_on_the_number() {
    assert_eq!(TicketRef::parse("prb-2"), Some(("PRB".to_string(), 2)));
    assert_eq!(TicketRef::parse(" PRB-2 "), Some(("PRB".to_string(), 2)));
    assert_eq!(TicketRef::parse("PRB-0"), None);
    assert_eq!(TicketRef::parse("PRB-x"), None);
    assert_eq!(TicketRef::parse("PRB"), None);
    assert_eq!(TicketRef::parse("not-a-uuid"), None);
    assert_eq!(TicketRef::render("TAN", 42), "TAN-42");
}

#[test]
fn a_done_ticket_keeps_its_ref_and_its_number() {
    let sandbox = Sandbox::new("ref-done");
    let board = sandbox.empty_board();
    board.add_project("Probe", Some("PRB")).unwrap();
    let ticket = board.add("done soon", in_project("Probe")).unwrap();
    let done = board.complete(&ticket.id.to_string()).unwrap();
    assert_eq!(done.reference.as_deref(), Some("PRB-1"));
    assert_eq!(done.status, Status::Done);
}
