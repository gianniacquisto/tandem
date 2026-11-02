//! The sprint runner: the deterministic transition function over the sprint record.
//!
//! The loop in `skills/tandem-sprint/SKILL.md` and `skills/tandem-build/SKILL.md`
//! is prose an agent reads: the draw, the per-pass dispatches, the record writes.
//! This module owns the decisions and leaves the effects to the caller. The core
//! holds the state machine because it is headless and testable; main owns spawn,
//! the harness lifetime and every effect the machine emits (ADR 0037: transport is
//! a seam, the machine never touches it).
//!
//! Three rules hold everywhere below:
//!
//! - **The transitions are total.** [`step`] maps every (driver state, pass
//!   result) pair to exactly one (next state, effect request) or to a named
//!   refusal. Unknown results are refused ([`RunnerError::UnknownResult`]), never
//!   defaulted; a result for another ticket or another pass is refused, never
//!   guessed at.
//! - **The machine reads the sprint record (ADR 0032), never a copy.**
//!   [`Driver::batch`] is the record's batch, written through `sprint ticket`,
//!   and the driver carries the record's handle ([`SprintHandle`]), so there is
//!   one truth about what the sprint carries and no batch copied beside it.
//! - **The machine performs no I/O.** No spawn, no git, no store read, no clock
//!   read: the caller supplies `at` (the stamp the record path will carry) and
//!   executes each [`Effect`] (dispatch through the harness HTTP API is part 2,
//!   preflight and the record writes are part 3). A dated entry rides as its two
//!   halves, `at` and `note`; the record path joins them the way `sprint run`
//!   does, so the machine never formats the store's own separator.

use super::query::HUMAN;
use super::ticket::Status;
use crate::store::timestamp;

/// How many tickets one draw takes. A cap, not a quota: fewer is a short batch,
/// never an error.
pub const MAX_BATCH: usize = 5;

/// Why a pass failed, or what it returned: the three fixed shapes the passes
/// keep (refiner: brief, builder: diff, reviewer: JSON verdict), plus the two
/// refusals the table needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassResult {
    /// The refiner shaped the ticket's brief. `ready` names the status the brief
    /// earns: true lands `ready`, false parks `on-hold` for the human.
    Refined { ticket: String, ready: bool },
    /// The builder landed a diff as a PR. `pr` is the review's handle.
    Built { ticket: String, pr: u64 },
    /// The reviewer read the head. True merged (two-way door, low blast radius);
    /// false escalated the one-way door to the human.
    Reviewed { ticket: String, merged: bool },
    /// The pass failed. The ticket and the pass name what to read; `reason` is
    /// prose for the run entry, never parsed back.
    Failed {
        ticket: String,
        pass: PassKind,
        reason: String,
    },
    /// A result the machine does not speak. [`step`] refuses it, never defaults it.
    Unknown { raw: String },
}

/// Which pass a result or a spawn names: refiner, builder or reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassKind {
    Refine,
    Build,
    Review,
}

impl PassKind {
    /// The word the briefs and the run entries use.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Refine => "refine",
            Self::Build => "build",
            Self::Review => "review",
        }
    }

    /// The pass a word names, or the refusal part 2 routes on. Unknown words are
    /// refused, never defaulted.
    pub fn parse(raw: &str) -> Result<Self, RunnerError> {
        match raw {
            "refine" => Ok(Self::Refine),
            "build" => Ok(Self::Build),
            "review" => Ok(Self::Review),
            _ => Err(RunnerError::UnknownResult(raw.to_string())),
        }
    }
}

/// What a transition was refused for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerError {
    /// The result names no pass the machine speaks.
    UnknownResult(String),
    /// The result names another ticket than the outstanding pass.
    OrphanResult { expected: String, got: String },
    /// The result names the outstanding ticket but the wrong pass for this state,
    /// or a call the state cannot take.
    UnexpectedResult(String),
    /// A terminal state (closing, done, halted) carries no pass, so no pass
    /// result applies to it.
    NoPassOutstanding(String),
    /// The caller's stamp is not the store's own ISO-8601 UTC form.
    InvalidStamp(String),
}

impl RunnerError {
    /// The words a caller acts on.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownResult(raw) => format!(
                "Unknown pass result \"{raw}\". The runner speaks refine, build and review results; nothing else."
            ),
            Self::OrphanResult { expected, got } => format!(
                "Pass result for \"{got}\" does not belong to the outstanding pass on \"{expected}\". Route each result to its own ticket's step."
            ),
            Self::UnexpectedResult(detail) => format!("The runner refused the step: {detail}."),
            Self::NoPassOutstanding(detail) => format!("{detail}, so no pass result applies."),
            Self::InvalidStamp(bad) => format!(
                "\"{bad}\" is not a timestamp. Stamps are ISO-8601 UTC, YYYY-MM-DDTHH:MM:SSZ."
            ),
        }
    }
}

