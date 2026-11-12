//! The workflow: the persistent process the board's stages belong to.
//!
//! A workflow is a UUID, a name, a scope, an author, timestamps and an ordered
//! stage list (TAN-328). A stage has a stable key, an editable label, one of six
//! roles and an optional binding to a roster role and one of the three passes.
//! The active workflow is the `active_workflow` settings row; a missing or
//! unreadable row reads as the seeded `agile` default, so a broken row never
//! hides the board.
//!
//! **The stage set is data, not code.** The board, the list filter and the
//! ticket status validation read the active workflow's stages, so a custom stage
//! appears without a second build. The default's stages are the seven keys the
//! board draws after TAN-327, in UI-15's order with `selected` before `ready`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::store::records::sprint_rows;
use crate::store::schema::{DEFAULT_WORKFLOW_CREATED, DEFAULT_WORKFLOW_ID, current_default_stages};
use crate::store::workflows::{StageRow, WorkflowRow, workflow_rows};
use crate::store::{
    Actor, NewSetting, NewWorkflow, NewWorkflowStage, Store, StoreError, timestamp,
};

pub use crate::store::schema::DEFAULT_WORKFLOW_NAME;

use super::error::{DomainError, DomainResult};
use super::settings::Settings;
use super::sprint::SprintStatus;
use super::ticket::Status;

/// The settings key that names the active workflow.
pub const ACTIVE_WORKFLOW_KEY: &str = "active_workflow";

/// One stage's role: what the stage means to the process, not its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StageRole {
    Intake,
    Parked,
    Ready,
    Active,
    Review,
    Finished,
}

impl StageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Parked => "parked",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Review => "review",
            Self::Finished => "finished",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "intake" => Self::Intake,
            "parked" => Self::Parked,
            "ready" => Self::Ready,
            "active" => Self::Active,
            "review" => Self::Review,
            "finished" => Self::Finished,
            _ => return None,
        })
    }
}

/// One of the three passes a stage may bind to a roster role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Pass {
    Refine,
    Build,
    Review,
}

impl Pass {
    pub const VALUES: [&'static str; 3] = ["refine", "build", "review"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Refine => "refine",
            Self::Build => "build",
            Self::Review => "review",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "refine" => Self::Refine,
            "build" => Self::Build,
            "review" => Self::Review,
            _ => return None,
        })
    }
}

/// The agent a stage hands to: a roster role and the pass it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StageBinding {
    pub role: String,
    pub pass: Pass,
}

/// One stage of a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStage {
    /// The stable stored word, and the ticket status a ticket in this stage
    /// carries. A key change is a new stage.
    pub key: String,
    /// The editable word the board and every picker draw.
    pub label: String,
    pub role: StageRole,
    pub binding: Option<StageBinding>,
}

/// A workflow: the persistent process, its stages in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    /// `home` today; a project's workflow lands with `projects.workflow`.
    pub scope: String,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
    pub stages: Vec<WorkflowStage>,
}

impl Workflow {
    pub fn is_default(&self) -> bool {
        self.id == DEFAULT_WORKFLOW_ID
    }

    /// The key of the stage with `role`, if the workflow carries one.
    pub fn stage_key(&self, role: StageRole) -> Option<&str> {
        self.stages
            .iter()
            .find(|stage| stage.role == role)
            .map(|stage| stage.key.as_str())
    }

    /// The key of every stage, in order.
    pub fn stage_keys(&self) -> Vec<String> {
        self.stages.iter().map(|stage| stage.key.clone()).collect()
    }

    /// The stage a new ticket lands in: the first intake stage in stage order.
    pub fn landing_key(&self) -> &str {
        self.stage_key(StageRole::Intake).unwrap_or("backlog")
    }

    /// The key of the stage a completed ticket sits in: the finished stage, or
    /// `done` when a hand-edited store carries none.
    pub fn finished_key(&self) -> &str {
        self.stage_key(StageRole::Finished).unwrap_or("done")
    }

