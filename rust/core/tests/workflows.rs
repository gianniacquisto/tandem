//! The workflow model (TAN-328): the seeded default, the whole-document
//! validation, the active-workflow fallback, the switch refusals and the board's
//! status validation.
//!
//! The store's own schema seam covers the v6 to v7 migration; these are the
//! domain claims over a fresh home.

mod support;

use support::Sandbox;
use tandem_core::domain::sprint::{SprintMode, Sprints};
use tandem_core::domain::workflow::{
    Pass, StageBinding, StageRole, WorkflowDraft, WorkflowStage, Workflows, default_workflow,
};
use tandem_core::domain::{AddTicket, DomainError, Status};
use tandem_core::store::{Actor, NewSetting};

fn stage(key: &str, label: &str, role: StageRole) -> WorkflowStage {
    WorkflowStage {
        key: key.to_string(),
        label: label.to_string(),
        role,
        binding: None,
    }
}

/// A valid workflow document: one intake, one parked, one finished, plus the
/// three run roles so it is also runnable.
fn minimal(name: &str) -> WorkflowDraft {
    WorkflowDraft::new(
        name,
        vec![
            stage("inbox", "Inbox", StageRole::Intake),
            stage("parked", "Parked", StageRole::Parked),
            stage("queued", "Queued", StageRole::Ready),
            stage("doing", "Doing", StageRole::Active),
            stage("checking", "Checking", StageRole::Review),
            stage("shipped", "Shipped", StageRole::Finished),
        ],
    )
}

#[test]
fn a_fresh_home_seeds_the_agile_default_in_the_board_order() {
    let sandbox = Sandbox::new("workflow-seed");
    let workflows = Workflows::new(&sandbox.store());

    let list = workflows.list().expect("the workflows read");
    assert_eq!(list.len(), 1, "a fresh home carries the one default");
    let agile = &list[0];
    assert_eq!(agile.name, "agile");
    assert!(agile.is_default());
    assert_eq!(
        agile.stage_keys(),
        vec![
            "backlog",
            "on-hold",
            "ready",
            "in-progress",
            "in-review",
            "done"
        ],
        "the seed follows UI-15's board order"
    );
    assert_eq!(agile.stage_key(StageRole::Intake), Some("backlog"));
    assert_eq!(agile.stage_key(StageRole::Parked), Some("on-hold"));
    assert_eq!(agile.stage_key(StageRole::Ready), Some("ready"));
    assert_eq!(agile.stage_key(StageRole::Active), Some("in-progress"));
    assert_eq!(agile.stage_key(StageRole::Review), Some("in-review"));
    assert_eq!(agile.stage_key(StageRole::Finished), Some("done"));

    let backlog = &agile.stages[0];
    assert_eq!(
        backlog.binding,
        Some(StageBinding {
            role: "refiner".to_string(),
            pass: Pass::Refine,
        })
    );
    assert_eq!(agile.stages[1].binding, None, "on-hold is parked, unbound");
    assert!(
        agile.is_runnable(),
        "the default carries the three run roles"
    );
    let memory = default_workflow();
    assert_eq!(agile.id, memory.id);
    assert_eq!(agile.name, memory.name);
    assert_eq!(
        agile.stages, memory.stages,
        "the seed is the in-memory default"
    );
}

#[test]
fn the_active_workflow_is_the_default() {
    let sandbox = Sandbox::new("workflow-active");
    let workflows = Workflows::new(&sandbox.store());

    let active = workflows.active().expect("the active workflow reads");
    assert_eq!(active.id, default_workflow().id);
    assert_eq!(active.name, "agile");
}

#[test]
fn a_broken_active_row_reads_as_the_default() {
    let sandbox = Sandbox::new("workflow-broken");
    let store = sandbox.store();
    store
        .write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: tandem_core::domain::workflow::ACTIVE_WORKFLOW_KEY,
                value: "no-such-workflow",
            })
        })
        .expect("the broken row lands");

    let active = Workflows::new(&store).active().expect("the read answers");
    assert_eq!(active.id, default_workflow().id);
    assert_eq!(active.stages, default_workflow().stages);
    assert_eq!(
        active.name, "agile",
        "a broken active row never hides the board"
    );
}

