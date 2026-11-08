//! The ticket: the board's only entity (`Ticket.swift`, ADR 0017 and ADR 0020).
//!
//! A ticket carries the full shape: where it is in the workflow (`status`), whether
//! it is set aside (`archived`), what it is about (`description`), who is on it
//! (`assignee`), who filed it (`origin`), which cycle lane it belongs to (`lane`)
//! and the two ends of a dependency edge (`blocked_by`, `blocks`).
//!
//! Three axes stay independent on purpose (the brief's `code` lens): a status
//! transition is free-form and validates only the value, `archived` is a flag the
//! human sets, and the lane is a grouping. Nothing here has a transition table.
//!
//! **Derived, never stored:** `done` (`status == Done`), `blocked` (an edge whose
//! blocker is not done) and `blocked_reason` (the body's `## Blocked on` section).
//! The ref (`KEY-N`) is derived by [`Contents`](super::lane::Contents) from the
//! lane's key and the ticket's number on every read.
//!
//! Time is whole seconds since the Unix epoch: the store writes ISO-8601 UTC text,
//! and whole seconds is that same fact without a parser in the middle. A ticket read
//! back equals the one written, which is what every round-trip case asserts.

use uuid::Uuid;

use super::description::DescriptionBody;
use super::error::DomainError;
use crate::store::timestamp;

/// Where a ticket is in the workflow (`Status.swift`, ADR 0017).
///
/// The six default stages, and a stage a workflow declares that this build does
/// not name (`Custom`). The transitions between them are free-form: the value is
/// validated against the active workflow, never the move, so a ticket can go
/// backwards as easily as forwards. A status move itself is never refused.
/// `on-hold` is the parked slot (ADR 0017's 2026-10-29 amendment).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// Nobody has looked at it yet. The default for a new ticket.
    Backlog,
    /// Parked on purpose: a decision waiting, a blocker, a "not now".
    OnHold,
    /// A brief with no question open: the next pass can pick it up.
    Ready,
    InProgress,
    /// The work is done and its PR is waiting for a reader.
    InReview,
    Done,
    /// A stage a workflow declares that the default six do not name. Its key is
    /// the stored word, so a custom stage round-trips without a second truth.
    Custom(String),
}

impl Status {
    /// Every stage, in workflow order: the board's columns and the error's list.
    pub const WORKFLOW: [Status; 6] = [
        Status::Backlog,
        Status::OnHold,
        Status::Ready,
        Status::InProgress,
        Status::InReview,
        Status::Done,
    ];

    /// Every wire value, in workflow order.
    pub const WIRE_VALUES: [&'static str; 6] = [
        "backlog",
        "on-hold",
        "ready",
        "in-progress",
        "in-review",
        "done",
    ];

    /// The word the store and the wire carry.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Backlog => "backlog",
            Self::OnHold => "on-hold",
            Self::Ready => "ready",
            Self::InProgress => "in-progress",
            Self::InReview => "in-review",
            Self::Done => "done",
            Self::Custom(key) => key,
        }
    }

    /// The phrase every face draws: the default's phrase, or the key a custom
    /// stage carries until the active workflow's label is read.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Backlog => "Backlog",
            Self::OnHold => "On hold",
            Self::Ready => "Ready",
            Self::InProgress => "In progress",
            Self::InReview => "In Review",
            Self::Done => "Done",
            Self::Custom(key) => key,
        }
    }

    /// The domain boundary: the default six by name, every other non-empty key
    /// as a custom stage. Membership in the active workflow is the board's
    /// question (`Board`), because the workflow is what knows the stage set.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let raw = raw.trim();
        if raw.is_empty() || raw == RETIRED_ON_HOLD {
            return Err(DomainError::InvalidStatus(raw.to_string()));
        }
        Ok(match raw {
            "backlog" => Self::Backlog,
            "on-hold" => Self::OnHold,
            "ready" => Self::Ready,
            "in-progress" => Self::InProgress,
            "in-review" => Self::InReview,
            "done" => Self::Done,
            other => Self::Custom(other.to_string()),
        })
    }

    /// The lenient edge: `on-hold` for the retired spelling `refined`, `backlog`
    /// for an empty stored word, and the key itself for a stage a workflow
    /// declares. A custom key round-trips; an older build maps it to backlog as
    /// the reader-first rule says.
    pub fn stored(raw: &str) -> Self {
        if raw == RETIRED_ON_HOLD {
            return Self::OnHold;
        }
        if raw.trim().is_empty() {
            return Self::Backlog;
        }
        Self::parse(raw).unwrap_or(Self::Backlog)
    }

    /// Whether this stage is the finished one.
    pub fn is_done(&self) -> bool {
        *self == Self::Done
    }
}

