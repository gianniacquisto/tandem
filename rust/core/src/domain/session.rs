//! The session record: one agent session's run, kept as an artifact of its own.
//!
//! **A session is not a ticket.** It has its own `sessions` table in
//! `<home>/tandem.db` (ADR 0031), and a session write never touches the board's
//! rows. **The vocabulary is the record's**: `running` and `ended` are its two
//! states, `succeeded`/`failed`/`interrupted`/`unknown` its outcomes, and the
//! board's `Status` says nothing about a session.
//!
//! **A record is a fact, not a log.** It is created once ([`Sessions::start`]),
//! linked to ([`Sessions::link`]) and stamped once ([`Sessions::end`]); there is
//! no edit and no delete. The Swift record is `Sources/TandemCore/Session.swift`,
//! and its rules are the port's: an id and a brief are trimmed and an empty one
//! is refused, a second `start` returns the record unchanged with a note, a
//! second `end` cannot restate the outcome or the usage, and the link order is
//! the order the links were made in.
//!
//! **The usage and the skills are stamped together, once.** The token counts,
//! the cost, the model ([TAN-208]) and the skill counts ([TAN-241]) are what the
//! harness reported; each piece is absent when it was not reported, never a zero
//! dressed up as a count.
//!
//! [TAN-208]: https://github.com/gianniacquisto/tandem
//! [TAN-241]: https://github.com/gianniacquisto/tandem

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::{DomainError, DomainResult};
use super::operation::Operation;
use crate::store::records::session_rows;
use crate::store::{
    Actor, NewSession, Result as StoreResult, SessionRow, Store, StoreError, timestamp,
};

/// The skill counts a pass loaded, by skill name: `thinking=2, plan=1`.
///
/// One count per name, held as a map so the order is the reader's, not the
/// store's. It is stamped once, by `end`, and stored as one JSON object in the
/// `skills` column (schema v3, an append-only migration).
pub type SkillCounts = BTreeMap<String, i64>;

/// Parse a `--skills name=count,...` value.
///
/// The shape is strict on purpose (TAN-241): a comma-separated list of
/// `name=count` pairs, the name trimmed and non-empty, the count a non-negative
/// whole number, and no name twice. **Every malformed form is a domain
/// failure** ([`DomainError::InvalidSkills`]) rather than a value quietly
/// dropped: a count an agent meant to report is a fact, and a silently lost one
/// is a wrong roll-up later. An empty value (or only whitespace) is no counts at
/// all, [`None`].
pub fn parse_skills(text: &str) -> DomainResult<Option<SkillCounts>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut counts = SkillCounts::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        let (name, count) = part
            .split_once('=')
            .ok_or_else(|| DomainError::InvalidSkills(text.to_string()))?;
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::InvalidSkills(text.to_string()));
        }
        let count: i64 = count
            .trim()
            .parse()
            .ok()
            .filter(|count| *count >= 0)
            .ok_or_else(|| DomainError::InvalidSkills(text.to_string()))?;
        if counts.insert(name.to_string(), count).is_some() {
            return Err(DomainError::InvalidSkills(text.to_string()));
        }
    }
    Ok(Some(counts))
}

/// The token counts one session spent, as the reporting harness holds them
/// (spec `F68948C0`): five integers, each the count its own flag names.
///
/// A member the harness did not report is `None`, absent on the wire, never a
/// zero dressed up as a count.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<i64>,
}

impl SessionTokens {
    /// Whether any piece was reported at all.
    pub fn reported(&self) -> bool {
        self.input.is_some()
            || self.output.is_some()
            || self.reasoning.is_some()
            || self.cache_read.is_some()
            || self.cache_write.is_some()
    }
}

/// The two states a session record has: `running` and `ended`.
///
/// `interrupted` is an outcome, not a state, and there is no third state to
/// fall back on: a row whose word this build does not know is a corrupt row,
/// never a guess at which half of the record was true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Running,
    Ended,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ended => "ended",
        }
    }

    /// The words a row or a filter may carry, in state order.
    pub const WIRE_VALUES: [&'static str; 2] = ["running", "ended"];

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "running" => Some(Self::Running),
            "ended" => Some(Self::Ended),
            _ => None,
        }
    }
}