/// One end of a dependency edge, as the draw reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockerRef {
    /// The blocking ticket's id.
    pub id: String,
    /// Its ref (`TAN-42`), when the lane carries one.
    pub reference: Option<String>,
    /// Its title, the fallback when the lane carries no ref.
    pub title: String,
    /// Its status, read so the skip line says what holds the ticket.
    pub status: Status,
}

impl BlockerRef {
    fn display_name(&self) -> String {
        self.reference.clone().unwrap_or_else(|| self.title.clone())
    }
}

/// The draw's snapshot input: one lane ticket in stored order, with only the
/// fields the draw rule reads. A summary, not the row: the record stays the
/// truth and this never writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketSnapshot {
    /// The ticket's id, the handle the batch carries.
    pub id: String,
    /// Its ref (`TAN-42`), the name skip lines draw.
    pub reference: Option<String>,
    /// Its title, the fallback when the lane carries no ref.
    pub title: String,
    /// Where it is in the workflow.
    pub status: Status,
    /// Whether an open edge holds it, derived the way `Ticket::blocked` reads it.
    pub blocked: bool,
    /// What holds it, in edge order.
    pub blockers: Vec<BlockerRef>,
    /// Who owns it, with `human` stored lowercase.
    pub assignee: Option<String>,
    /// Its body, read for the promotion rule and nothing else.
    pub description: Option<String>,
    /// Set aside: out of the draw whatever its status says.
    pub archived: bool,
}

impl TicketSnapshot {
    fn display_name(&self) -> String {
        self.reference.clone().unwrap_or_else(|| self.title.clone())
    }
}

/// One drawn ticket: its id and whether it refines before it builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawnTicket {
    /// The ticket's id, in run order.
    pub id: String,
    /// True for a backlog pick whose brief is still missing: it refines first. A
    /// promoted backlog pick (its brief already finished) builds straight away.
    pub needs_refine: bool,
}

/// What the draw decided: the batch in run order, the skip lines for the record,
/// and the promotions the caller writes before the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    /// The slots in run order: ready first, then backlog, each in stored order.
    pub batch: Vec<DrawnTicket>,
    /// One line per skipped row, naming the ticket and each blocker, in stored
    /// order. The caller prints them and files them in the sprint's first run
    /// note, so the record carries them after the transcript is gone.
    pub skips: Vec<String>,
    /// Backlog tickets whose bodies are already finished briefs: the caller
    /// promotes them to `ready` in the same pass.
    pub promotions: Vec<String>,
}

/// Whether a body is already a finished brief: the Why, What and Done when
/// sections are present, and the Open questions section is absent, empty, or the
/// explicit `(none)`. The assignee hold is the draw's check, not this one's.
pub fn is_finished_brief(description: Option<&str>) -> bool {
    let Some(text) = description else {
        return false;
    };
    let lines: Vec<&str> = text.lines().collect();
    let headed = |name: &str| {
        lines
            .iter()
            .any(|line| line.trim().to_lowercase().starts_with(name))
    };
    if !(headed("## why") && headed("## what") && headed("## done when")) {
        return false;
    }
    let Some(open) = lines
        .iter()
        .position(|line| line.trim().to_lowercase().starts_with("## open questions"))
    else {
        return true;
    };
    let mut body: Vec<&str> = Vec::new();
    for line in &lines[open + 1..] {
        if line.trim().starts_with("## ") {
            break;
        }
        body.push(line);
    }
    let text = body.join("\n");
    let text = text.trim();
    text.is_empty() || text.eq_ignore_ascii_case("(none)")
}