    /// Whether the workflow can run: ready, active and review are required.
    pub fn is_runnable(&self) -> bool {
        [StageRole::Ready, StageRole::Active, StageRole::Review]
            .into_iter()
            .all(|role| self.stage_key(role).is_some())
    }

    fn from_rows(row: WorkflowRow, stages: &[&StageRow]) -> DomainResult<Self> {
        let stages = stages
            .iter()
            .map(|stage| {
                let role = StageRole::parse(&stage.role).ok_or_else(|| {
                    DomainError::Io(format!(
                        "workflow \"{}\" carries an unknown stage role \"{}\"",
                        row.name, stage.role
                    ))
                })?;
                let binding = match (&stage.binding_role, &stage.binding_pass) {
                    (Some(role), Some(pass)) => {
                        let pass = Pass::parse(pass).ok_or_else(|| {
                            DomainError::Io(format!(
                                "workflow \"{}\" carries an unknown binding pass \"{pass}\"",
                                row.name
                            ))
                        })?;
                        Some(StageBinding {
                            role: role.clone(),
                            pass,
                        })
                    }
                    (None, None) => None,
                    _ => {
                        return Err(DomainError::Io(format!(
                            "workflow \"{}\" carries half a binding",
                            row.name
                        )));
                    }
                };
                Ok(WorkflowStage {
                    key: stage.key.clone(),
                    label: stage.label.clone(),
                    role,
                    binding,
                })
            })
            .collect::<DomainResult<Vec<_>>>()?;
        Ok(Self {
            id: row.id,
            name: row.name,
            scope: row.scope,
            author: row.author,
            created_at: row.created_at,
            updated_at: row.updated_at,
            stages,
        })
    }
}

/// The document `workflow set` writes: the name, the scope and the whole ordered
/// stage list. The id is not part of the document; the verb's `--id` names the
/// workflow a write updates, and an absent id creates one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDraft {
    pub name: String,
    pub scope: String,
    pub stages: Vec<WorkflowStage>,
}

impl WorkflowDraft {
    pub fn new(name: impl Into<String>, stages: Vec<WorkflowStage>) -> Self {
        Self {
            name: name.into(),
            scope: "home".to_string(),
            stages,
        }
    }
}

/// The default `agile` workflow, in memory: the fallback when the store has no
/// workflow rows, and the shape a fresh home ends with. It reads
/// [`current_default_stages`], the v7 six plus `selected` before `ready`, so
/// the migrated store and this fallback cannot disagree.
pub fn default_workflow() -> Workflow {
    Workflow {
        id: DEFAULT_WORKFLOW_ID.to_string(),
        name: DEFAULT_WORKFLOW_NAME.to_string(),
        scope: "home".to_string(),
        author: "human".to_string(),
        created_at: DEFAULT_WORKFLOW_CREATED.to_string(),
        updated_at: DEFAULT_WORKFLOW_CREATED.to_string(),
        stages: current_default_stages()
            .iter()
            .map(|stage| WorkflowStage {
                key: stage.key.to_string(),
                label: stage.label.to_string(),
                role: StageRole::parse(stage.role).expect("the seeded stage role is valid"),
                binding: stage.binding.map(|(role, pass)| StageBinding {
                    role: role.to_string(),
                    pass: Pass::parse(pass).expect("the seeded binding pass is valid"),
                }),
            })
            .collect(),
    }
}

/// The workflows over the home's store.
#[derive(Debug, Clone)]
pub struct Workflows {
    store: Store,
}

impl Workflows {
    pub fn new(store: &Store) -> Self {
        Self {
            store: store.clone(),
        }
    }

    /// Every workflow, in stored order. A home with no store reads as the
    /// default alone, so a fresh board still has its stage set.
    pub fn list(&self) -> DomainResult<Vec<Workflow>> {
        self.list_snapshot().map(|(workflows, _)| workflows)
    }

