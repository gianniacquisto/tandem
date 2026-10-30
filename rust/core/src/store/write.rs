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
    NewComment, NewDependency, NewLane, NewSession, NewSetting, NewSprint, NewTicket,
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
            "INSERT INTO lanes (name, key, last_number, position, author)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                lane.name,
                lane.key,
                lane.last_number,
                lane.position,
                self.actor.as_str()
            ],
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

    pub fn insert_session(&self, session: NewSession<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO sessions
                (id, project, agent, brief, status, outcome, summary, started_at,
                 ended_at, tokens_input, tokens_output, tokens_reasoning,
                 tokens_cache_read, tokens_cache_write, seconds, cost, model,
                 tickets, skills, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                     ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                session.id,
                session.project,
                session.agent,
                session.brief,
                session.status,
                session.outcome,
                session.summary,
                session.started_at,
                session.ended_at,
                session.tokens_input,
                session.tokens_output,
                session.tokens_reasoning,
                session.tokens_cache_read,
                session.tokens_cache_write,
                session.seconds,
                session.cost,
                session.model,
                session.tickets,
                session.skills,
                self.actor.as_str(),
            ],
        )
    }

    /// Whether the sessions table carries an id. The records domain is a
    /// read-modify-write in one transaction, so it asks inside the write rather
    /// than trusting an earlier read.
    pub fn session_exists(&self, id: &str) -> Result<bool> {
        self.connection
            .query_row("SELECT COUNT(*) FROM sessions WHERE id = ?1", [id], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count > 0)
            .map_err(super::translate)
    }

    /// Replace a session row's mutable half: a record is created once, linked
    /// to and stamped once, so every write after `start` is this one. The
    /// author moves with the row: the last write's actor is the row's author.
    pub fn update_session(&self, session: NewSession<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "UPDATE sessions SET
                project = ?2, agent = ?3, brief = ?4, status = ?5, outcome = ?6,
                summary = ?7, started_at = ?8, ended_at = ?9, tokens_input = ?10,
                tokens_output = ?11, tokens_reasoning = ?12, tokens_cache_read = ?13,
                tokens_cache_write = ?14, seconds = ?15, cost = ?16, model = ?17,
                tickets = ?18, skills = ?19, author = ?20
             WHERE id = ?1",
            params![
                session.id,
                session.project,
                session.agent,
                session.brief,
                session.status,
                session.outcome,
                session.summary,
                session.started_at,
                session.ended_at,
                session.tokens_input,
                session.tokens_output,
                session.tokens_reasoning,
                session.tokens_cache_read,
                session.tokens_cache_write,
                session.seconds,
                session.cost,
                session.model,
                session.tickets,
                session.skills,
                self.actor.as_str(),
            ],
        )
    }

    pub fn insert_sprint(&self, sprint: NewSprint<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "INSERT INTO sprints
                (lane, number, session, mode, batch, runs, close, status,
                 opened_at, closed_at, author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                sprint.lane,
                sprint.number,
                sprint.session,
                sprint.mode,
                sprint.batch,
                sprint.runs,
                sprint.close,
                sprint.status,
                sprint.opened_at,
                sprint.closed_at,
                self.actor.as_str(),
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

    /// Whether the sprints table carries a (lane, number). The sprints domain is
    /// a read-modify-write in one transaction, so it asks inside the write.
    pub fn sprint_exists(&self, lane: &str, number: i64) -> Result<bool> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM sprints WHERE lane = ?1 AND number = ?2",
                params![lane, number],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .map_err(super::translate)
    }

    /// Replace a sprint row: `open` creates it, and `ticket`, `run` and
    /// `close` each write the whole record back, because a sprint's batch and
    /// run log are ordered lists that a column update cannot append to
    /// truthfully.
    pub fn update_sprint(&self, sprint: NewSprint<'_>) -> Result<()> {
        self.before_mutation()?;
        self.execute(
            "UPDATE sprints SET
                session = ?3, mode = ?4, batch = ?5, runs = ?6, close = ?7,
                status = ?8, opened_at = ?9, closed_at = ?10, author = ?11
             WHERE lane = ?1 AND number = ?2",
            params![
                sprint.lane,
                sprint.number,
                sprint.session,
                sprint.mode,
                sprint.batch,
                sprint.runs,
                sprint.close,
                sprint.status,
                sprint.opened_at,
                sprint.closed_at,
                self.actor.as_str(),
            ],
        )
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
            .map_err(super::translate)?;
        Ok(())
    }
}