/// The spelling a store may still hold for `on-hold`: history, never a wire value.
const RETIRED_ON_HOLD: &str = "refined";

/// One end of a dependency edge, as a ticket reads it: the other ticket's id, its
/// ref, its title and its status (TAN-232). Derived from the edge row on every
/// read, so a lane move or a key move updates the display without touching the
/// edge.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TicketBlock {
    pub id: Uuid,
    /// The other ticket's ref (`TAN-42`), `None` in the default area.
    pub reference: Option<String>,
    pub title: String,
    pub status: Status,
}

/// A ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub id: Uuid,
    pub title: String,
    pub status: Status,
    /// Set aside: out of the default views, still in the store.
    pub archived: bool,
    /// The body: markdown, trimmed at write time; an empty result means `None`.
    pub description: Option<String>,
    /// The human (`human`), an agent's name, or nobody.
    pub assignee: Option<String>,
    /// Who filed the ticket. Written once, at add, and never changed (ADR 0020).
    pub origin: String,
    /// The cycle lane, or `None` for the default (unnamed) area.
    pub lane: Option<String>,
    /// The ticket's number within its lane, issued by the lane's counter.
    pub number: Option<i64>,
    /// `KEY-N`, the lane's key and this ticket's number. Derived, never stored.
    pub reference: Option<String>,
    /// A bikeshed-root-relative note path, or `None` for an unlinked ticket.
    pub link: Option<String>,
    /// What this ticket waits on, in the order the edges were set. Derived.
    pub blocked_by: Vec<TicketBlock>,
    /// Who waits on this ticket, the far end of the same edges. Derived.
    pub blocks: Vec<TicketBlock>,
    /// When the ticket was filed, whole seconds since the Unix epoch.
    pub created_at: i64,
    /// When it entered `done`, cleared when it left.
    pub completed_at: Option<i64>,
}

impl Ticket {
    /// A ticket with the shape's defaults: backlog, unarchived, unassigned, filed
    /// by the human, in the default area, created now.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            status: Status::Backlog,
            archived: false,
            description: None,
            assignee: None,
            origin: "human".to_string(),
            lane: None,
            number: None,
            reference: None,
            link: None,
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            created_at: timestamp::unix_now(),
            completed_at: None,
        }
    }

    /// The legacy v1 field, derived: never a second truth to disagree with
    /// `status`.
    pub fn done(&self) -> bool {
        self.status.is_done()
    }

    /// Whether this ticket is waiting: any edge whose blocker is not done. Derived,
    /// so a block lifts the moment its blocker reaches done, with no write here.
    pub fn blocked(&self) -> bool {
        self.blocked_by.iter().any(|edge| !edge.status.is_done())
    }

    /// Why it waits: the body's `## Blocked on` section, read from the description
    /// and never copied into a column.
    pub fn blocked_reason(&self) -> Option<String> {
        DescriptionBody::blocked_reason(self.description.as_deref())
    }

    /// Move the ticket in the workflow, keeping `completed_at` truthful: entering
    /// done stamps it, leaving done clears it.
    pub fn set_status(&mut self, status: Status, now: i64) {
        if status.is_done() {
            if !self.status.is_done() {
                self.completed_at = Some(now);
            }
        } else {
            self.completed_at = None;
        }
        self.status = status;
    }
}
