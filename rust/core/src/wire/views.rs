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

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use ts_rs::TS;

use crate::bikeshed::{
    Backlink, BikeshedNode, MovedNote, NodeKind, NoteContent, Recovery, WriteRecord,
};
use crate::domain::session::{Session, SessionTokens};
use crate::domain::sprint::Sprint;
use crate::domain::usage::{UsageReport, UsageRow};
use crate::domain::{
    Comment, InlineStyle, LaneDeletion, MarkdownBlock, MarkdownInline, Ticket, TicketBlock,
};
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

/// A filesystem instant as the store's ISO-8601 UTC text.
fn stamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    encode(seconds)
}

// MARK: - Notes

/// One tree row: a folder or a note, with its children.
///
/// `kind` is `"folder"` or `"note"`, the two words the renderer branches on.
/// The walk is the core's own (`Bikeshed::tree`): the wire adds no ordering and
/// no filtering, and the hidden layer is drawn only when the read asked for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteNodeView {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub children: Vec<NoteNodeView>,
}

impl NoteNodeView {
    pub fn new(node: &BikeshedNode) -> Self {
        Self {
            name: node.name.clone(),
            path: node.path.clone(),
            kind: match node.kind {
                NodeKind::Folder => "folder".to_string(),
                NodeKind::Note => "note".to_string(),
            },
            children: node.children.iter().map(Self::new).collect(),
        }
    }
}

/// `note.read`: the note's text, its content hash and its modification stamp.
///
/// The hash is the save discipline's whole conflict rule: the buffer tracks the
/// hash it last agreed with, and a read whose hash differs is an external write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteContentView {
    pub path: String,
    pub text: String,
    pub hash: String,
    pub modified_at: String,
}

impl NoteContentView {
    pub fn new(path: &str, content: &NoteContent) -> Self {
        Self {
            path: path.to_string(),
            text: content.text.clone(),
            hash: content.hash.clone(),
            modified_at: stamp(content.modified_at),
        }
    }
}

/// `note.write`: the path written and the hash of the bytes that landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteWriteView {
    pub path: String,
    pub hash: String,
}

impl NoteWriteView {
    pub fn new(record: &WriteRecord) -> Self {
        Self {
            path: record.path.clone(),
            hash: record.hash.clone(),
        }
    }
}

/// One note whose path changed in a move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MovedNoteView {
    pub old_path: String,
    pub new_path: String,
}

impl MovedNoteView {
    fn new(note: &MovedNote) -> Self {
        Self {
            old_path: note.old_path.clone(),
            new_path: note.new_path.clone(),
        }
    }
}

/// `note.move`: where the note or folder landed, and every note it carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteMoveView {
    pub path: String,
    pub old_path: String,
    pub is_folder: bool,
    pub moved_notes: Vec<MovedNoteView>,
}

impl NoteMoveView {
    pub fn new(outcome: &crate::bikeshed::MoveOutcome) -> Self {
        Self {
            path: outcome.path.clone(),
            old_path: outcome.old_path.clone(),
            is_folder: outcome.is_folder,
            moved_notes: outcome.moved_notes.iter().map(MovedNoteView::new).collect(),
        }
    }
}

/// One backlink row: the note that links here, and the line of its first link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkView {
    pub path: String,
    pub line: String,
}

impl BacklinkView {
    pub fn new(backlink: &Backlink) -> Self {
        Self {
            path: backlink.path.clone(),
            line: backlink.line.clone(),
        }
    }
}

/// One orphaned temp a killed save left: the file, its size and its stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryView {
    pub file: String,
    pub bytes: u64,
    pub modified_at: String,
}

impl RecoveryView {
    pub fn new(recovery: &Recovery) -> Self {
        Self {
            file: recovery.file.clone(),
            bytes: recovery.bytes,
            modified_at: stamp(recovery.modified_at),
        }
    }
}

// MARK: - Usage

/// The five token pieces a record reported, each absent on the wire when it was
/// not reported: the face draws the absent mark, never a zero dressed up as a
/// count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageTokensView {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
}

impl UsageTokensView {
    fn new(tokens: &SessionTokens) -> Self {
        Self {
            input: tokens.input,
            output: tokens.output,
            reasoning: tokens.reasoning,
            cache_read: tokens.cache_read,
            cache_write: tokens.cache_write,
        }
    }
}

/// One record behind a row, as the roll-down draws it: the session id, the agent
/// and model it was stamped with, its local start, and what it reported. The
/// record's brief and summary belong to the session surfaces, not to this one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageSessionView {
    pub id: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub started_at: String,
    pub cost: Option<f64>,
    pub tokens: Option<UsageTokensView>,
    /// The tickets the record carried, as ids: the face resolves one to its ref
    /// and opens its sheet.
    pub tickets: Vec<String>,
}

impl UsageSessionView {
    fn new(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            agent: session.agent.clone(),
            model: session.model.clone(),
            started_at: session.started_at.clone(),
            cost: session.cost,
            tokens: session.tokens.as_ref().map(UsageTokensView::new),
            tickets: session.tickets.clone(),
        }
    }
}