/// How a session ended.
///
/// `unknown` is the honest value: a session that ended without saying how, and
/// also what a row carrying a word this build does not know reads as. Flattening
/// one record's outcome is recoverable where refusing the whole store is not
/// (the store's own rule for a value it cannot read).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionOutcome {
    Succeeded,
    Failed,
    Interrupted,
    /// The honest value for a session that ended without saying how, and the
    /// default an [`Ending`] starts from.
    #[default]
    Unknown,
}

impl SessionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Unknown => "unknown",
        }
    }

    /// Every wire value, in declaration order.
    pub const WIRE_VALUES: [&'static str; 4] = ["succeeded", "failed", "interrupted", "unknown"];

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// How Tandem reads a harness session's usage (PRT-29).
///
/// The record names its transport so `session end --usage-from-harness` reads
/// the same way the pass was recorded: `event-stream` is the embedded harness,
/// whose events Tandem holds; `database` is the external opencode on this Mac,
/// whose store Tandem reads read-only. Neither is a join on identity: the
/// session record is Tandem's, and the harness session is a foreign pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessTransport {
    EventStream,
    Database,
}

impl HarnessTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EventStream => "event-stream",
            Self::Database => "database",
        }
    }

    /// The words a row or a flag may carry, in declaration order.
    pub const WIRE_VALUES: [&'static str; 2] = ["event-stream", "database"];

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "event-stream" => Some(Self::EventStream),
            "database" => Some(Self::Database),
            _ => None,
        }
    }
}

/// The harness session a record points at, and the transport that reported it.
///
/// One value because the two travel together: a harness id with no transport
/// would leave `--usage-from-harness` with no way to read it. The id is the
/// harness's own and is trimmed; an empty one is refused at the write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSession {
    pub id: String,
    pub transport: HarnessTransport,
}

/// What `end` writes: the outcome, and the usage and skill counts the harness
/// reported. One value because the stamp is one write: a record's outcome and
/// what it spent are written together or not at all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ending<'a> {
    /// How the session ended.
    pub outcome: SessionOutcome,
    /// What it did, trimmed by `end`; empty is no summary.
    pub summary: Option<&'a str>,
    /// The token counts the harness reported, each piece absent when it did not.
    pub tokens: Option<SessionTokens>,
    /// What the session cost, the harness's own number.
    pub cost: Option<f64>,
    /// The model that spent the tokens.
    pub model: Option<&'a str>,
    /// The skill counts the pass loaded (TAN-241).
    pub skills: Option<SkillCounts>,
    /// The caller's own end time, whole seconds like the store's stamps;
    /// absent, it is now.
    pub at: Option<&'a str>,
}

impl<'a> Ending<'a> {
    /// A stamp of `outcome` and nothing else: no summary, no usage, now.
    pub fn new(outcome: SessionOutcome) -> Self {
        Self {
            outcome,
            ..Self::default()
        }
    }
}

/// What `end` answers. `stamped` is the hook's gate: the usage comment the CLI
/// posts on the linked tickets runs only when this call stamped, so a second
/// `end` (which alters nothing) cannot double-post.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    pub session: Session,
    pub note: Option<String>,
    pub stamped: bool,
}

/// A session: a record of one agent session's run.
///
/// Timestamps are ISO-8601 UTC text, whole seconds, which is the store's own
/// form and sorts chronologically as text. `tickets` is the tickets the session
/// carried, in link order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// The harness's own session, as a foreign pointer (PRT-29). Tandem's `id`
    /// is the primary key; this column is how a pass is joined to what the
    /// harness recorded, and it is absent on a hand-driven run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness_session_id: Option<String>,
    /// How the harness session's usage is read: `event-stream` or `database`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness_transport: Option<HarnessTransport>,
    pub project: String,
    pub agent: Option<String>,
    pub brief: String,
    pub summary: Option<String>,
    pub status: SessionStatus,
    pub outcome: Option<SessionOutcome>,
    pub tickets: Vec<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<SessionTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<SkillCounts>,
}

