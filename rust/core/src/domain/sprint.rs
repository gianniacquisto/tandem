//! The sprint record: one sprint's run, its lane, batch, run log and close.
//!
//! **A sprint is not a ticket.** It has its own `sprints` table (ADR 0031), and
//! a sprint write never touches the board's rows. **The pair (lane, number) is
//! the identity**: there is no UUID on this record, the number is per lane, and
//! the human-facing name is the number and the lane in one line. `open` and
//! `closed` are its two
//! states, and the board's `Status` says nothing about a sprint.
//!
//! **A record is a fact, not a log.** It is created once ([`Sprints::open`]),
//! appended to ([`Sprints::ticket`], [`Sprints::run`]) and stamped once
//! ([`Sprints::close`]); there is no edit and no delete. The Swift record is
//! `Sources/TandemCore/Sprints.swift`, and its rules are the port's: the number
//! is derived at open as one more than the highest the lane holds, a lane with an
//! open sprint does not get a second one, and the batch and the run log keep the
//! order they were appended in.

use serde::Serialize;
use ts_rs::TS;

use super::error::{DomainError, DomainResult};
use super::operation::Operation;
use crate::store::records::sprint_rows;
use crate::store::{
    Actor, NewSprint, Result as StoreResult, SprintRow, Store, StoreError, timestamp,
};

/// How a sprint's batch was built: the human's choice at open, stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SprintMode {
    Serial,
    Parallel,
}

impl SprintMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::Parallel => "parallel",
        }
    }

    /// Every wire value, in declaration order.
    pub const WIRE_VALUES: [&'static str; 2] = ["serial", "parallel"];

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "serial" => Some(Self::Serial),
            "parallel" => Some(Self::Parallel),
            _ => None,
        }
    }
}

/// The two states a sprint record has. `close` is stamped against this state,
/// and there is no third one to fall back on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SprintStatus {
    Open,
    Closed,
}

impl SprintStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Every wire value, in state order.
    pub const WIRE_VALUES: [&'static str; 2] = ["open", "closed"];

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

/// A sprint: a record of one sprint's run.
///
/// `batch` is the tickets the sprint carried, in stored order, and `runs` is one
/// dated entry per landing: the core's ISO-8601 UTC stamp, an em dash and the
/// caller's words, so a run log reads as a log.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub lane: String,
    pub number: i64,
    pub session: Option<String>,
    pub mode: SprintMode,
    pub batch: Vec<String>,
    pub runs: Vec<String>,
    pub close: Option<String>,
    pub status: SprintStatus,
    pub opened_at: String,
    pub closed_at: Option<String>,
}

impl Sprint {
    /// The sprint's human-facing identity: the number and the lane in one line,
    /// `Sprint 3` and the lane's name around an em dash. The pair (lane, number)
    /// is the whole handle.
    pub fn name(&self) -> String {
        format!("Sprint {} — {}", self.number, self.lane)
    }

    /// Append one run entry, dated by the core: the caller supplies the words.
    fn stamp_run(&mut self, note: &str, at: &str) {
        self.runs.push(format!("{at} — {note}"));
    }

    fn from_row(row: SprintRow) -> StoreResult<Self> {
        let mode = SprintMode::parse(&row.mode).ok_or_else(|| {
            StoreError::Io(format!("a sprint row carries an unknown mode {}", row.mode))
        })?;
        let status = SprintStatus::parse(&row.status).ok_or_else(|| {
            StoreError::Io(format!(
                "a sprint row carries an unknown status {}",
                row.status
            ))
        })?;
        let opened_at = readable("opened_at", &row.opened_at)?;
        let closed_at = match &row.closed_at {
            Some(raw) => Some(readable("closed_at", raw)?),
            None => None,
        };
        let batch = list_of::<String>(&row.batch, "batch")?;
        let runs = list_of::<String>(&row.runs, "run log")?;
        Ok(Self {
            lane: row.lane,
            number: row.number,
            session: row.session,
            mode,
            batch,
            runs,
            close: row.close,
            status,
            opened_at,
            closed_at,
        })
    }
}

/// A lane's sprints, grouped for display: the lane's name and its records in the
/// order they arrived (newest `opened_at` first, the core's own list order).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SprintLaneGroup {
    pub lane: String,
    pub sprints: Vec<Sprint>,
}

/// The sprints store, over the home's `tandem.db`.
#[derive(Debug, Clone)]
pub struct Sprints {
    store: Store,
}

impl Sprints {
    pub fn new(store: &Store) -> Self {
        Self {
            store: store.clone(),
        }
    }