    /// [`list`](Self::list) and the revision its rows are a snapshot of, read in
    /// one transaction. A fallback to the in-memory default carries the store's
    /// own revision so a read of a v6 home is not reported as revision zero.
    pub fn list_snapshot(&self) -> DomainResult<(Vec<Workflow>, i64)> {
        match self.read_all()? {
            Some((workflows, revision)) if !workflows.is_empty() => Ok((workflows, revision)),
            Some((_, revision)) => Ok((vec![default_workflow()], revision)),
            None => Ok((vec![default_workflow()], 0)),
        }
    }

    /// The workflow a handle names: its UUID or its name. An unknown handle is
    /// refused with the known names.
    pub fn get(&self, handle: &str) -> DomainResult<Workflow> {
        self.get_snapshot(handle).map(|(workflow, _)| workflow)
    }

    /// [`get`](Self::get) and the revision its rows are a snapshot of.
    pub fn get_snapshot(&self, handle: &str) -> DomainResult<(Workflow, i64)> {
        let handle = handle.trim();
        let (workflows, revision) = self.list_snapshot()?;
        let found = workflows
            .iter()
            .find(|workflow| workflow.id == handle || workflow.name == handle)
            .cloned()
            .ok_or_else(|| DomainError::UnknownWorkflow {
                handle: handle.to_string(),
                workflows: workflows
                    .iter()
                    .map(|workflow| workflow.name.clone())
                    .collect(),
            })?;
        Ok((found, revision))
    }

    /// The active workflow: the `active_workflow` row's target, or the default
    /// when the row is absent, unreadable or names no workflow.
    pub fn active(&self) -> DomainResult<Workflow> {
        self.active_snapshot().map(|(workflow, _)| workflow)
    }

    /// [`active`](Self::active) and the revision its rows are a snapshot of.
    pub fn active_snapshot(&self) -> DomainResult<(Workflow, i64)> {
        let (workflows, revision) = self.list_snapshot()?;
        let stored = self
            .store
            .setting(ACTIVE_WORKFLOW_KEY)
            .map_err(store_error)?;
        let wanted = stored
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(wanted) = wanted
            && let Some(found) = workflows
                .iter()
                .find(|workflow| workflow.id == wanted || workflow.name == wanted)
        {
            return Ok((found.clone(), revision));
        }
        let found = workflows
            .iter()
            .find(|workflow| workflow.is_default())
            .cloned()
            .or_else(|| workflows.first().cloned())
            .unwrap_or_else(default_workflow);
        Ok((found, revision))
    }

    /// The stage keys a ticket status is validated against: the active
    /// workflow's, in order.
    pub fn active_stage_keys(&self) -> DomainResult<Vec<String>> {
        Ok(self.active()?.stage_keys())
    }

    /// The status a new ticket lands in: the active workflow's first intake
    /// stage.
    pub fn landing_status(&self) -> DomainResult<Status> {
        Status::parse(self.active()?.landing_key())
            .map_err(|_| DomainError::InvalidStatus(String::new()))
    }

    /// Refuse a status the active workflow does not declare, naming the valid
    /// keys.
    pub fn validate_status(&self, status: &Status) -> DomainResult<()> {
        let stages = self.active_stage_keys()?;
        if stages.iter().any(|key| key == status.as_str()) {
            return Ok(());
        }
        Err(DomainError::InvalidStage {
            value: status.as_str().to_string(),
            stages,
        })
    }