impl Session {
    /// Whether the record carries any usage: the triple `end` stamps together.
    /// The model alone is usage too, so a reported model with no cost still
    /// counts as reported (TAN-208).
    pub fn carries_usage(&self) -> bool {
        if self.cost.is_some() || self.model.is_some() {
            return true;
        }
        self.tokens.as_ref().is_some_and(SessionTokens::reported)
    }

    /// The record as the store holds it. A row the store cannot read is the
    /// store's own I/O failure: a timestamp, a status word or a JSON column this
    /// build cannot parse is a corrupt row, never a silent default.
    fn from_row(row: SessionRow) -> StoreResult<Self> {
        let status = SessionStatus::parse(&row.status).ok_or_else(|| {
            StoreError::Io(format!(
                "a session row carries an unknown status {}",
                row.status
            ))
        })?;
        let started_at = readable("started_at", &row.started_at)?;
        let ended_at = match &row.ended_at {
            Some(raw) => Some(readable("ended_at", raw)?),
            None => None,
        };
        let tickets: Vec<String> = match &row.tickets {
            Some(raw) => serde_json::from_str(raw).map_err(|error| {
                StoreError::Io(format!(
                    "a session row carries an unreadable ticket list: {error}"
                ))
            })?,
            None => Vec::new(),
        };
        let skills: Option<SkillCounts> = match &row.skills {
            Some(raw) => Some(serde_json::from_str(raw).map_err(|error| {
                StoreError::Io(format!(
                    "a session row carries an unreadable skill object: {error}"
                ))
            })?),
            None => None,
        };
        let tokens = SessionTokens {
            input: row.tokens_input,
            output: row.tokens_output,
            reasoning: row.tokens_reasoning,
            cache_read: row.tokens_cache_read,
            cache_write: row.tokens_cache_write,
        };
        Ok(Self {
            id: row.id,
            // A transport word this build does not know is flattened to absence,
            // like an outcome: the record still reads, and the pointer it cannot
            // interpret is the piece missing, never the whole row.
            harness_transport: row
                .harness_transport
                .as_deref()
                .and_then(HarnessTransport::parse),
            harness_session_id: row.harness_session_id,
            project: row.project,
            agent: row.agent,
            brief: row.brief,
            summary: row.summary,
            status,
            outcome: row
                .outcome
                .as_deref()
                .map(|word| SessionOutcome::parse(word).unwrap_or(SessionOutcome::Unknown)),
            tickets,
            started_at,
            ended_at,
            tokens: tokens.reported().then_some(tokens),
            cost: row.cost,
            model: row.model,
            skills,
        })
    }

    /// The record as an insert or an update writes it. `seconds` is derived
    /// here, as the Swift store derives it: the span between the start and the
    /// end, absent while the record runs.
    fn to_row(&self) -> StoreResult<(String, Option<String>)> {
        let tickets = serde_json::to_string(&self.tickets)
            .map_err(|error| StoreError::Io(format!("could not write the ticket list: {error}")))?;
        let skills = match &self.skills {
            Some(counts) => Some(serde_json::to_string(counts).map_err(|error| {
                StoreError::Io(format!("could not write the skill counts: {error}"))
            })?),
            None => None,
        };
        Ok((tickets, skills))
    }
}

/// A write's result: the value, and whether it changed the store. `true` is
/// what gates the backup and the note.
type Changed<T> = (T, Option<Session>, Option<String>);

/// The sessions store, over the home's `tandem.db`.
#[derive(Debug, Clone)]
pub struct Sessions {
    store: Store,
}

impl Sessions {
    pub fn new(store: &Store) -> Self {
        Self {
            store: store.clone(),
        }
    }

    /// `session start`: record a session, once.
    ///
    /// The id and the brief are trimmed, and an empty one is refused before
    /// anything is written; a blank agent is no agent. A record with the same
    /// id is returned unchanged, whatever its status, with a note: a session
    /// that has ended is not restarted, and one that runs is not touched.
    ///
    /// `at` is the caller's own timestamp for the start, whole seconds like the
    /// store's stamps; absent, it is now.
    pub fn start(
        &self,
        actor: &Actor,
        id: &str,
        project: &str,
        agent: Option<&str>,
        brief: &str,
        at: Option<&str>,
    ) -> DomainResult<Operation<Session>> {
        self.open(actor, id, project, agent, brief, None, at)
    }

