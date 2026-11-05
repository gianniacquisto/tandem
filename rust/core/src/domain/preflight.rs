//! Part 3 preflight and record: per-ticket gate facts, the rebase symbol check,
//! and the close sweep.
//!
//! The loop in `skills/tandem-build/SKILL.md` step 3 and `skills/tandem-sprint/SKILL.md`
//! section 7 is prose an agent reads: rev-list counts, head-sha-bound checks,
//! update-branch, the union read, dated run entries, the close sweep. This module
//! owns the decisions and leaves the effects to the caller. The core holds the
//! checks because it is headless and testable; main owns spawn, the harness
//! lifetime and every effect the machine emits (ADR 0037: transport is a seam,
//! the core never touches it).
//!
//! Three rules hold everywhere below:
//!
//! - **Per-ticket isolation.** [`preflight`] reads only its own ticket's two
//!   facts: a green head on its sha, and a branch not behind its base. No batch,
//!   no sibling, no count of what else runs. A raced step that let another
//!   ticket's state into a verdict is refused by construction: the input carries
//!   one ticket.
//! - **Verdicts report into the part 1 vocabulary, never an invented one.**
//!   A block becomes [`PassResult::Failed`] with `pass: Review`, and the step
//!   halts with [`Effect::Fail`] (status back to ready, owner cleared, a dated
//!   entry). No new status, no new record field (the brief's Decided against).
//! - **The record is the truth (ADR 0032), the board rows are the truth (ADR
//!   0035).** [`release_dead_claims`] reads the session rows and the ticket rows
//!   it settles; it keeps no copy of either. Run entries stay dated by the core
//!   through `sprint run`, which already exists.

use super::board::{Board, SetFields};
use super::edit::Edit;
use super::query::{HUMAN, TicketQuery};
use super::runner::{PassKind, PassResult};
use super::session::{SessionStatus, Sessions};
use super::ticket::Status;

/// One head check: the name the run shows and whether it is green.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// The check's name, as the run lists it.
    pub name: String,
    /// Whether the head is green, red or still pending.
    pub state: CheckState,
}

/// Whether one head check lets the gate read the head.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CheckState {
    /// The check passed on this head sha.
    Green,
    /// The check failed on this head sha.
    Red,
    /// The check has not settled on this head sha.
    Pending,
}

impl CheckState {
    /// The word the block and the run entry use.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Red => "red",
            Self::Pending => "pending",
        }
    }
}

/// The gate's two facts for one ticket: the head sha its checks ran on, and how
/// far its branch sits behind its base. One ticket, never a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPreflight {
    /// The ticket's id, the handle the batch carries.
    pub ticket: String,
    /// The PR under review.
    pub pr: u64,
    /// The head sha the checks ran on.
    pub head: String,
    /// How far the branch sits behind its base (`rev-list --left-right
    /// --count`, the left number). Zero means current.
    pub behind: u64,
    /// The head's checks, in run order.
    pub checks: Vec<Check>,
}

/// What the gate may do next for one ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightAction {
    /// The branch is behind: bring it up to date before any gate read. The
    /// caller runs `gh pr update-branch` and names the new base head.
    UpdateBranch {
        ticket: String,
        pr: u64,
        behind: u64,
        head: String,
    },
    /// The head is green and the branch is current: the gate may dispatch.
    ReadyToGate {
        ticket: String,
        pr: u64,
        head: String,
    },
    /// The head cannot merge: the check, its state and the sha it ran on.
    Blocked {
        ticket: String,
        pr: u64,
        head: String,
        check: String,
        state: CheckState,
    },
}