/// The draw: ready first, five at most, blocked skipped with named skip lines,
/// finished briefs promoted.
///
/// `tickets` is the lane in stored order. Ready and unblocked take the slots
/// first, backlog and unblocked fill what is left, each group keeping stored
/// order. `on-hold` is never drawn, `done` and archived never compete, and a
/// blocked row is skipped whatever its status: the edge, not the column, is the
/// block (ADR 0035). A skipped row is reported, never silent, and never an error.
pub fn draw(tickets: &[TicketSnapshot]) -> Draw {
    let mut promotions: Vec<String> = Vec::new();
    for ticket in tickets {
        if ticket.status == Status::Backlog
            && !ticket.archived
            && ticket.assignee.as_deref() != Some(HUMAN)
            && is_finished_brief(ticket.description.as_deref())
        {
            promotions.push(ticket.id.clone());
        }
    }
    let promoted = |id: &str| promotions.iter().any(|promoted| promoted == id);
    let eligible = |ticket: &TicketSnapshot| {
        !ticket.archived
            && !ticket.status.is_done()
            && ticket.status != Status::OnHold
            && !ticket.blocked
            && (ticket.status == Status::Ready || ticket.status == Status::Backlog)
    };

    let mut batch: Vec<DrawnTicket> = Vec::new();
    for ticket in tickets
        .iter()
        .filter(|ticket| ticket.status == Status::Ready && eligible(ticket))
    {
        if batch.len() == MAX_BATCH {
            break;
        }
        batch.push(DrawnTicket {
            id: ticket.id.clone(),
            needs_refine: false,
        });
    }
    if batch.len() < MAX_BATCH {
        for ticket in tickets
            .iter()
            .filter(|ticket| ticket.status == Status::Backlog && eligible(ticket))
        {
            if batch.len() == MAX_BATCH {
                break;
            }
            batch.push(DrawnTicket {
                id: ticket.id.clone(),
                needs_refine: !promoted(&ticket.id),
            });
        }
    }

    let mut skips: Vec<String> = Vec::new();
    for ticket in tickets.iter().filter(|ticket| {
        (ticket.status == Status::Ready || ticket.status == Status::Backlog)
            && !ticket.archived
            && ticket.blocked
    }) {
        skips.push(skip_line(ticket));
    }

    Draw {
        batch,
        skips,
        promotions,
    }
}

fn skip_line(ticket: &TicketSnapshot) -> String {
    let name = ticket.display_name();
    if ticket.blockers.is_empty() {
        return format!("skipped {name}: blocked");
    }
    let blockers: Vec<String> = ticket
        .blockers
        .iter()
        .map(|blocker| format!("{} ({})", blocker.display_name(), blocker.status.as_str()))
        .collect();
    format!("skipped {name}: blocked by {}", blockers.join(", "))
}

/// The sprint the driver runs: the record's handle, never a copy of its batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintHandle {
    /// The lane's stored spelling.
    pub lane: String,
    /// One more than the lane's highest number held, derived at open.
    pub number: i64,
}

/// The driver's state: the record's batch, the cursor into it, what already
/// landed, and the outstanding pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    /// The record this run writes to.
    pub sprint: SprintHandle,
    /// The record's batch in run order.
    pub batch: Vec<DrawnTicket>,
    /// The index of the outstanding ticket.
    pub cursor: usize,
    /// What merged, in run order, for the close note.
    pub completed: Vec<String>,
    /// What parked on the human while the rest of the batch ran on.
    pub parked: Vec<String>,
    /// The outstanding pass, or where the run stands.
    pub phase: Phase,
}

/// Where the run stands: the outstanding pass, or the terminal the run reached.
/// Closing still holds the close write; Done and Halted hold nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// A refine pass is out on the ticket.
    Refining { ticket: String },
    /// A build pass is out on the ticket.
    Building { ticket: String },
    /// A review pass is out on the ticket's PR.
    Reviewing { ticket: String, pr: u64 },
    /// Every ticket landed or parked aside: the close write is the only move left.
    Closing,
    /// The close landed.
    Done,
    /// A pass failed, escalated, or stalled on parked briefs: triage reads the
    /// ticket patch and the dated entry the effect carried, never prose.
    Halted { ticket: String, reason: String },
}

/// What follows a recorded ticket: the next ticket's spawn, or the close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Dispatch the next ticket's pass.
    Spawn { ticket: String, pass: PassKind },
    /// Write the close.
    Close,
}

/// The one request the transition emits. The caller executes it: dispatching a
/// pass (part 2, through the harness HTTP API), writing the ticket patch, and
/// appending the dated entry through the record path (part 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Dispatch a pass on the ticket.
    Spawn { ticket: String, pass: PassKind },
    /// The ticket merged: complete it, append the dated entry, then follow `next`.
    RecordAndAdvance {
        ticket: String,
        pr: u64,
        at: String,
        note: String,
        next: Next,
    },
    /// The brief parked on the human: hold the ticket, append the dated entry,
    /// then follow `next`, or stall when nothing is left to run.
    ParkAndAdvance {
        ticket: String,
        at: String,
        note: String,
        next: Option<Next>,
    },
    /// The pass failed: patch the ticket back to an actionable status with its
    /// owner cleared, and append the dated entry. The run halts; triage picks the
    /// ticket up from its status, not from prose.
    Fail {
        ticket: String,
        status: Status,
        assignee: Option<String>,
        at: String,
        note: String,
    },
    /// The verdict escalated to the human: hold the ticket in review, append the
    /// dated entry, and halt. The sprint waits for the merge.
    Escalate {
        ticket: String,
        status: Status,
        assignee: Option<String>,
        at: String,
        note: String,
    },
    /// Write the close (part 3 owns the stamp once, ADR 0032), then call
    /// [`closed`].
    Close { note: String },
    /// Nothing is outstanding: the draw was empty, so no sprint opens.
    None,
}

