//! The write side: one transaction, one actor, one chance to roll the backup.
//!
//! A [`Write`] is the only way a row enters the store, so the actor cannot be
//! forgotten at a call site: the write takes it once and every insert records
//! it. The guard also owns the rolling backup's timing: the pre-write state is
//! copied **before the first mutation of the transaction**, while the database
//! still holds the last committed state (ADR 0018), and a write that changes
//! nothing rolls no backup at all.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::rows::{
    NewComment, NewDependency, NewLane, NewProject, NewSetting, NewTicket, NewWorkflow,
    NewWorkflowStage,
};
use super::{Actor, Result};

/// A write in progress. Rows enter the store through its inserts only, so no
/// caller writes SQL and no caller chooses the author.
pub struct Write<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) actor: &'a Actor,
    pub(crate) backup: BackupState<'a>,
}

/// Whether this write still owes the rolling backup, and whether it has been
/// taken. Armed only when the database existed and did not migrate: the first
/// write has no earlier state, and a migrating write already rolled its
/// pre-image before the migration ran.
pub(crate) struct BackupState<'a> {
    pub(crate) database: &'a Path,
    pub(crate) target: PathBuf,
    pub(crate) armed: bool,
    pub(crate) taken: Cell<bool>,
}

impl Write<'_> {
    /// The actor every row of this write records.
    pub fn actor(&self) -> &Actor {
        self.actor
    }

    pub fn insert_lane(&self, lane: NewLane<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO lanes (name, key, last_number, position, project, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                lane.name,
                lane.key,
                lane.last_number,
                lane.position,
                lane.project,
                self.actor.as_str()
            ],
        )
    }

    pub fn insert_project(&self, project: NewProject<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO projects (name, position, author) VALUES (?1, ?2, ?3)",
            params![project.name, project.position, self.actor.as_str()],
        )
    }

    pub fn insert_ticket(&self, ticket: NewTicket<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO tickets
                (id, title, description, status, archived, lane, number, origin,
                 assignee, link, position, created_at, completed_at, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                ticket.id,
                ticket.title,
                ticket.description,
                ticket.status,
                i64::from(ticket.archived),
                ticket.lane,
                ticket.number,
                ticket.origin,
                ticket.assignee,
                ticket.link,
                ticket.position,
                ticket.created_at,
                ticket.completed_at,
                self.actor.as_str(),
            ],
        )
    }

    pub fn insert_dependency(&self, dependency: NewDependency<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO ticket_dependencies (ticket_id, depends_on_id, author)
             VALUES (?1, ?2, ?3)",
            params![
                dependency.ticket_id,
                dependency.depends_on_id,
                self.actor.as_str()
            ],
        )
    }

    /// Insert a comment whose author is the comment's own, not the write's actor.
    /// The two are different facts: the actor made the write, the author said the
    /// words, and only the CLI's `--author` names the second (the minutes rule).
    pub(crate) fn insert_comment_authored(
        &self,
        author: &str,
        comment: NewComment<'_>,
    ) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO comments (ticket_id, author, at, body) VALUES (?1, ?2, ?3, ?4)",
            params![comment.ticket_id, author, comment.at, comment.body],
        )
    }

    pub fn insert_comment(&self, comment: NewComment<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO comments (ticket_id, author, at, body) VALUES (?1, ?2, ?3, ?4)",
            params![
                comment.ticket_id,
                self.actor.as_str(),
                comment.at,
                comment.body
            ],
        )
    }

    pub fn insert_setting(&self, setting: NewSetting<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO settings (key, value, author) VALUES (?1, ?2, ?3)",
            params![setting.key, setting.value, self.actor.as_str()],
        )
    }

    /// Store a setting whether or not the key was stored before: the roster and
    /// the focus are whole-value replaces, so the second write overwrites the
    /// first in one statement. A first write inserts; a later one updates the
    /// value and the author, and a write that changes nothing still reads its
    /// receipt after.
    pub fn upsert_setting(&self, setting: NewSetting<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO settings (key, value, author) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, author = excluded.author",
            params![setting.key, setting.value, self.actor.as_str()],
        )
    }

    /// Drop a setting's row: clearing the focus removes the key, so an absent
    /// row and a cleared one read the same way. A missing row is a no-op.
    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.before_mutation()?;
        self.execute("DELETE FROM settings WHERE key = ?1", params![key])
    }

    pub fn insert_workflow(&self, workflow: NewWorkflow<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO workflows (id, name, scope, created_at, updated_at, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                workflow.id,
                workflow.name,
                workflow.scope,
                workflow.created_at,
                workflow.updated_at,
                self.actor.as_str()
            ],
        )
    }

    /// Update a workflow's editable half: its name and its stamp. The id, the
    /// scope and the creation stamp never move.
    pub fn update_workflow(&self, workflow: NewWorkflow<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "UPDATE workflows SET name = ?2, updated_at = ?3, author = ?4 WHERE id = ?1",
            params![
                workflow.id,
                workflow.name,
                workflow.updated_at,
                self.actor.as_str()
            ],
        )
    }

    /// Replace every stage of a workflow with the document's ordered list.
    /// The write is one transaction, so a refused document leaves the old
    /// stages in place; an unchanged document still rewrites, and the revision
    /// moves.
    pub fn replace_workflow_stages(
        &self,
        workflow_id: &str,
        stages: &[NewWorkflowStage<'_>],
    ) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "DELETE FROM workflow_stages WHERE workflow_id = ?1",
            params![workflow_id],
        )?;
        for stage in stages {
            self.execute(
                "INSERT INTO workflow_stages
                    (workflow_id, position, key, label, role, binding_role, binding_pass)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    stage.workflow_id,
                    stage.position,
                    stage.key,
                    stage.label,
                    stage.role,
                    stage.binding_role,
                    stage.binding_pass
                ],
            )?;
        }
        Ok(())
    }

    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "DELETE FROM workflow_stages WHERE workflow_id = ?1",
            params![id],
        )?;
        self.execute("DELETE FROM workflows WHERE id = ?1", params![id])
    }

    /// One setting's value inside this transaction: the row as it stands now,
    /// not as it read before the write began.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        super::setting(self.connection, key)
    }

    /// The pre-write state, copied once, before this transaction modifies a
    /// page. The copy is taken through its own read-only connection: a WAL
    /// reader sees the last committed state while this write waits, and the
    /// backup API does not run against a connection holding the write lock.
    pub(crate) fn before_mutation(&self) -> Result<()> {
        if !self.backup.armed || self.backup.taken.get() {
            return Ok(());
        }
        super::backup_database(self.backup.database, &self.backup.target)?;
        self.backup.taken.set(true);
        Ok(())
    }

    pub(crate) fn execute(&self, sql: &str, values: impl rusqlite::Params) -> Result<()> {
        self.connection
            .execute(sql, values)
            .map_err(|error| super::translate(error, self.connection))?;
        Ok(())
    }
}