    /// `session start`, pointing at the harness session the pass ran in (PRT-29).
    ///
    /// The handle stays Tandem's: a session, a re-dispatched pass and a second
    /// pass in one harness session each name their own record, and the harness
    /// id is one column on each. Nothing continues a stamped record; a pass that
    /// runs again starts again, with a fresh handle and the same pointer.
    // The verb's own fields, one argument each: a struct here would be a second
    // spelling of the same call for every caller.
    #[allow(clippy::too_many_arguments)]
    pub fn start_with_harness(
        &self,
        actor: &Actor,
        id: &str,
        project: &str,
        agent: Option<&str>,
        brief: &str,
        harness: HarnessSession,
        at: Option<&str>,
    ) -> DomainResult<Operation<Session>> {
        self.open(actor, id, project, agent, brief, Some(harness), at)
    }

    #[allow(clippy::too_many_arguments)]
    fn open(
        &self,
        actor: &Actor,
        id: &str,
        project: &str,
        agent: Option<&str>,
        brief: &str,
        harness: Option<HarnessSession>,
        at: Option<&str>,
    ) -> DomainResult<Operation<Session>> {
        let handle = self.handle(id)?;
        let brief = brief.trim();
        if brief.is_empty() {
            return Err(DomainError::EmptySessionBrief);
        }
        let harness = harness
            .map(|harness| {
                let id = harness.id.trim();
                if id.is_empty() {
                    return Err(DomainError::EmptyHarnessSessionId);
                }
                Ok(HarnessSession {
                    id: id.to_string(),
                    transport: harness.transport,
                })
            })
            .transpose()?;
        let name = agent
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let started_at = stamp(at)?;
        let (value, note) = self.mutate(actor, |sessions| {
            if let Some(existing) = sessions.iter().find(|session| session.id == handle) {
                return Ok((
                    existing.clone(),
                    None,
                    Some(format!("session {handle} already recorded")),
                ));
            }
            let session = Session {
                id: handle.clone(),
                harness_session_id: harness.as_ref().map(|harness| harness.id.clone()),
                harness_transport: harness.as_ref().map(|harness| harness.transport),
                project: project.to_string(),
                agent: name.clone(),
                brief: brief.to_string(),
                summary: None,
                status: SessionStatus::Running,
                outcome: None,
                tickets: Vec::new(),
                started_at: started_at.clone(),
                ended_at: None,
                tokens: None,
                cost: None,
                model: None,
                skills: None,
            };
            sessions.push(session.clone());
            Ok((session.clone(), Some(session), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `session end`: stamp a session as ended, once.
    ///
    /// The outcome, the end time, the summary and the usage and skills the
    /// harness reported are written together. The summary is trimmed, and an
    /// empty one is no summary. A record that has already ended is returned
    /// unchanged with a note, so a second `end` cannot restate how a session
    /// ended nor what it spent: the first stamp is the fact.
    pub fn end(&self, actor: &Actor, id: &str, ending: Ending<'_>) -> DomainResult<Stamp> {
        let Ending {
            outcome,
            summary,
            tokens,
            cost,
            model,
            skills,
            at,
        } = ending;
        let handle = self.handle(id)?;
        let said = summary
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        let model = model
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let ended_at = stamp(at)?;
        let ((value, stamped), note) = self.mutate(actor, |sessions| {
            let Some(index) = sessions.iter().position(|session| session.id == handle) else {
                return Err(DomainError::UnknownSession {
                    id: handle.clone(),
                    known: sessions.iter().map(|session| session.id.clone()).collect(),
                });
            };
            if sessions[index].status != SessionStatus::Running {
                return Ok((
                    (sessions[index].clone(), false),
                    None,
                    Some(format!("session {handle} already ended")),
                ));
            }
            let record = &mut sessions[index];
            record.outcome = Some(outcome);
            record.ended_at = Some(ended_at.clone());
            record.summary = said.clone();
            record.tokens = tokens.clone();
            record.cost = cost;
            record.model = model.clone();
            record.skills = skills.clone();
            record.status = SessionStatus::Ended;
            Ok(((record.clone(), true), Some(record.clone()), None))
        })?;
        Ok(Stamp {
            session: value,
            stamped,
            note,
        })
    }

    /// `session link`: record that a session carried a ticket. Appended once,
    /// in link order; a ticket already linked is not linked twice.
    pub fn link(&self, actor: &Actor, id: &str, ticket: &str) -> DomainResult<Operation<Session>> {
        let handle = self.handle(id)?;
        let (value, note) = self.mutate(actor, |sessions| {
            let Some(index) = sessions.iter().position(|session| session.id == handle) else {
                return Err(DomainError::UnknownSession {
                    id: handle.clone(),
                    known: sessions.iter().map(|session| session.id.clone()).collect(),
                });
            };
            if sessions[index]
                .tickets
                .iter()
                .any(|carried| carried == ticket)
            {
                return Ok((sessions[index].clone(), None, None));
            }
            let record = &mut sessions[index];
            record.tickets.push(ticket.to_string());
            Ok((record.clone(), Some(record.clone()), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `session list`: the records, newest `started_at` first, with the
    /// caller's filters. Records that share a timestamp keep the order they were
    /// recorded in, so a list does not reshuffle between two reads.
    pub fn list(
        &self,
        project: Option<&str>,
        status: Option<SessionStatus>,
    ) -> DomainResult<Vec<Session>> {
        let rows = self.store.sessions()?;
        let sessions = rows
            .into_iter()
            .map(Session::from_row)
            .collect::<StoreResult<Vec<_>>>()?;
        let mut filtered: Vec<Session> = sessions
            .into_iter()
            .filter(|session| project.is_none_or(|name| session.project == name))
            .filter(|session| status.is_none_or(|state| session.status == state))
            .collect();
        newest_first(&mut filtered);
        Ok(filtered)
    }

    /// `session show`: one record, or the failure naming the ids the store does
    /// hold so the caller recovers in one turn.
    pub fn show(&self, id: &str) -> DomainResult<Session> {
        let handle = self.handle(id)?;
        let sessions = self.records()?;
        sessions
            .into_iter()
            .find(|session| session.id == handle)
            .ok_or_else(|| DomainError::UnknownSession {
                id: handle,
                known: self.known_ids(),
            })
    }

    /// Resolve a handle to a record's own id.
    ///
    /// An exact record id answers itself, so every caller that names Tandem's
    /// handle keeps working. A handle the store does not carry is tried as a
    /// **harness pointer**: the newest record that names it in
    /// `harness_session_id` answers. That is how the external opencode wrapper,
    /// which only ever knew the harness id, still reaches its record through the
    /// same column (PRT-29); the record's identity stays Tandem's.
    pub fn resolve(&self, handle: &str) -> DomainResult<String> {
        let handle = self.handle(handle)?;
        let records = self.records()?;
        if records.iter().any(|session| session.id == handle) {
            return Ok(handle);
        }
        // Newest by start time, and the latest stored record among records that
        // share one: two passes can start in the same second, and the pointer
        // belongs to the pass that opened its record last.
        let mut newest: Option<Session> = None;
        for session in records
            .into_iter()
            .filter(|session| session.harness_session_id.as_deref() == Some(handle.as_str()))
        {
            if newest
                .as_ref()
                .is_none_or(|best| session.started_at >= best.started_at)
            {
                newest = Some(session);
            }
        }
        newest
            .map(|session| session.id)
            .ok_or_else(|| DomainError::UnknownSession {
                id: handle,
                known: self.known_ids(),
            })
    }

    /// Every record, in stored order. A read: the store is not touched.
    fn records(&self) -> DomainResult<Vec<Session>> {
        self.store
            .sessions()?
            .into_iter()
            .map(Session::from_row)
            .collect::<StoreResult<Vec<_>>>()
            .map_err(DomainError::from)
    }

    fn known_ids(&self) -> Vec<String> {
        self.records()
            .map(|sessions| sessions.into_iter().map(|session| session.id).collect())
            .unwrap_or_default()
    }

    /// The session's handle, trimmed; an empty result is a domain failure,
    /// because a record with no handle is a record nothing can find.
    fn handle(&self, id: &str) -> DomainResult<String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptySessionId);
        }
        Ok(trimmed.to_string())
    }

    /// A read-modify-write in one transaction: the rows are read under the write
    /// lock, the body changes at most one of them, and the change and the lock
    /// commit together. A domain failure rolls the transaction back and comes
    /// out as itself, never as the store's I/O words.
    fn mutate<T>(
        &self,
        actor: &Actor,
        body: impl FnOnce(&mut Vec<Session>) -> DomainResult<Changed<T>>,
    ) -> DomainResult<(T, Option<String>)> {
        let mut failure: Option<DomainError> = None;
        let mut result: Option<(T, Option<String>)> = None;
        let outcome = self.store.write(actor, |write| {
            let mut sessions = session_rows(write.connection)?
                .into_iter()
                .map(Session::from_row)
                .collect::<StoreResult<Vec<_>>>()?;
            let (value, changed, note) = match body(&mut sessions) {
                Ok(changed) => changed,
                Err(error) => {
                    failure = Some(error);
                    return Err(StoreError::Io("the domain refused the write".to_string()));
                }
            };
            if let Some(record) = changed {
                let (tickets, skills) = record.to_row()?;
                let new = NewSession {
                    id: &record.id,
                    harness_session_id: record.harness_session_id.as_deref(),
                    harness_transport: record.harness_transport.map(|transport| transport.as_str()),
                    project: &record.project,
                    agent: record.agent.as_deref(),
                    brief: &record.brief,
                    status: record.status.as_str(),
                    outcome: record.outcome.map(|outcome| outcome.as_str()),
                    summary: record.summary.as_deref(),
                    started_at: &record.started_at,
                    ended_at: record.ended_at.as_deref(),
                    tickets: Some(&tickets),
                    model: record.model.as_deref(),
                    tokens_input: record.tokens.as_ref().and_then(|tokens| tokens.input),
                    tokens_output: record.tokens.as_ref().and_then(|tokens| tokens.output),
                    tokens_reasoning: record.tokens.as_ref().and_then(|tokens| tokens.reasoning),
                    tokens_cache_read: record.tokens.as_ref().and_then(|tokens| tokens.cache_read),
                    tokens_cache_write: record
                        .tokens
                        .as_ref()
                        .and_then(|tokens| tokens.cache_write),
                    seconds: seconds(&record),
                    cost: record.cost,
                    skills: skills.as_deref(),
                };
                if write.session_exists(&record.id)? {
                    write.update_session(new)?;
                } else {
                    write.insert_session(new)?;
                }
            }
            result = Some((value, note));
            Ok(())
        });
        if let Some(failure) = failure {
            return Err(failure);
        }
        outcome?;
        result.ok_or_else(|| DomainError::Io("the write answered nothing".to_string()))
    }
}

/// The caller's stamp for a write, or now.
///
/// **Decoded at the write, refused when it is not a stamp.** The store's own
/// form is ISO-8601 UTC, whole seconds, and the read side rejects anything else,
/// so an undecoded value written here would poison every later read of the
/// record: a bad `at` is a domain failure now, at the one call that can still
/// say which value was wrong.
fn stamp(at: Option<&str>) -> DomainResult<String> {
    match at {
        Some(text) => match timestamp::decode(text) {
            Some(_) => Ok(text.to_string()),
            None => Err(DomainError::InvalidStamp(text.to_string())),
        },
        None => Ok(timestamp::now()),
    }
}

/// The span the store keeps beside the stamps, absent while the record runs.
fn seconds(session: &Session) -> Option<f64> {
    let start = timestamp::decode(&session.started_at)?;
    let end = timestamp::decode(session.ended_at.as_deref()?)?;
    Some((end - start) as f64)
}

fn readable(field: &str, text: &str) -> StoreResult<String> {
    if timestamp::decode(text).is_some() {
        Ok(text.to_string())
    } else {
        Err(StoreError::Io(format!(
            "a session row carries an unreadable {field} {text}"
        )))
    }
}

/// Newest `started_at` first; records that share a timestamp keep the order they
/// were read in.
fn newest_first(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
}