#[test]
fn a_custom_stage_round_trips_and_the_board_validates_against_the_active_workflow() {
    let sandbox = Sandbox::new("workflow-custom");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());

    let created = workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("a valid document writes");
    assert!(!created.is_default());

    // The default does not carry `doing`: the board refuses it naming the keys.
    let refused = board
        .add(
            "first",
            AddTicket {
                status: Status::parse("doing").expect("a valid key"),
                ..AddTicket::default()
            },
        )
        .expect_err("the default workflow does not carry the custom stage");
    match refused {
        DomainError::InvalidStage { value, stages } => {
            assert_eq!(value, "doing");
            assert!(stages.contains(&"backlog".to_string()));
        }
        other => panic!("unexpected refusal: {other:?}"),
    }

    workflows
        .use_workflow(&Actor::Human, "flow")
        .expect("the custom workflow activates on an empty board");

    let added = board
        .add(
            "second",
            AddTicket {
                status: Status::parse("doing").expect("a valid key"),
                ..AddTicket::default()
            },
        )
        .expect("the active workflow carries the custom stage");
    assert_eq!(added.status.as_str(), "doing");

    let listed = board
        .list(&tandem_core::domain::TicketQuery {
            statuses: vec![Status::parse("doing").expect("a valid key")],
            ..Default::default()
        })
        .expect("the listing reads");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "second");
}

#[test]
fn a_new_ticket_lands_in_the_first_intake_stage() {
    let sandbox = Sandbox::new("workflow-landing");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());

    assert_eq!(
        board.landing_status().expect("the landing reads"),
        Status::Backlog,
        "the default lands backlog"
    );

    let mut document = minimal("flow");
    document
        .stages
        .insert(0, stage("selected", "Selected", StageRole::Intake));
    workflows
        .set(&Actor::Human, None, document)
        .expect("two intake stages are legal");
    workflows
        .use_workflow(&Actor::Human, "flow")
        .expect("the workflow activates");

    assert_eq!(
        board.landing_status().expect("the landing reads"),
        Status::parse("selected").expect("a valid key"),
        "the first intake stage in stage order lands the ticket"
    );
}

#[test]
fn the_whole_document_validation_names_each_refusal() {
    let sandbox = Sandbox::new("workflow-validation");
    let workflows = Workflows::new(&sandbox.store());

    let empty = WorkflowDraft::new("empty", Vec::new());
    assert_eq!(
        workflows.set(&Actor::Human, None, empty).unwrap_err(),
        DomainError::EmptyWorkflowStages
    );

    let mut duplicate = minimal("duplicate");
    duplicate.stages[1].key = duplicate.stages[0].key.clone();
    assert!(matches!(
        workflows.set(&Actor::Human, None, duplicate),
        Err(DomainError::DuplicateStageKey(_))
    ));

    let mut blank_key = minimal("blank-key");
    blank_key.stages[0].key = "  ".to_string();
    assert!(matches!(
        workflows.set(&Actor::Human, None, blank_key),
        Err(DomainError::EmptyStageKey)
    ));

    let mut blank_label = minimal("blank-label");
    blank_label.stages[0].label = "  ".to_string();
    assert!(matches!(
        workflows.set(&Actor::Human, None, blank_label),
        Err(DomainError::EmptyStageLabel(_))
    ));

    let mut no_intake = minimal("no-intake");
    no_intake
        .stages
        .retain(|stage| stage.role != StageRole::Intake);
    assert!(matches!(
        workflows.set(&Actor::Human, None, no_intake),
        Err(DomainError::StageRoleCount { role, .. }) if role == "intake"
    ));

    let mut two_parked = minimal("two-parked");
    two_parked
        .stages
        .push(stage("parked-2", "Parked again", StageRole::Parked));
    assert!(matches!(
        workflows.set(&Actor::Human, None, two_parked),
        Err(DomainError::StageRoleCount { role, count: 2, .. }) if role == "parked"
    ));

    let mut no_finished = minimal("no-finished");
    no_finished
        .stages
        .retain(|stage| stage.role != StageRole::Finished);
    assert!(matches!(
        workflows.set(&Actor::Human, None, no_finished),
        Err(DomainError::StageRoleCount { role, count: 0, .. }) if role == "finished"
    ));

    let mut unknown_binding = minimal("unknown-binding");
    unknown_binding.stages[0].binding = Some(StageBinding {
        role: "nobody".to_string(),
        pass: Pass::Refine,
    });
    assert!(matches!(
        workflows.set(&Actor::Human, None, unknown_binding),
        Err(DomainError::UnknownBindingRole { .. })
    ));

    let original = Workflows::new(&sandbox.store())
        .get("agile")
        .expect("the default still reads");
    assert_eq!(
        original.stages.len(),
        6,
        "a refused write leaves the stored workflow whole"
    );
}

