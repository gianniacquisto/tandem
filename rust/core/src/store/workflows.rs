//! The workflow rows: the workflows and their ordered stages.
//!
//! The domain above the store parses a workflow's role words and validates the
//! document; this module is the adapter's half, and the only place the two
//! tables' SQL lives. A workflow is read whole, stages in position order, because
//! a stage list is the document.

use rusqlite::Connection;

use super::{Result, Store, translate};

/// One workflow row, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowRow {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: String,
}

/// One stage row, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StageRow {
    pub workflow_id: String,
    pub position: i64,
    pub key: String,
    pub label: String,
    pub role: String,
    pub binding_role: Option<String>,
    pub binding_pass: Option<String>,
}

/// The two row lists and the revision they are a snapshot of.
pub(crate) type WorkflowSnapshot = (Vec<WorkflowRow>, Vec<StageRow>, i64);

/// Every workflow and every stage, read in one transaction: a listing's
/// documents and its revision cannot be split by a concurrent writer.
pub(crate) fn workflow_rows(connection: &Connection) -> Result<(Vec<WorkflowRow>, Vec<StageRow>)> {
    let workflows = connection
        .prepare(
            "SELECT id, name, scope, created_at, updated_at, author FROM workflows
             ORDER BY rowid",
        )
        .map_err(|error| translate(error, connection))?
        .query_map([], |row| {
            Ok(WorkflowRow {
                id: row.get(0)?,
                name: row.get(1)?,
                scope: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                author: row.get(5)?,
            })
        })
        .map_err(|error| translate(error, connection))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| translate(error, connection))?;

    let stages = connection
        .prepare(
            "SELECT workflow_id, position, key, label, role, binding_role, binding_pass
             FROM workflow_stages ORDER BY workflow_id, position",
        )
        .map_err(|error| translate(error, connection))?
        .query_map([], |row| {
            Ok(StageRow {
                workflow_id: row.get(0)?,
                position: row.get(1)?,
                key: row.get(2)?,
                label: row.get(3)?,
                role: row.get(4)?,
                binding_role: row.get(5)?,
                binding_pass: row.get(6)?,
            })
        })
        .map_err(|error| translate(error, connection))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| translate(error, connection))?;

    Ok((workflows, stages))
}

impl Store {
    /// The workflows and the revision they are a snapshot of, read in one
    /// transaction.
    pub(crate) fn workflow_snapshot(&self) -> Result<Option<WorkflowSnapshot>> {
        self.read_snapshot(|connection| {
            let (workflows, stages) = workflow_rows(connection)?;
            Ok((workflows, stages))
        })
        .map(|value| value.map(|((workflows, stages), revision)| (workflows, stages, revision)))
    }
}