    /// `workflow set`: write the whole document, all or nothing.
    ///
    /// An absent `id` creates a workflow; a present one updates the workflow it
    /// names. Validation names every refusal, and the occupied-stage check runs
    /// inside the write's transaction so a ticket that moved in does not escape
    /// it.
    pub fn set(
        &self,
        actor: &Actor,
        id: Option<&str>,
        draft: WorkflowDraft,
    ) -> DomainResult<Workflow> {
        let name = draft.name.trim().to_string();
        if name.is_empty() {
            return Err(DomainError::EmptyWorkflowName);
        }
        let scope = draft.scope.trim();
        let scope = if scope.is_empty() { "home" } else { scope };
        if scope != "home" {
            return Err(DomainError::InvalidWorkflowScope(scope.to_string()));
        }
        let mut stages = draft.stages;
        if stages.is_empty() {
            return Err(DomainError::EmptyWorkflowStages);
        }
        // Clean the document's text before validation, so the write stores
        // exactly what the receipt answers.
        for stage in &mut stages {
            stage.key = stage.key.trim().to_string();
            stage.label = stage.label.trim().to_string();
            if stage.key.is_empty() {
                return Err(DomainError::EmptyStageKey);
            }
            if stage.label.is_empty() {
                return Err(DomainError::EmptyStageLabel(stage.key.clone()));
            }
        }
        let mut keys = HashSet::new();
        for stage in &stages {
            if !keys.insert(stage.key.clone()) {
                return Err(DomainError::DuplicateStageKey(stage.key.clone()));
            }
        }
        let counts = |role: StageRole| stages.iter().filter(|stage| stage.role == role).count();
        if counts(StageRole::Intake) == 0 {
            return Err(DomainError::StageRoleCount {
                role: "intake".to_string(),
                count: 0,
                expected: "at least one".to_string(),
            });
        }
        for (role, expected) in [
            (StageRole::Parked, "exactly one"),
            (StageRole::Finished, "exactly one"),
        ] {
            let count = counts(role);
            if count != 1 {
                return Err(DomainError::StageRoleCount {
                    role: role.as_str().to_string(),
                    count,
                    expected: expected.to_string(),
                });
            }
        }
        let roster = Settings::new(&self.store).roster()?;
        // A pass has already been parsed by the wire: a hand-built document
        // carries only the three variants, so serde refuses an unknown pass
        // before the domain sees it. Only the binding's roster role is checked.
        for stage in &stages {
            if let Some(binding) = &stage.binding
                && !roster.iter().any(|name| name == &binding.role)
            {
                return Err(DomainError::UnknownBindingRole {
                    role: binding.role.clone(),
                    roster,
                });
            }
        }

        let handle = id.map(str::trim).filter(|value| !value.is_empty());
        let mut failure: Option<DomainError> = None;
        let mut written: Option<Workflow> = None;
        let outcome = self.store.write(actor, |write| {
            let (rows, stage_rows) = workflow_rows(write.connection)
                .map_err(|error| StoreError::Io(error.to_string()))?;
            let known: Vec<Workflow> = rows
                .iter()
                .map(|row| {
                    let own: Vec<&StageRow> = stage_rows
                        .iter()
                        .filter(|stage| stage.workflow_id == row.id)
                        .collect();
                    Workflow::from_rows(row.clone(), &own)
                })
                .collect::<DomainResult<Vec<_>>>()
                .map_err(|error| StoreError::Io(error.to_string()))?;
            let existing = match handle {
                Some(handle) => match known
                    .iter()
                    .find(|workflow| workflow.id == handle || workflow.name == handle)
                {
                    Some(found) => Some(found.clone()),
                    None => {
                        failure = Some(DomainError::UnknownWorkflow {
                            handle: handle.to_string(),
                            workflows: known.iter().map(|workflow| workflow.name.clone()).collect(),
                        });
                        return Err(StoreError::Io("the domain refused the write".to_string()));
                    }
                },
                None => None,
            };
            if known.iter().any(|workflow| {
                workflow.name == name && Some(&workflow.id) != existing.as_ref().map(|w| &w.id)
            }) {
                failure = Some(DomainError::DuplicateWorkflowName(name.clone()));
                return Err(StoreError::Io("the domain refused the write".to_string()));
            }
            // A stage holding tickets cannot disappear from the document: a key
            // change is a new stage and a move.
            if let Some(existing) = &existing {
                let occupied = ticket_stage_counts(write.connection)
                    .map_err(|error| StoreError::Io(error.to_string()))?;
                for stage in &existing.stages {
                    let count = occupied.get(&stage.key).copied().unwrap_or(0);
                    if count > 0 && !keys.contains(&stage.key) {
                        failure = Some(DomainError::StageOccupied {
                            key: stage.key.clone(),
                            count,
                        });
                        return Err(StoreError::Io("the domain refused the write".to_string()));
                    }
                }
            }
            let now = timestamp::now();
            let workflow = match &existing {
                Some(existing) => {
                    write
                        .update_workflow(NewWorkflow {
                            id: &existing.id,
                            name: &name,
                            scope: &existing.scope,
                            created_at: &existing.created_at,
                            updated_at: &now,
                        })
                        .map_err(|error| StoreError::Io(error.to_string()))?;
                    Workflow {
                        id: existing.id.clone(),
                        name: name.clone(),
                        scope: existing.scope.clone(),
                        author: actor.as_str().to_string(),
                        created_at: existing.created_at.clone(),
                        updated_at: now.clone(),
                        stages: stages.clone(),
                    }
                }
                None => {
                    let id = Uuid::new_v4().to_string();
                    write
                        .insert_workflow(NewWorkflow {
                            id: &id,
                            name: &name,
                            scope,
                            created_at: &now,
                            updated_at: &now,
                        })
                        .map_err(|error| StoreError::Io(error.to_string()))?;
                    Workflow {
                        id,
                        name: name.clone(),
                        scope: scope.to_string(),
                        author: actor.as_str().to_string(),
                        created_at: now.clone(),
                        updated_at: now.clone(),
                        stages: stages.clone(),
                    }
                }
            };
            let rows: Vec<NewWorkflowStage<'_>> = workflow
                .stages
                .iter()
                .enumerate()
                .map(|(position, stage)| NewWorkflowStage {
                    workflow_id: &workflow.id,
                    position: position as i64,
                    key: &stage.key,
                    label: &stage.label,
                    role: stage.role.as_str(),
                    binding_role: stage.binding.as_ref().map(|binding| binding.role.as_str()),
                    binding_pass: stage.binding.as_ref().map(|binding| binding.pass.as_str()),
                })
                .collect();
            write
                .replace_workflow_stages(&workflow.id, &rows)
                .map_err(|error| StoreError::Io(error.to_string()))?;
            written = Some(workflow);
            Ok(())
        });
        if let Some(failure) = failure {
            return Err(failure);
        }
        outcome.map_err(store_error)?;
        written.ok_or_else(|| DomainError::Io("the write answered nothing".to_string()))
    }

