//! The workflow model (TAN-328): the seeded default, the whole-document
//! validation, the active-workflow fallback, the switch refusals and the board's
//! status validation.
//!
//! The store's own schema seam covers the v6 to v7 migration; these are the
//! domain claims over a fresh home.

mod support;

use support::Sandbox;
use tandem_core::domain::workflow::{
    Pass, StageBinding, StageRole, WorkflowDraft, WorkflowStage, Workflows, default_workflow,
};
use tandem_core::domain::{AddTicket, DomainError, Edit, Filter, SetFields, Status, TicketQuery};
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
fn a_fresh_home_ends_with_the_seven_stage_agile_default() {
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
            "selected",
            "ready",
            "in-progress",
            "in-review",
            "done"
        ],
        "the migration inserts selected before ready, leaving UI-15's order intact"
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
    let selected = &agile.stages[2];
    assert_eq!(selected.key, "selected");
    assert_eq!(selected.label, "Selected for development");
    assert_eq!(selected.role, StageRole::Intake);
    assert_eq!(selected.binding, None, "selected carries no binding");
    assert_eq!(agile.stages[3].key, "ready", "ready follows selected");
    assert!(
        agile.is_runnable(),
        "the default carries the three run roles"
    );
    let memory = default_workflow();
    assert_eq!(agile.id, memory.id);
    assert_eq!(agile.name, memory.name);
    assert_eq!(
        agile.stages, memory.stages,
        "the migrated default is the in-memory default"
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
fn a_ticket_with_no_status_lands_in_the_first_intake_stage() {
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
fn workflow_set_accepts_two_intake_stages_and_refuses_zero() {
    let sandbox = Sandbox::new("workflow-intake-count");
    let workflows = Workflows::new(&sandbox.store());

    let mut two_intakes = minimal("flow");
    two_intakes.stages.insert(
        1,
        stage("selected", "Selected for development", StageRole::Intake),
    );
    let written = workflows
        .set(&Actor::Human, None, two_intakes)
        .expect("two intake stages are legal");
    assert_eq!(
        written
            .stages
            .iter()
            .filter(|stage| stage.role == StageRole::Intake)
            .count(),
        2
    );

    let mut no_intake = minimal("no-intake");
    no_intake
        .stages
        .retain(|stage| stage.role != StageRole::Intake);
    assert!(matches!(
        workflows.set(&Actor::Human, None, no_intake),
        Err(DomainError::StageRoleCount { role, count: 0, .. }) if role == "intake"
    ));
}

#[test]
fn a_ticket_moves_to_and_lists_by_the_selected_stage() {
    let sandbox = Sandbox::new("selected-roundtrip");
    let board = sandbox.board();
    let selected = Status::Custom("selected".to_string());

    let added = board
        .add(
            "promoted for the next refine",
            AddTicket {
                status: selected.clone(),
                ..AddTicket::default()
            },
        )
        .expect("the default workflow declares selected");
    assert_eq!(added.status, selected);

    let parked = board
        .add("later", AddTicket::default())
        .expect("the ticket lands backlog");
    board
        .set(
            &parked.id.to_string(),
            SetFields {
                status: Edit::Set(selected.clone()),
                ..SetFields::default()
            },
        )
        .expect("a status move is free-form");

    let listed = board
        .list(&TicketQuery {
            statuses: vec![selected.clone()],
            ..TicketQuery::default()
        })
        .expect("the status listing reads");
    let titles: Vec<&str> = listed.iter().map(|ticket| ticket.title.as_str()).collect();
    assert_eq!(titles.len(), 2, "both tickets are in the stage: {titles:?}");
    assert!(titles.contains(&"promoted for the next refine"));
    assert!(titles.contains(&"later"));
    assert!(listed.iter().all(|ticket| ticket.status == selected));
}

#[test]
fn an_unknown_stage_names_selected_among_the_valid_keys() {
    let sandbox = Sandbox::new("selected-unknown");
    let board = sandbox.board();

    let refused = board
        .add(
            "nowhere",
            AddTicket {
                status: Status::Custom("nowhere".to_string()),
                ..AddTicket::default()
            },
        )
        .expect_err("an unknown stage is refused");
    match refused {
        DomainError::InvalidStage { value, stages } => {
            assert_eq!(value, "nowhere");
            assert!(
                stages.iter().any(|stage| stage == "selected"),
                "the refusal names selected: {stages:?}"
            );
        }
        other => panic!("expected InvalidStage, got {other:?}"),
    }
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
        7,
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

#[test]
fn complete_and_reopen_follow_the_active_workflows_roles() {
    let sandbox = Sandbox::new("workflow-complete");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());
    workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");
    workflows
        .use_workflow(&Actor::Human, "flow")
        .expect("the workflow activates");

    let added = board
        .add(
            "a shipped ticket",
            AddTicket {
                status: Status::parse("doing").expect("a key the flow carries"),
                ..AddTicket::default()
            },
        )
        .expect("the ticket lands");
    let handle = added.id.to_string();

    let completed = board.complete(&handle).expect("complete writes");
    assert_eq!(
        completed.status.as_str(),
        "shipped",
        "complete moves to the workflow's finished stage, not the literal done"
    );
    assert!(completed.done(), "a custom finished stage is done");
    assert!(
        completed.completed_at.is_some(),
        "entering finished stamps the time"
    );
    assert_eq!(
        board
            .list_filter(Filter::Done)
            .expect("the done filter reads")
            .len(),
        1,
        "the done filter finds a custom finished key"
    );

    let reopened = board.reopen(&handle).expect("reopen writes");
    assert_eq!(
        reopened.status.as_str(),
        "inbox",
        "reopen lands in the workflow's intake stage"
    );
    assert!(!reopened.done());
    assert!(
        reopened.completed_at.is_none(),
        "leaving finished clears the time"
    );

    board.complete(&handle).expect("complete again");
    assert_eq!(board.archive_done().expect("archive done"), 1);
}

#[test]
fn a_custom_finished_stage_lifts_a_block() {
    let sandbox = Sandbox::new("workflow-block-lift");
    let board = sandbox.board();
    let workflows = Workflows::new(&sandbox.store());
    workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");
    workflows
        .use_workflow(&Actor::Human, "flow")
        .expect("the workflow activates");

    let blocker = board
        .add(
            "the blocker",
            AddTicket {
                status: Status::parse("doing").expect("a key the flow carries"),
                ..AddTicket::default()
            },
        )
        .expect("the blocker lands");
    let dependent = board
        .add(
            "the dependent",
            AddTicket {
                status: Status::parse("inbox").expect("a key the flow carries"),
                ..AddTicket::default()
            },
        )
        .expect("the dependent lands");
    board
        .block(&dependent.id.to_string(), &blocker.id.to_string())
        .expect("the block lands");

    board
        .complete(&blocker.id.to_string())
        .expect("the blocker completes");
    let lifted = board
        .ticket(&dependent.id.to_string())
        .expect("the dependent reads");
    assert!(
        !lifted.blocked(),
        "a ticket in a custom finished stage lifts the block"
    );
}

#[test]
fn an_update_returns_the_author_the_next_read_carries() {
    let sandbox = Sandbox::new("workflow-author");
    let workflows = Workflows::new(&sandbox.store());
    let created = workflows
        .set(&Actor::Human, None, minimal("flow"))
        .expect("the workflow writes");

    let mut renamed = minimal("flow");
    renamed.name = "flow renamed".to_string();
    let builder = Actor::Agent("builder".to_string());
    let updated = workflows
        .set(&builder, Some(&created.id), renamed)
        .expect("the update writes");

    let read = workflows.get(&created.id).expect("the read answers");
    assert_eq!(updated, read, "the receipt is the next read");
    assert_eq!(updated.author, "builder", "the update records the writer");
}

#[test]
fn the_workflow_reads_carry_the_snapshots_revision() {
    let sandbox = Sandbox::new("workflow-revision");
    let store = sandbox.store();
    store
        .write(&Actor::Human, |write| {
            write.upsert_setting(NewSetting {
                key: "k",
                value: "v",
            })
        })
        .expect("the write lands");
    let expected = store.last_revision();
    assert!(expected > 0, "the write moved the revision");

    let workflows = Workflows::new(&store);
    assert_eq!(
        workflows.list_snapshot().expect("the list reads").1,
        expected
    );
    assert_eq!(
        workflows.get_snapshot("agile").expect("the get reads").1,
        expected
    );
    assert_eq!(
        workflows.active_snapshot().expect("the active reads").1,
        expected
    );
}

fn default_draft() -> WorkflowDraft {
    let workflow = default_workflow();
    WorkflowDraft {
        name: workflow.name,
        scope: workflow.scope,
        stages: workflow.stages,
    }
}