/// One line of the table: its label, its record count, its tokens, its cost and
/// the bar's width, all from the report.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageRowView {
    pub id: String,
    pub label: String,
    pub records: usize,
    pub reported: usize,
    pub cost: f64,
    /// The bar's width as a percentage of the row's own table, derived with the
    /// report: the face writes the width and computes no percentage (PRT-15).
    pub bar_percent: f64,
    pub tokens: UsageTokensView,
    /// The records behind the row, newest start first.
    pub sessions: Vec<UsageSessionView>,
}

impl UsageRowView {
    fn new(row: &UsageRow) -> Self {
        Self {
            id: row.id.clone(),
            label: row.label.clone(),
            records: row.records,
            reported: row.reported,
            cost: row.cost,
            bar_percent: row.bar_percent,
            tokens: UsageTokensView::new(&row.tokens),
            sessions: row.sessions.iter().map(UsageSessionView::new).collect(),
        }
    }

    fn rows(rows: &[UsageRow]) -> Vec<Self> {
        rows.iter().map(Self::new).collect()
    }
}

/// `usage.report`: the whole report the Usage face draws, one message.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageReportView {
    pub records: usize,
    pub reported: usize,
    pub cost: f64,
    pub tokens: UsageTokensView,
    pub day: Vec<UsageRowView>,
    pub model: Vec<UsageRowView>,
    pub agent: Vec<UsageRowView>,
    pub ticket: Vec<UsageRowView>,
    /// The ticket rows' excess over `cost`: a shared record's cost per extra
    /// ticket.
    pub overlap: f64,
    /// What the ticket rows add up to, `cost + overlap`, said once in the
    /// header.
    pub ticket_total: f64,
}

impl UsageReportView {
    pub fn new(report: &UsageReport) -> Self {
        Self {
            records: report.records,
            reported: report.reported,
            cost: report.cost,
            tokens: UsageTokensView::new(&report.tokens),
            day: UsageRowView::rows(&report.day),
            model: UsageRowView::rows(&report.model),
            agent: UsageRowView::rows(&report.agent),
            ticket: UsageRowView::rows(&report.ticket),
            overlap: report.overlap,
            ticket_total: report.ticket_total(),
        }
    }
}

/// One styled span of a ticket's markdown: the text, the markers it wears, and
/// the destination a link pointed at. `styles` is a list because the domain
/// keeps a set (`**bold with `code`**` is one run carrying both).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownInlineView {
    pub text: String,
    pub styles: Vec<String>,
    pub link: Option<String>,
}

impl MarkdownInlineView {
    pub fn new(run: &MarkdownInline) -> Self {
        Self {
            text: run.text.clone(),
            styles: run
                .styles
                .iter()
                .map(|style| {
                    match style {
                        InlineStyle::Bold => "bold",
                        InlineStyle::Italic => "italic",
                        InlineStyle::Strikethrough => "strikethrough",
                        InlineStyle::Code => "code",
                    }
                    .to_string()
                })
                .collect(),
            link: run.link.clone(),
        }
    }
}

/// One block of a ticket's markdown, ready to draw (`TicketMarkdown`, TAN-219).
///
/// The tag is `kind`, so a face switches on one field and never on the shape of
/// the object. The parsing is the core's: the sheet sends the text it holds and
/// draws what comes back, so the app and the CLI read one dialect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MarkdownBlockView {
    Heading {
        level: usize,
        runs: Vec<MarkdownInlineView>,
    },
    Paragraph {
        runs: Vec<MarkdownInlineView>,
    },
    List {
        ordered: bool,
        start: i64,
        items: Vec<Vec<MarkdownBlockView>>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Quote {
        blocks: Vec<MarkdownBlockView>,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Rule,
    Task {
        checked: bool,
        blocks: Vec<MarkdownBlockView>,
    },
}

impl MarkdownBlockView {
    pub fn new(block: &MarkdownBlock) -> Self {
        match block {
            MarkdownBlock::Heading { level, runs } => Self::Heading {
                level: *level,
                runs: runs.iter().map(MarkdownInlineView::new).collect(),
            },
            MarkdownBlock::Paragraph { runs } => Self::Paragraph {
                runs: runs.iter().map(MarkdownInlineView::new).collect(),
            },
            MarkdownBlock::List {
                ordered,
                start,
                items,
            } => Self::List {
                ordered: *ordered,
                start: *start,
                items: items
                    .iter()
                    .map(|item| item.iter().map(Self::new).collect())
                    .collect(),
            },
            MarkdownBlock::Code { language, text } => Self::Code {
                language: language.clone(),
                text: text.clone(),
            },
            MarkdownBlock::Quote(blocks) => Self::Quote {
                blocks: blocks.iter().map(Self::new).collect(),
            },
            MarkdownBlock::Table { header, rows } => Self::Table {
                header: header.clone(),
                rows: rows.clone(),
            },
            MarkdownBlock::Rule => Self::Rule,
            MarkdownBlock::Task { checked, blocks } => Self::Task {
                checked: *checked,
                blocks: blocks.iter().map(Self::new).collect(),
            },
        }
    }

    /// Every block of a document, in order.
    pub fn blocks(blocks: &[MarkdownBlock]) -> Vec<Self> {
        blocks.iter().map(Self::new).collect()
    }
}