    /// `workflow use`: activate a workflow.
    ///
    /// Refused while a sprint is open, and when a ticket sits in a stage the
    /// target lacks, naming the stages and their counts. A workflow missing the
    /// ready, active or review role activates on the board; the runner refuses
    /// it later.
    pub fn use_workflow(&self, actor: &Actor, handle: &str) -> DomainResult<Workflow> {
        let target = self.get(handle)?;
        let open_lanes = self.open_sprint_lanes()?;
        if !open_lanes.is_empty() {
            return Err(DomainError::WorkflowHasOpenSprint { lanes: open_lanes });
        }
        let target_keys: HashSet<&str> = target
            .stages
            .iter()
            .map(|stage| stage.key.as_str())
            .collect();
        let missing: Vec<(String, usize)> = self
            .ticket_stage_counts()?
            .into_iter()
            .filter(|(key, _)| !target_keys.contains(key.as_str()))
            .collect();
        if !missing.is_empty() {
            return Err(DomainError::WorkflowMissingStages { stages: missing });
        }
        let current = self.active()?;
        if current.id == target.id {
            return Ok(target);
        }
        self.store
            .write(actor, |write| {
                write.upsert_setting(NewSetting {
                    key: ACTIVE_WORKFLOW_KEY,
                    value: &target.id,
                })
            })
            .map_err(store_error)?;
        Ok(target)
    }