/// Open the run over a drawn batch. The first ticket's spawn is the effect; the
/// batch decides whether it refines first. An empty batch opens nothing: the
/// driver is done and there is no effect to execute.
pub fn start(sprint: SprintHandle, batch: &[DrawnTicket]) -> (Driver, Effect) {
    let idle = Driver {
        sprint,
        batch: batch.to_vec(),
        cursor: 0,
        completed: Vec::new(),
        parked: Vec::new(),
        phase: Phase::Done,
    };
    let Some(first) = batch.first() else {
        return (idle, Effect::None);
    };
    let phase = entry_phase(first);
    let effect = Effect::Spawn {
        ticket: first.id.clone(),
        pass: entry_pass(first),
    };
    (Driver { phase, ..idle }, effect)
}

fn entry_phase(ticket: &DrawnTicket) -> Phase {
    if ticket.needs_refine {
        Phase::Refining {
            ticket: ticket.id.clone(),
        }
    } else {
        Phase::Building {
            ticket: ticket.id.clone(),
        }
    }
}

fn entry_pass(ticket: &DrawnTicket) -> PassKind {
    if ticket.needs_refine {
        PassKind::Refine
    } else {
        PassKind::Build
    }
}

/// The transition: (driver state, pass result) maps to exactly one (next state,
/// effect request), or a named refusal.
///
/// `at` is the caller's stamp for the entries this step dates; a stamp the store
/// cannot read is refused before anything moves. Results for another ticket or
/// another pass are refused, and a terminal state carries no pass, so results
/// addressed to it are refused too.
pub fn step(
    state: &Driver,
    result: &PassResult,
    at: &str,
) -> Result<(Driver, Effect), RunnerError> {
    if let PassResult::Unknown { raw } = result {
        return Err(RunnerError::UnknownResult(raw.clone()));
    }
    if timestamp::decode(at).is_none() {
        return Err(RunnerError::InvalidStamp(at.to_string()));
    }
    let outstanding = match &state.phase {
        Phase::Refining { ticket } | Phase::Building { ticket } => ticket.clone(),
        Phase::Reviewing { ticket, .. } => ticket.clone(),
        Phase::Closing | Phase::Done | Phase::Halted { .. } => {
            return Err(RunnerError::NoPassOutstanding(format!(
                "{:?} carries no pass",
                state.phase
            )));
        }
    };
    let got = result_ticket(result);
    if got != outstanding {
        return Err(RunnerError::OrphanResult {
            expected: outstanding,
            got,
        });
    }
    match (&state.phase, result) {
        (
            Phase::Refining { ticket },
            PassResult::Refined {
                ticket: _,
                ready: true,
            },
        ) => {
            let next = Driver {
                phase: Phase::Building {
                    ticket: ticket.clone(),
                },
                ..state.clone()
            };
            Ok((
                next,
                Effect::Spawn {
                    ticket: ticket.clone(),
                    pass: PassKind::Build,
                },
            ))
        }
        (
            Phase::Refining { ticket },
            PassResult::Refined {
                ticket: _,
                ready: false,
            },
        ) => Ok(park(state, ticket, at)),
        (Phase::Building { ticket }, PassResult::Built { ticket: _, pr }) => {
            let next = Driver {
                phase: Phase::Reviewing {
                    ticket: ticket.clone(),
                    pr: *pr,
                },
                ..state.clone()
            };
            Ok((
                next,
                Effect::Spawn {
                    ticket: ticket.clone(),
                    pass: PassKind::Review,
                },
            ))
        }
        (
            Phase::Reviewing { ticket, pr },
            PassResult::Reviewed {
                ticket: _,
                merged: true,
            },
        ) => Ok(land(state, ticket, *pr, at)),
        (
            Phase::Reviewing { ticket, .. },
            PassResult::Reviewed {
                ticket: _,
                merged: false,
            },
        ) => {
            let reason = "the verdict escalated to the human".to_string();
            let next = Driver {
                phase: Phase::Halted {
                    ticket: ticket.clone(),
                    reason: reason.clone(),
                },
                ..state.clone()
            };
            Ok((
                next,
                Effect::Escalate {
                    ticket: ticket.clone(),
                    status: Status::InReview,
                    assignee: Some(HUMAN.to_string()),
                    at: at.to_string(),
                    note: format!("{ticket} escalated: {reason}."),
                },
            ))
        }
        (Phase::Refining { ticket }, PassResult::Failed { pass, reason, .. })
            if *pass == PassKind::Refine =>
        {
            Ok(halt(state, ticket, "refine", reason, at))
        }
        (Phase::Building { ticket }, PassResult::Failed { pass, reason, .. })
            if *pass == PassKind::Build =>
        {
            Ok(halt(state, ticket, "build", reason, at))
        }
        (Phase::Reviewing { ticket, .. }, PassResult::Failed { pass, reason, .. })
            if *pass == PassKind::Review =>
        {
            Ok(halt(state, ticket, "review", reason, at))
        }
        _ => Err(RunnerError::UnexpectedResult(format!(
            "{result:?} answers no outstanding pass in {:?}",
            state.phase
        ))),
    }
}

