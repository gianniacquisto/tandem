//! The agent verbs' answers, in the shapes the Swift CLI prints (PRT-9).
//!
//! Ported from `Sources/TandemCore/Encoding.swift`: one struct per result, the
//! domain type projected into the wire's field names and its ISO-8601 stamps.
//! The CLI shapes nothing of its own: it calls a constructor here and prints the
//! serialization, so a verb's JSON is the same fact on both binaries.
//!
//! Two projections are deliberate, not decoration:
//!
//! - **`ticket list` is a summary.** The body is the one field that scales with
//!   the writing, so a list row carries `descriptionLength` in its place and
//!   drops `blockedReason` (also body text). `ticket show` and the write verbs'
//!   receipts carry the whole ticket.
//! - **`note` is a top-level key, present only when the write changed nothing.**
//!   A re-run of `session start`, `sprint open` or `lane add` is a success that
//!   says so, and the note is how it says it.

use serde::Serialize;
use ts_rs::TS;

use crate::domain::session::Session;
use crate::domain::sprint::Sprint;
use crate::domain::{Comment, LaneDeletion, Ticket, TicketBlock};
use crate::store::timestamp::encode;

/// One end of a dependency edge: the other ticket's id, ref, title and status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TicketBlockView {
    pub id: String,
    /// The wire key is `ref`; `reference` is the domain's name for it.
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
}

impl TicketBlockView {
    fn new(block: &TicketBlock) -> Self {
        Self {
            id: block.id.to_string(),
            reference: block.reference.clone(),
            title: block.title.clone(),
            status: block.status.as_str().to_string(),
        }
    }
}

/// A whole ticket: `ticket show`, and the receipt `add`/`set`/`complete` print.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TicketDetail {
    pub archived: bool,
    pub assignee: Option<String>,
    pub blocked: bool,
    pub blocked_by: Vec<TicketBlockView>,
    pub blocked_reason: Option<String>,
    pub blocks: Vec<TicketBlockView>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub description: Option<String>,
    pub done: bool,
    pub id: String,
    pub lane: Option<String>,
    pub link: Option<String>,
    pub number: Option<i64>,
    pub origin: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub status: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl TicketDetail {
    pub fn new(ticket: &Ticket) -> Self {
        Self {
            archived: ticket.archived,
            assignee: ticket.assignee.clone(),
            blocked: ticket.blocked(),
            blocked_by: ticket.blocked_by.iter().map(TicketBlockView::new).collect(),
            blocked_reason: ticket.blocked_reason(),
            blocks: ticket.blocks.iter().map(TicketBlockView::new).collect(),
            completed_at: ticket.completed_at.map(encode),
            created_at: encode(ticket.created_at),
            description: ticket.description.clone(),
            done: ticket.done(),
            id: ticket.id.to_string(),
            lane: ticket.lane.clone(),
            link: ticket.link.clone(),
            number: ticket.number,
            origin: ticket.origin.clone(),
            reference: ticket.reference.clone(),
            status: ticket.status.as_str().to_string(),
            title: ticket.title.clone(),
            note: None,
        }
    }

    /// The same ticket, with the note a write that changed nothing answers.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }
}

/// A list row: the ticket's shape with the body projected to its length.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TicketSummary {
    pub archived: bool,
    pub assignee: Option<String>,
    pub blocked: bool,
    pub blocked_by: Vec<TicketBlockView>,
    pub blocks: Vec<TicketBlockView>,
    pub completed_at: Option<String>,
    pub created_at: String,
    /// Characters, not bytes: the Swift CLI counts what a reader counts.
    pub description_length: usize,
    pub done: bool,
    pub id: String,
    pub lane: Option<String>,
    pub link: Option<String>,
    pub number: Option<i64>,
    pub origin: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub status: String,
    pub title: String,
}

impl TicketSummary {
    pub fn new(ticket: &Ticket) -> Self {
        Self {
            archived: ticket.archived,
            assignee: ticket.assignee.clone(),
            blocked: ticket.blocked(),
            blocked_by: ticket.blocked_by.iter().map(TicketBlockView::new).collect(),
            blocks: ticket.blocks.iter().map(TicketBlockView::new).collect(),
            completed_at: ticket.completed_at.map(encode),
            created_at: encode(ticket.created_at),
            description_length: ticket
                .description
                .as_deref()
                .map(|body| body.chars().count())
                .unwrap_or(0),
            done: ticket.done(),
            id: ticket.id.to_string(),
            lane: ticket.lane.clone(),
            link: ticket.link.clone(),
            number: ticket.number,
            origin: ticket.origin.clone(),
            reference: ticket.reference.clone(),
            status: ticket.status.as_str().to_string(),
            title: ticket.title.clone(),
        }
    }
}

/// `ticket list`: `{"tickets": [...]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TicketList {
    pub tickets: Vec<TicketSummary>,
}

/// One comment: the object `ticket comment add` prints and a list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CommentView {
    pub at: String,
    pub author: String,
    pub body: String,
}

impl CommentView {
    pub fn new(comment: &Comment) -> Self {
        Self {
            at: encode(comment.at),
            author: comment.author.clone(),
            body: comment.body.clone(),
        }
    }
}

/// `ticket comment add`: the comment, with the note when the write changed
/// nothing (it cannot, but the shape is the other writes').
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommentResult {
    #[serde(flatten)]
    pub comment: CommentView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `ticket comment list`: `{"comments": [...]}`, append order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommentList {
    pub comments: Vec<CommentView>,
}

/// `lane list`, `lane add`, `lane rename`: `{"lanes": [...]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneList {
    pub lanes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `lane delete`: the lanes left, and how many tickets moved to the default area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaneDeletionView {
    pub lanes: Vec<String>,
    pub moved: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl LaneDeletionView {
    pub fn new(deletion: &LaneDeletion, note: Option<String>) -> Self {
        Self {
            lanes: deletion.lanes.clone(),
            moved: deletion.moved,
            note,
        }
    }
}

/// `ticket archive` without an id: `{"archived": n}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchivedView {
    pub archived: usize,
}

/// A session write's answer: the record, and the note when nothing changed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionResult {
    #[serde(flatten)]
    pub session: Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `session show`: the record plus the note paths the session wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionDetailView {
    #[serde(flatten)]
    pub session: Session,
    /// Derived from the capture files, never stored (ADR 0016).
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `session list`: `{"sessions": [...]}`, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionList {
    pub sessions: Vec<Session>,
}

/// A sprint write's answer: the record, and the note when nothing changed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SprintResult {
    #[serde(flatten)]
    pub sprint: Sprint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `sprint list`: `{"sprints": [...]}`, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SprintList {
    pub sprints: Vec<Sprint>,
}

/// `note rename` / `note capture`: `{"path": ...}`, and the duplicate's note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotePathView {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