    /// `sprint open`: record a sprint, once per lane.
    ///
    /// The lane is trimmed and an empty one is refused; a blank opening session
    /// is no session, because the core never reads the environment. **A lane
    /// that already has an open sprint does not get a second one**: the open
    /// record is returned unchanged with a note, so a re-run of the open step
    /// cannot fork the sprint. A lane whose sprints are all closed opens a new
    /// one, numbered one more than the highest number the lane holds, open and
    /// closed alike, so a closed sprint is never renumbered over.
    pub fn open(
        &self,
        actor: &Actor,
        lane: &str,
        mode: SprintMode,
        session: Option<&str>,
        at: Option<&str>,
    ) -> DomainResult<Operation<Sprint>> {
        let lane = lane_name(lane)?;
        let session = session
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let opened_at = stamp(at)?;
        let (value, note) = self.mutate(actor, |sprints| {
            if let Some(existing) = sprints
                .iter()
                .find(|sprint| sprint.lane == lane && sprint.status == SprintStatus::Open)
            {
                return Ok((
                    existing.clone(),
                    None,
                    Some(format!("{} is already open", existing.name())),
                ));
            }
            let number = sprints
                .iter()
                .filter(|sprint| sprint.lane == lane)
                .map(|sprint| sprint.number)
                .max()
                .unwrap_or(0)
                + 1;
            let sprint = Sprint {
                lane: lane.clone(),
                number,
                session: session.clone(),
                mode,
                batch: Vec::new(),
                runs: Vec::new(),
                close: None,
                status: SprintStatus::Open,
                opened_at: opened_at.clone(),
                closed_at: None,
            };
            sprints.push(sprint.clone());
            Ok((sprint.clone(), Some(sprint), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `sprint ticket`: add one ticket to a sprint's batch.
    ///
    /// Appended once, in stored order. An id the board does not carry is refused
    /// before the sprint is touched: the batch takes only ids the store holds,
    /// archive included, because the rule is about what the store carries and not
    /// what is open this minute. A ticket deleted *after* the write is not this
    /// failure: the batch keeps the id and a reader reads "not on the board".
    pub fn ticket(
        &self,
        actor: &Actor,
        lane: &str,
        number: i64,
        ticket: &str,
    ) -> DomainResult<Operation<Sprint>> {
        let lane = lane_name(lane)?;
        // The handle first: an unknown (lane, number) is that failure, whatever
        // the ticket id goes on to say.
        self.show(&lane, number)?;
        if !self.store.carries_ticket(ticket)? {
            return Err(DomainError::UnknownTicketId(ticket.to_string()));
        }
        let (value, note) = self.mutate(actor, |sprints| {
            let Some(index) = index_of(sprints, &lane, number) else {
                return Err(unknown_sprint(sprints, &lane, number));
            };
            if sprints[index].batch.iter().any(|carried| carried == ticket) {
                return Ok((sprints[index].clone(), None, None));
            }
            let record = &mut sprints[index];
            record.batch.push(ticket.to_string());
            Ok((record.clone(), Some(record.clone()), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `sprint run`: append one dated entry to the run log.
    ///
    /// The note is trimmed and an empty one is refused before anything is
    /// written: an empty pipe, not a landing without words. The date is the
    /// core's, stamped as the entry is appended.
    pub fn run(
        &self,
        actor: &Actor,
        lane: &str,
        number: i64,
        note: &str,
        at: Option<&str>,
    ) -> DomainResult<Operation<Sprint>> {
        let lane = lane_name(lane)?;
        let text = note.trim();
        if text.is_empty() {
            return Err(DomainError::EmptySprintNote);
        }
        let stamped = stamp(at)?;
        let (value, note) = self.mutate(actor, |sprints| {
            let Some(index) = index_of(sprints, &lane, number) else {
                return Err(unknown_sprint(sprints, &lane, number));
            };
            let record = &mut sprints[index];
            record.stamp_run(text, &stamped);
            Ok((record.clone(), Some(record.clone()), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `sprint close`: stamp a sprint as closed, once.
    ///
    /// The close text and the close time are written together; a text that is
    /// given but blank is refused, because a close that says nothing is spelled
    /// `None`, not `""`. A record that has already closed is returned unchanged
    /// with a note, so a second `close` cannot restate how a sprint ended.
    pub fn close(
        &self,
        actor: &Actor,
        lane: &str,
        number: i64,
        note: Option<&str>,
        at: Option<&str>,
    ) -> DomainResult<Operation<Sprint>> {
        let lane = lane_name(lane)?;
        let text = match note {
            Some(text) if text.trim().is_empty() => return Err(DomainError::EmptySprintNote),
            Some(text) => Some(text.trim().to_string()),
            None => None,
        };
        let closed_at = stamp(at)?;
        let (value, note) = self.mutate(actor, |sprints| {
            let Some(index) = index_of(sprints, &lane, number) else {
                return Err(unknown_sprint(sprints, &lane, number));
            };
            if sprints[index].status != SprintStatus::Open {
                return Ok((
                    sprints[index].clone(),
                    None,
                    Some(format!("{} is already closed", sprints[index].name())),
                ));
            }
            let record = &mut sprints[index];
            record.close = text.clone();
            record.closed_at = Some(closed_at.clone());
            record.status = SprintStatus::Closed;
            Ok((record.clone(), Some(record.clone()), None))
        })?;
        Ok(Operation { value, note })
    }

    /// `sprint list`: the records, newest `opened_at` first, with the caller's
    /// filters. Records that share a timestamp keep the order they were recorded
    /// in.
    pub fn list(
        &self,
        lane: Option<&str>,
        status: Option<SprintStatus>,
    ) -> DomainResult<Vec<Sprint>> {
        let lane = match lane {
            Some(name) => Some(lane_name(name)?),
            None => None,
        };
        let mut sprints = self.records()?;
        sprints.retain(|sprint| lane.as_ref().is_none_or(|name| &sprint.lane == name));
        sprints.retain(|sprint| status.is_none_or(|state| sprint.status == state));
        newest_first(&mut sprints);
        Ok(sprints)
    }

    /// `sprint list` and the revision its rows are a snapshot of, read in one
    /// transaction: the wire's listing pairs rows with a number no concurrent
    /// writer split, the board's `list_snapshot` shape for sprints.
    pub fn list_snapshot(
        &self,
        lane: Option<&str>,
        status: Option<SprintStatus>,
    ) -> DomainResult<(Vec<Sprint>, i64)> {
        let lane = match lane {
            Some(name) => Some(lane_name(name)?),
            None => None,
        };
        let Some((rows, revision)) = self.store.sprint_snapshot()? else {
            return Ok((Vec::new(), 0));
        };
        let mut sprints = rows
            .into_iter()
            .map(Sprint::from_row)
            .collect::<StoreResult<Vec<_>>>()
            .map_err(DomainError::from)?;
        sprints.retain(|sprint| lane.as_ref().is_none_or(|name| &sprint.lane == name));
        sprints.retain(|sprint| status.is_none_or(|state| sprint.status == state));
        newest_first(&mut sprints);
        Ok((sprints, revision))
    }

    /// `sprint show`: one record by the (lane, number) handle, or the failure
    /// carrying the numbers the lane does hold.
    pub fn show(&self, lane: &str, number: i64) -> DomainResult<Sprint> {
        let lane = lane_name(lane)?;
        let sprints = self.records()?;
        sprints
            .iter()
            .find(|sprint| sprint.lane == lane && sprint.number == number)
            .cloned()
            .ok_or_else(|| unknown_sprint(&sprints, &lane, number))
    }

    /// The lane-only handle: the lane's open record. A lane with none answers
    /// [`DomainError::NoOpenSprint`], whose next step is the lane's own `open`.
    pub fn open_in(&self, lane: &str) -> DomainResult<Sprint> {
        let lane = lane_name(lane)?;
        self.open_record(&lane)?
            .ok_or(DomainError::NoOpenSprint(lane))
    }

    /// [`open_in`](Self::open_in) and the revision the record is a snapshot
    /// of, read in one transaction: the wire's `sprint.show` answers the open
    /// sprint and its number together.
    pub fn open_in_snapshot(&self, lane: &str) -> DomainResult<(Sprint, i64)> {
        let lane = lane_name(lane)?;
        let Some((rows, revision)) = self.store.sprint_snapshot()? else {
            return Err(DomainError::NoOpenSprint(lane));
        };
        let record = rows
            .into_iter()
            .map(Sprint::from_row)
            .collect::<StoreResult<Vec<_>>>()
            .map_err(DomainError::from)?
            .into_iter()
            .find(|sprint| sprint.lane == lane && sprint.status == SprintStatus::Open)
            .ok_or(DomainError::NoOpenSprint(lane))?;
        Ok((record, revision))
    }

    /// The lane's open record, if the lane holds one. The one rule both
    /// lane-only handles read, so `sprint ticket` and `sprint close` cannot
    /// disagree about which record is open.
    fn open_record(&self, lane: &str) -> DomainResult<Option<Sprint>> {
        Ok(self
            .records()?
            .into_iter()
            .find(|sprint| sprint.lane == lane && sprint.status == SprintStatus::Open))
    }

    /// The lane-only handle `sprint close` acts on: the lane's open record, or
    /// the lane's newest when every record has closed. A close is a stamp once,
    /// so a re-run of the close step must reach the same record and read its
    /// note rather than open a second one. A lane with no record at all answers
    /// [`DomainError::NoOpenSprint`], as [`Sprints::open_in`] does.
    pub fn close_in(&self, lane: &str) -> DomainResult<Sprint> {
        let lane = lane_name(lane)?;
        if let Some(open) = self.open_record(&lane)? {
            return Ok(open);
        }
        self.list(Some(&lane), None)?
            .into_iter()
            .next()
            .ok_or(DomainError::NoOpenSprint(lane))
    }

    /// The records grouped by lane for display (spec `4F746EFE`, R3).
    ///
    /// The rule lives in the core because the core is the only test seam: the
    /// app's Sprints tab and any later reader must reach the same grouping rather
    /// than lookalikes. The lanes come out in first-appearance order, the newest
    /// sprint's lane first, and the sprints inside a group keep the order they
    /// arrive in, which is [`Sprints::list`]'s own. The grouping is a fold, not a
    /// sort, so it never reshuffles a read.
    pub fn grouped_by_lane(sprints: &[Sprint]) -> Vec<SprintLaneGroup> {
        let mut groups: Vec<(String, Vec<Sprint>)> = Vec::new();
        for sprint in sprints {
            match groups.iter_mut().find(|(lane, _)| lane == &sprint.lane) {
                Some((_, group)) => group.push(sprint.clone()),
                None => groups.push((sprint.lane.clone(), vec![sprint.clone()])),
            }
        }
        groups
            .into_iter()
            .map(|(lane, sprints)| SprintLaneGroup { lane, sprints })
            .collect()
    }

    fn records(&self) -> DomainResult<Vec<Sprint>> {
        self.store
            .sprints()?
            .into_iter()
            .map(Sprint::from_row)
            .collect::<StoreResult<Vec<_>>>()
            .map_err(DomainError::from)
    }

    /// A read-modify-write in one transaction, the sessions' own shape: the rows
    /// are read under the write lock, the body changes at most one of them, and
    /// a domain failure rolls the transaction back as itself.
    fn mutate<T>(
        &self,
        actor: &Actor,
        body: impl FnOnce(&mut Vec<Sprint>) -> DomainResult<(T, Option<Sprint>, Option<String>)>,
    ) -> DomainResult<(T, Option<String>)> {
        let mut failure: Option<DomainError> = None;
        let mut result: Option<(T, Option<String>)> = None;
        let outcome = self.store.write(actor, |write| {
            let mut sprints = sprint_rows(write.connection)?
                .into_iter()
                .map(Sprint::from_row)
                .collect::<StoreResult<Vec<_>>>()?;
            let (value, changed, note) = match body(&mut sprints) {
                Ok(changed) => changed,
                Err(error) => {
                    failure = Some(error);
                    return Err(StoreError::Io("the domain refused the write".to_string()));
                }
            };
            if let Some(record) = changed {
                let batch = serde_json::to_string(&record.batch).map_err(|error| {
                    StoreError::Io(format!("could not write the batch: {error}"))
                })?;
                let runs = serde_json::to_string(&record.runs).map_err(|error| {
                    StoreError::Io(format!("could not write the run log: {error}"))
                })?;
                let new = NewSprint {
                    lane: &record.lane,
                    number: record.number,
                    session: record.session.as_deref(),
                    mode: record.mode.as_str(),
                    batch: Some(&batch),
                    runs: Some(&runs),
                    close: record.close.as_deref(),
                    status: record.status.as_str(),
                    opened_at: &record.opened_at,
                    closed_at: record.closed_at.as_deref(),
                };
                if write.sprint_exists(&record.lane, record.number)? {
                    write.update_sprint(new)?;
                } else {
                    write.insert_sprint(new)?;
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

fn index_of(sprints: &[Sprint], lane: &str, number: i64) -> Option<usize> {
    sprints
        .iter()
        .position(|sprint| sprint.lane == lane && sprint.number == number)
}

fn unknown_sprint(sprints: &[Sprint], lane: &str, number: i64) -> DomainError {
    let mut known: Vec<i64> = sprints
        .iter()
        .filter(|sprint| sprint.lane == lane)
        .map(|sprint| sprint.number)
        .collect();
    known.sort_unstable();
    DomainError::UnknownSprint {
        lane: lane.to_string(),
        number,
        known,
    }
}

fn lane_name(raw: &str) -> DomainResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyLaneName);
    }
    Ok(trimmed.to_string())
}

fn list_of<T: serde::de::DeserializeOwned>(
    raw: &Option<String>,
    what: &str,
) -> StoreResult<Vec<T>> {
    match raw {
        Some(text) => serde_json::from_str(text).map_err(|error| {
            StoreError::Io(format!(
                "a sprint row carries an unreadable {what}: {error}"
            ))
        }),
        None => Ok(Vec::new()),
    }
}

fn readable(field: &str, text: &str) -> StoreResult<String> {
    if timestamp::decode(text).is_some() {
        Ok(text.to_string())
    } else {
        Err(StoreError::Io(format!(
            "a sprint row carries an unreadable {field} {text}"
        )))
    }
}

/// Newest `opened_at` first; records that share a timestamp keep the order they
/// were read in.
fn newest_first(sprints: &mut [Sprint]) {
    sprints.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
}