fn result_ticket(result: &PassResult) -> String {
    match result {
        PassResult::Refined { ticket, .. }
        | PassResult::Built { ticket, .. }
        | PassResult::Reviewed { ticket, .. }
        | PassResult::Failed { ticket, .. } => ticket.clone(),
        PassResult::Unknown { raw } => raw.clone(),
    }
}

fn advance(state: &Driver) -> (usize, Option<DrawnTicket>) {
    let cursor = state.cursor + 1;
    (cursor, state.batch.get(cursor).cloned())
}

fn spawn_next(ticket: &DrawnTicket) -> Next {
    Next::Spawn {
        ticket: ticket.id.clone(),
        pass: entry_pass(ticket),
    }
}

fn land(state: &Driver, ticket: &str, pr: u64, at: &str) -> (Driver, Effect) {
    let mut completed = state.completed.clone();
    completed.push(ticket.to_string());
    let (cursor, following) = advance(state);
    let (phase, next) = match following {
        Some(ticket) => (entry_phase(&ticket), spawn_next(&ticket)),
        Option::None => (Phase::Closing, Next::Close),
    };
    let next_state = Driver {
        cursor,
        completed,
        phase,
        ..state.clone()
    };
    (
        next_state,
        Effect::RecordAndAdvance {
            ticket: ticket.to_string(),
            pr,
            at: at.to_string(),
            note: format!("{ticket} landed, PR #{pr}."),
            next,
        },
    )
}

fn park(state: &Driver, ticket: &str, at: &str) -> (Driver, Effect) {
    let mut parked = state.parked.clone();
    parked.push(ticket.to_string());
    let (cursor, following) = advance(state);
    let (phase, next) = match following {
        Some(ticket) => (entry_phase(&ticket), Some(spawn_next(&ticket))),
        Option::None => (
            Phase::Halted {
                ticket: ticket.to_string(),
                reason: "parked briefs wait on the human".to_string(),
            },
            Option::None,
        ),
    };
    let next_state = Driver {
        cursor,
        parked,
        phase,
        ..state.clone()
    };
    (
        next_state,
        Effect::ParkAndAdvance {
            ticket: ticket.to_string(),
            at: at.to_string(),
            note: format!("{ticket} parked: the brief waits on the human."),
            next,
        },
    )
}

fn halt(state: &Driver, ticket: &str, pass: &str, reason: &str, at: &str) -> (Driver, Effect) {
    let next = Driver {
        phase: Phase::Halted {
            ticket: ticket.to_string(),
            reason: reason.to_string(),
        },
        ..state.clone()
    };
    (
        next,
        Effect::Fail {
            ticket: ticket.to_string(),
            status: Status::Ready,
            assignee: Option::None,
            at: at.to_string(),
            note: format!("{ticket} {pass} failed: {reason}."),
        },
    )
}

/// The close note: what landed, in run order. Read at [`Phase::Closing`], after
/// the last run entry.
pub fn close_note(state: &Driver) -> String {
    if state.completed.is_empty() {
        return "Nothing landed.".to_string();
    }
    format!("Landed {}.", state.completed.join(", "))
}

/// Stamp the close: [`Phase::Closing`] becomes [`Phase::Done`]. Any other phase
/// is refused, because the close comes after the batch lands, never before.
pub fn closed(state: &Driver) -> Result<Driver, RunnerError> {
    if state.phase != Phase::Closing {
        return Err(RunnerError::UnexpectedResult(format!(
            "only a closing run closes, got {:?}",
            state.phase
        )));
    }
    Ok(Driver {
        phase: Phase::Done,
        ..state.clone()
    })
}