#[test]
fn deleting_a_stage_that_holds_tickets_is_refused_with_its_count() {
    let sandbox = Sandbox::new("workflow-occupied");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());
    board
        .add("waiting", AddTicket::default())
        .expect("the ticket lands backlog");

    let mut document = default_draft();
    document.stages.retain(|stage| stage.key != "backlog");
    document
        .stages
        .push(stage("inbox", "Inbox", StageRole::Intake));

    match workflows.set(&Actor::Human, Some("agile"), document) {
        Err(DomainError::StageOccupied { key, count }) => {
            assert_eq!(key, "backlog");
            assert_eq!(count, 1);
        }
        other => panic!("the occupied stage must be refused, got {other:?}"),
    }
    assert_eq!(
        workflows
            .get("agile")
            .expect("the default still reads")
            .stage_keys(),
        default_workflow().stage_keys(),
        "the refused write left the stages in place"
    );
}

#[test]
fn activating_a_workflow_is_refused_while_a_sprint_is_open() {
    let sandbox = Sandbox::new("workflow-sprint");
    let workflows = Workflows::new(&sandbox.store());
    workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");
    Sprints::new(&sandbox.store())
        .open(&Actor::Human, "Tandem", SprintMode::Serial, None, None)
        .expect("the sprint opens");

    match workflows.use_workflow(&Actor::Human, "flow") {
        Err(DomainError::WorkflowHasOpenSprint { lanes }) => {
            assert_eq!(lanes, vec!["Tandem".to_string()]);
        }
        other => panic!("an open sprint must refuse the switch, got {other:?}"),
    }
}

#[test]
fn activating_a_workflow_that_lacks_an_occupied_stage_is_refused_with_counts() {
    let sandbox = Sandbox::new("workflow-missing");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());
    board
        .add("waiting", AddTicket::default())
        .expect("the ticket lands backlog");
    workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");

    match workflows.use_workflow(&Actor::Human, "flow") {
        Err(DomainError::WorkflowMissingStages { stages }) => {
            assert_eq!(stages, vec![("backlog".to_string(), 1)]);
        }
        other => panic!("a missing occupied stage must refuse, got {other:?}"),
    }
    assert_eq!(
        workflows.active().expect("the active reads").name,
        "agile",
        "the refused switch left the default active"
    );
}

#[test]
fn the_default_and_the_active_workflow_cannot_be_deleted() {
    let sandbox = Sandbox::new("workflow-delete");
    let workflows = Workflows::new(&sandbox.store());

    assert!(matches!(
        workflows.delete(&Actor::Human, "agile"),
        Err(DomainError::WorkflowIsDefault(_))
    ));

    workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");
    workflows
        .use_workflow(&Actor::Human, "flow")
        .expect("the workflow activates");
    assert!(matches!(
        workflows.delete(&Actor::Human, "flow"),
        Err(DomainError::WorkflowIsActive(_))
    ));
}

fn default_draft() -> WorkflowDraft {
    let workflow = default_workflow();
    WorkflowDraft {
        name: workflow.name,
        scope: workflow.scope,
        stages: workflow.stages,
    }
}