    /// `workflow delete`: refuse the default, the active workflow, or one with
    /// recorded runs (TAN-332). The target's row, the active setting and the
    /// run count are all read inside the delete's own transaction, so a switch
    /// or a run that lands first cannot slip past the guard.
    pub fn delete(&self, actor: &Actor, handle: &str) -> DomainResult<Vec<Workflow>> {
        let target = self.get(handle)?;
        let mut refusal: Option<DomainError> = None;
        let outcome = self.store.write(actor, |write| {
            if target.is_default() {
                refusal = Some(DomainError::WorkflowIsDefault(target.name.clone()));
                return Err(StoreError::Io("the domain refused the write".to_string()));
            }
            let active = write.setting(ACTIVE_WORKFLOW_KEY)?;
            let active = active
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if active == Some(target.id.as_str()) || active == Some(target.name.as_str()) {
                refusal = Some(DomainError::WorkflowIsActive(target.name.clone()));
                return Err(StoreError::Io("the domain refused the write".to_string()));
            }
            let count = write.workflow_run_count(&target.id)?;
            if count > 0 {
                refusal = Some(DomainError::WorkflowHasRuns {
                    name: target.name.clone(),
                    count,
                });
                return Err(StoreError::Io("the domain refused the write".to_string()));
            }
            write.delete_workflow(&target.id)
        });
        if let Some(refusal) = refusal {
            return Err(refusal);
        }
        outcome.map_err(store_error)?;
        self.list()
    }

    fn read_all(&self) -> DomainResult<Option<(Vec<Workflow>, i64)>> {
        let Some((rows, stages, revision)) = self.store.workflow_snapshot().map_err(store_error)?
        else {
            return Ok(None);
        };
        let workflows = rows
            .iter()
            .map(|row| {
                let own: Vec<&StageRow> = stages
                    .iter()
                    .filter(|stage| stage.workflow_id == row.id)
                    .collect();
                Workflow::from_rows(row.clone(), &own)
            })
            .collect::<DomainResult<Vec<_>>>()?;
        Ok(Some((workflows, revision)))
    }

    fn open_sprint_lanes(&self) -> DomainResult<Vec<String>> {
        let Some((rows, _)) = self.store.read_snapshot(sprint_rows).map_err(store_error)? else {
            return Ok(Vec::new());
        };
        Ok(rows
            .into_iter()
            .filter(|sprint| sprint.status == SprintStatus::Open.as_str())
            .map(|sprint| sprint.lane)
            .collect())
    }

    fn ticket_stage_counts(&self) -> DomainResult<BTreeMap<String, usize>> {
        let Some(rows) = self.store.board_rows().map_err(store_error)? else {
            return Ok(BTreeMap::new());
        };
        let mut counts = BTreeMap::new();
        for ticket in rows.tickets {
            *counts.entry(ticket.status).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

/// The statuses of every ticket in the store, counted by status, on a write's
/// own connection: the occupied-stage check the transaction commits over.
fn ticket_stage_counts(
    connection: &rusqlite::Connection,
) -> crate::store::Result<BTreeMap<String, usize>> {
    let mut statement = connection
        .prepare("SELECT status, COUNT(*) FROM tickets GROUP BY status")
        .map_err(|error| crate::store::translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })
        .map_err(|error| crate::store::translate(error, connection))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| crate::store::translate(error, connection))?;
    Ok(rows.into_iter().collect())
}

fn store_error(error: crate::store::StoreError) -> DomainError {
    match error {
        crate::store::StoreError::Locked { home } => DomainError::Locked(home),
        crate::store::StoreError::Io(message) => DomainError::Io(message),
        crate::store::StoreError::Legacy { version } => DomainError::Io(format!(
            "the store is schema v{version}, newer than this build"
        )),
    }
}