/// The preflight: behind first, then the head's checks.
///
/// A branch that went stale costs the gate its whole read before it learns it
/// cannot merge, so behind is checked before any check is read: a behind branch
/// requests update-branch whatever the checks say. A current branch with a red
/// or pending check blocks, naming the first unsettled check, its state and the
/// sha it ran on. Anything else is ready to gate.
pub fn preflight(facts: &TicketPreflight) -> PreflightAction {
    if facts.behind > 0 {
        return PreflightAction::UpdateBranch {
            ticket: facts.ticket.clone(),
            pr: facts.pr,
            behind: facts.behind,
            head: facts.head.clone(),
        };
    }
    if let Some(failing) = facts
        .checks
        .iter()
        .find(|check| check.state != CheckState::Green)
    {
        return PreflightAction::Blocked {
            ticket: facts.ticket.clone(),
            pr: facts.pr,
            head: facts.head.clone(),
            check: failing.name.clone(),
            state: failing.state.clone(),
        };
    }
    PreflightAction::ReadyToGate {
        ticket: facts.ticket.clone(),
        pr: facts.pr,
        head: facts.head.clone(),
    }
}

impl PreflightAction {
    /// A block as the part 1 vocabulary speaks it: a failed review whose reason
    /// names the check, its state and the sha. Anything else is not a failure.
    pub fn into_failed(&self) -> Option<PassResult> {
        match self {
            Self::Blocked {
                ticket,
                head,
                check,
                state,
                ..
            } => Some(PassResult::Failed {
                ticket: ticket.clone(),
                pass: PassKind::Review,
                reason: format!("check {check} is {} on {head}", state.as_str()),
            }),
            _ => None,
        }
    }
}

/// The rebase symbol check, run before the push.
///
/// A rebase can auto-merge a symbol the base renamed away, and the push would
/// land code that no longer compiles. The caller diffs the rebased branch
/// (`git diff origin/main...HEAD`) and stops before the push when a forbidden
/// symbol is still in it. The answer is the first offending line, so the stop
/// names what to read; a clean diff answers `None`.
pub fn find_stale_symbol(diff: &str, symbols: &[&str]) -> Option<String> {
    for line in diff.lines() {
        if symbols.iter().any(|symbol| line.contains(symbol)) {
            return Some(line.trim().to_string());
        }
    }
    None
}

/// One released claim: the ticket and the ended session that held it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasedClaim {
    /// The ticket's id, released back to ready.
    pub ticket: String,
    /// The ended session that held the claim, named in the signed comment.
    pub session: String,
}

/// The close sweep: release every dead claim.
///
/// A claim whose session has ended goes back to `ready` with its owner cleared,
/// dated and signed in a comment naming the ended session, so the next triage
/// reads status, not prose. A claim whose session still runs is left alone (a
/// claim is not a stall); `human` is left alone whatever the session says (a
/// parked question stays with the human until a comment plus `set` moves it); anything that is
/// not `in-progress` is not a claim. One comment per ticket the sweep moves,
/// none where nothing moved.
pub fn release_dead_claims(board: &Board, sessions: &Sessions, author: &str) -> Vec<ReleasedClaim> {
    let tickets = match board.list(&TicketQuery::default()) {
        Ok(tickets) => tickets,
        Err(_) => return Vec::new(),
    };
    let mut released = Vec::new();
    for ticket in tickets
        .iter()
        .filter(|ticket| ticket.status == Status::InProgress)
        .filter(|ticket| ticket.assignee.is_some())
        .filter(|ticket| ticket.assignee.as_deref() != Some(HUMAN))
    {
        let id = ticket.id.to_string();
        let carried = match sessions.for_ticket(&id) {
            Ok(carried) => carried,
            Err(_) => continue,
        };
        if carried.is_empty() {
            continue;
        }
        if carried
            .iter()
            .any(|session| session.status == SessionStatus::Running)
        {
            continue;
        }
        let Some(ended) = carried
            .iter()
            .find(|session| session.status == SessionStatus::Ended)
        else {
            continue;
        };
        let session_id = ended.id.clone();
        let patched = board.set(
            &id,
            SetFields {
                status: Edit::Set(Status::Ready),
                assignee: Edit::Set(None),
                ..SetFields::default()
            },
        );
        if patched.is_err() {
            continue;
        }
        let body = format!(
            "Released the dead claim: session {session_id} ended before the ticket landed. Ready for triage."
        );
        if board.append_comment(&id, author, &body).is_err() {
            continue;
        }
        released.push(ReleasedClaim {
            ticket: id,
            session: session_id,
        });
    }
    released
}
