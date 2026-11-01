//! The rpc protocol: the frames `tandem rpc` speaks on its stream.
//!
//! The app's only door to the core (`docs/port.md`, Transport and the UI
//! contract), so the shapes are the contract and live here, beside the CLI's
//! views, rather than in the process that reads stdin. One message set, one
//! owner: `ts-rs` generates the TypeScript declarations from these types, and a
//! later HTTPS adapter carries the same frames over another transport.
//!
//! Three rules the types encode:
//!
//! - **A frame is one JSON object on one line.** A request carries `id`,
//!   `method` and `params`; a response echoes `id` and carries `result` or
//!   `error`; a change event carries `event` and the store's new `revision`.
//! - **Every reply that carries a revision is a snapshot at that number.** There
//!   is no paging: the human's board is cheap to send whole, and a revision
//!   makes the copy correct. A read reply reads its rows and its number in one
//!   read transaction; a write receipt reads the number inside the write's own
//!   transaction. **The reply's revision is authoritative**: the payload is the
//!   store at exactly that number. An event is a hint to re-read, never a state
//!   to patch in: it can name an intermediate revision (a migration is a commit
//!   of its own) and it can be superseded before the client reads.
//! - **A method is a domain verb, never a row.** The names are the CLI's
//!   grammar (`ticket.add` is `ticket add`), and [`METHODS`] is the table the
//!   parity test walks: every write names a CLI verb, so no UI-only write can
//!   appear without the CLI growing the same act. A read is the one method that
//!   may name no verb (`usage.report`), and the table says so with an empty one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bikeshed::health::BikeshedHealth;
use crate::wire::views::{
    BacklinkView, CommentView, MarkdownBlockView, NoteContentView, NoteMoveView, NoteNodeView,
    NoteWriteView, RecoveryView, TicketDetail, TicketSummary, UsageReportView,
};

// MARK: - Requests

/// One call: its correlation id and the method with its parameters.
///
/// The method is flattened into the frame, so the wire reads
/// `{"id": 1, "method": "ticket.add", "params": {"title": "First"}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Request {
    /// Echoed on the response. The client chooses it; the server never reuses
    /// one.
    pub id: u64,
    #[serde(flatten)]
    pub method: Method,
}

/// Every method the wire speaks, generated together with [`METHODS`] so the
/// table and the enum cannot drift.
///
/// A method is one act: the CLI's verb, the app's call and the wire's message
/// are the same thing spelled for three readers. Adding one here without the
/// CLI growing the verb is what the parity test catches.
macro_rules! methods {
    ( $( $variant:ident : $tag:literal, $params:ty, $verb:literal; )* ) => {
        /// One method and its parameters. The tag is the wire's spelling and
        /// the verb is the CLI's, one act under two names.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
        #[serde(tag = "method", content = "params")]
        pub enum Method {
            $(
                #[serde(rename = $tag)]
                $variant($params),
            )*
        }

        /// Every method the wire speaks: its tag, and the CLI verb that does the
        /// same work. The parity test walks this table against the grammar.
        pub const METHODS: &[(&str, &str)] = &[ $( ($tag, $verb), )* ];

        impl Method {
            /// The wire's spelling of this method.
            pub fn tag(&self) -> &'static str {
                match self {
                    $( Self::$variant(_) => $tag, )*
                }
            }

            /// The CLI verb that performs the same act.
            pub fn cli_verb(&self) -> &'static str {
                match self {
                    $( Self::$variant(_) => $verb, )*
                }
            }
        }
    };
}

methods! {
    TicketList: "ticket.list", TicketListParams, "ticket list";
    TicketShow: "ticket.show", TicketIdParams, "ticket show";
    TicketAdd: "ticket.add", TicketAddParams, "ticket add";
    TicketAddFromNote: "ticket.addFromNote", TicketAddFromNoteParams, "ticket file";
    TicketSet: "ticket.set", TicketSetParams, "ticket set";
    TicketMove: "ticket.move", TicketMoveParams, "ticket move";
    TicketBlock: "ticket.block", TicketBlockParams, "ticket block";
    TicketUnblock: "ticket.unblock", TicketUnblockParams, "ticket unblock";
    TicketComplete: "ticket.complete", TicketIdParams, "ticket complete";
    TicketAnswer: "ticket.answer", TicketAnswerParams, "ticket answer";
    TicketArchive: "ticket.archive", TicketArchiveParams, "ticket archive";
    TicketCommentList: "ticket.comment.list", TicketIdParams, "ticket comment list";
    TicketCommentAdd: "ticket.comment.add", CommentAddParams, "ticket comment add";
    LaneList: "lane.list", EmptyParams, "lane list";
    LaneAdd: "lane.add", LaneAddParams, "lane add";
    LaneRename: "lane.rename", LaneRenameParams, "lane rename";
    LaneDelete: "lane.delete", LaneDeleteParams, "lane delete";
    // The Bikeshed face's reads. The CLI spells no verb for them today, and
    // they write nothing, so no UI-only write hides behind an empty verb. The
    // two mutations name the verbs they share with the agent face.
    NoteTree: "note.tree", NoteTreeParams, "";
    NoteRead: "note.read", NotePathParams, "";
    NoteHealth: "note.health", EmptyParams, "note links";
    NoteBacklinks: "note.backlinks", NotePathParams, "";
    NoteRecoveries: "note.recoveries", EmptyParams, "";
    NoteWrite: "note.write", NoteWriteParams, "note write";
    NoteMove: "note.move", NoteMoveParams, "note move";
    SetupStatus: "setup.status", EmptyParams, "setup status";
    SetupInstall: "setup.install", SetupInstallParams, "setup install";
    DoctorReport: "doctor.report", EmptyParams, "doctor";
    // A read the CLI does not spell a verb for: the Usage face's one report. It
    // writes nothing, so no UI-only write hides behind it, and the report is the
    // derivation `usage_report` already froze (PRT-15). An empty verb is how the
    // table says so; `the_read_methods_without_a_cli_verb_are_named_here` names it.
    UsageReport: "usage.report", EmptyParams, "";
    // The settings the faces read, in one read message: the sheet's assignee
    // picker and the board's filter both offer the store's roster (PRT-38),
    // which is a `settings` row. The read writes nothing, so no UI-only write
    // hides behind its empty verb; the write below names the CLI verb it
    // shares with the Settings face's Save.
    SettingsGet: "settings.get", EmptyParams, "";
    // The roster write: the Settings face's Save and `settings set` spell one
    // act, the whole-list replace `domain::Settings::set_roster` performs.
    SettingsSet: "settings.set", SettingsSetParams, "settings set";
    // The ticket sheet's read: a body in, the blocks it draws out. The parsing
    // is the core's (`TicketMarkdown`), so the app and the CLI cannot read one
    // dialect two ways, and the message writes nothing: an empty verb.
    MarkdownBlocks: "markdown.blocks", MarkdownBlocksParams, "";
}

/// `markdown.blocks`: the text to parse. Absent is no body, which draws nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct MarkdownBlocksParams {
    pub text: Option<String>,
}

/// `settings.set`: the roster's whole replacement list. Absent is the empty
/// list, which stores as empty: the write replaces whole, so there is no
/// "unchanged" to spell, and an emptied roster is the human's removal rather
/// than a value no build can read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsSetParams {
    pub roster: Vec<String>,
}

/// A method that takes nothing. The frame still carries `"params": {}`, so a
/// client always sends the same three keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct EmptyParams {}

/// `note.tree`: whether the hidden layer is drawn. The flag reaches the tree
/// walk only; the linkable graph is always the curated layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteTreeParams {
    pub hidden: bool,
}

/// A method that names one note.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct NotePathParams {
    pub path: Option<String>,
}

/// `note.write`: the path and its whole text. Both optional so the frame's
/// empty-params round-trip still parses; a missing path is refused by the
/// domain, never silently written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteWriteParams {
    pub path: Option<String>,
    pub text: Option<String>,
}

/// `note.move`: the note or folder, and the folder it moves into.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteMoveParams {
    pub path: Option<String>,
    pub into: Option<String>,
}

/// `ticket.list`: the CLI's filters, every one optional.
///
/// `filter` is the shipped three-value scope (`open`, `done`, `all`); the named
/// axes narrow it. Absent fields mean the CLI's absent flags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketListParams {
    pub filter: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub lane: Option<String>,
    pub archived: bool,
    pub blocked: bool,
}

/// A method that names one ticket.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketIdParams {
    pub id: Option<String>,
}

/// `ticket.add`: the same fields the verb's flags carry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketAddParams {
    pub title: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub lane: Option<String>,
    pub origin: Option<String>,
    pub description: Option<String>,
}

/// `ticket.addFromNote`: the selector's text filed as a ticket, linked to the
/// note it came from. The title is the core's own derivation
/// (`Board::add_from_note`), never the wire's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketAddFromNoteParams {
    /// The bikeshed-root-relative note the text came from.
    pub note: Option<String>,
    /// The selection, or the caret's checkbox line. Absent is the empty
    /// selection, which the domain refuses.
    pub text: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub lane: Option<String>,
    pub origin: Option<String>,
}

/// `ticket.set`: an absent field is unchanged, `description: ""` is the clear.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketSetParams {
    pub id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub description: Option<String>,
}

/// `ticket.move`: to the end of the lane when `index` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketMoveParams {
    pub id: Option<String>,
    pub lane: Option<String>,
    pub index: Option<i64>,
}

/// `ticket.block`: the edge's near end and the ticket it waits on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketBlockParams {
    pub id: Option<String>,
    pub on: Option<String>,
}

/// `ticket.unblock`: every edge from the ticket when `on` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketUnblockParams {
    pub id: Option<String>,
    pub on: Option<String>,
}

/// `ticket.answer`: the answer, its status and its next owner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketAnswerParams {
    pub id: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub body: Option<String>,
}

/// `ticket.archive`: one ticket, or the done ones when `id` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketArchiveParams {
    pub id: Option<String>,
}

/// `ticket.comment.add`: the comment's own author, not the write's actor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct CommentAddParams {
    pub id: Option<String>,
    pub author: Option<String>,
    pub body: Option<String>,
}

/// `lane.add`: the name, and the ref key it issues.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneAddParams {
    pub name: Option<String>,
    pub abbr: Option<String>,
}

/// `lane.rename`: from one name to another, optionally re-keying the refs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneRenameParams {
    pub from: Option<String>,
    pub to: Option<String>,
    pub abbr: Option<String>,
}

/// `lane.delete`: the name, and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneDeleteParams {
    pub name: Option<String>,
}

/// `setup.install`: the CLI's `--host` and `--force`, unchanged.
///
/// `host` is one name or a comma-separated list, and absent means the CLI's
/// default (`opencode`). `force` replaces a link that points elsewhere; a real
/// file is never touched, `force` included.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct SetupInstallParams {
    pub host: Option<String>,
    pub force: bool,
}

// MARK: - Responses

/// One answer: the request's id and exactly one of a result or an error.
///
/// A frame the server could not read as a request has no id to echo, so `id`
/// is null there; every other answer carries the client's.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Response {
    pub id: Option<u64>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// A response's two halves. Serialized as `{"result": ...}` or
/// `{"error": ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
// A response is built, serialized and dropped inside one frame's handling; the
// size difference between a whole ticket and an error is a few hundred stack
// bytes, not a box worth allocating per frame.
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    Result(Reply),
    Error(WireError),
}

/// What a method answers. Untagged: the client knows which method it called,
/// so the payload needs no second tag on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum Reply {
    TicketList(TicketListReply),
    UsageReport(UsageReportReply),
    Settings(SettingsReply),
    Ticket(TicketReply),
    Comment(CommentReply),
    CommentList(CommentListReply),
    LaneList(LaneListReply),
    LaneDeletion(LaneDeletionReply),
    Archived(ArchivedReply),
    Setup(SetupReply),
    Doctor(DoctorReply),
    NoteTree(NoteTreeReply),
    NoteRead(NoteReadReply),
    NoteWrite(NoteWriteReply),
    NoteMove(NoteMoveReply),
    NoteHealth(NoteHealthReply),
    NoteBacklinks(NoteBacklinksReply),
    NoteRecoveries(NoteRecoveriesReply),
    Markdown(MarkdownReply),
}

/// `markdown.blocks`: the document's blocks, top level first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownReply {
    pub blocks: Vec<MarkdownBlockView>,
}

/// `note.tree`: the folder and note rows, folders first as the core walks them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteTreeReply {
    pub nodes: Vec<NoteNodeView>,
}

/// `note.read`: the note's text, hash and stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteReadReply {
    pub note: NoteContentView,
}

/// `note.write`: the path written and the hash that landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteWriteReply {
    pub write: NoteWriteView,
}

/// `note.move`: where the note or folder landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteMoveReply {
    pub moved: NoteMoveView,
}

/// `note.health`: the same report `note links` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteHealthReply {
    pub health: BikeshedHealth,
}

/// `note.backlinks`: the notes linking to the path, and line of the first link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteBacklinksReply {
    pub path: String,
    pub backlinks: Vec<BacklinkView>,
}

/// `note.recoveries`: the orphaned temps a killed save left, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteRecoveriesReply {
    pub recoveries: Vec<RecoveryView>,
}

/// `usage.report`: the reported usage, rolled up over every session record and
/// labelled from the board.
///
/// One message, because the face draws every number from it: the partitions, the
/// ticket arithmetic, each row's records and each row's bar width (PRT-15). It
/// carries no revision: the rows are the records and the board as read, the
/// shell's displayed revision is the listing's, and a change event is what makes
/// the face read this again.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageReportReply {
    pub report: UsageReportView,
}

/// `settings.get`: the settings a face reads, and `settings.set`'s receipt.
///
/// The roster is the one setting the app reads (`TandemSettings.roster`): the
/// names the sheet's assignee picker and the board's filter offer. It is the
/// stored list, or the default when none was ever stored; the typing is the
/// core's (`domain::Settings`), so neither face parses a settings row. A
/// setting added later is one more field here; the write is its own method
/// (`settings.set`), because only reads share this reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsReply {
    pub roster: Vec<String>,
}

/// `ticket.list`: the full snapshot and the revision it is a snapshot of.
///
/// The `revision` is authoritative: these tickets are the store at exactly this
/// number, and no earlier. A change event may carry a smaller number (an
/// intermediate commit the client saw before it listed) or a larger one (a write
/// that landed after the list); either way the client compares this number, not
/// the event's, and re-lists on the next event.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TicketListReply {
    pub revision: i64,
    pub tickets: Vec<TicketSummary>,
}

/// A whole ticket: `ticket.show`, and every ticket write's receipt.
///
/// The `revision` is the number the ticket is a snapshot of: the write's own for
/// a receipt, the read transaction's for `ticket.show`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TicketReply {
    pub revision: i64,
    pub ticket: TicketDetail,
}

/// `ticket.comment.add`: the comment, and the revision the write produced.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommentReply {
    pub revision: i64,
    pub comment: CommentView,
}

/// `ticket.comment.list`: every comment, append order, at the revision the log
/// was read in.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommentListReply {
    pub revision: i64,
    pub comments: Vec<CommentView>,
}

/// `lane.list`, `lane.add`, `lane.rename`: every lane, render order.
///
/// The `revision` is the number the lanes are a snapshot of: the write's own for
/// a mutation, the read transaction's for `lane.list`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaneListReply {
    pub revision: i64,
    pub lanes: Vec<String>,
}

/// `lane.delete`: the lanes left, and how many tickets moved to the default.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaneDeletionReply {
    pub revision: i64,
    pub lanes: Vec<String>,
    pub moved: usize,
}

/// `ticket.archive` without an id: how many tickets the sweep archived.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedReply {
    pub revision: i64,
    pub archived: usize,
}

/// One agent host's root, as the Settings surface shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostRoot {
    /// `opencode`, `claude` or `agents`, the wire's host names.
    pub host: String,
    /// The directory the host reads the face from.
    pub path: String,
}

/// `setup.status` and `setup.install`: the report the verb prints, and the
/// roots the Settings surface shows beside it.
///
/// The report is the verb's answer unchanged: `paths` is what the run wrote
/// (empty on a read), and `state` is the face's state after it. `home`,
/// `bikeshed` and `hosts` are the roots that report is about, resolved by the
/// core's own path rule, so a Settings surface names no path of its own. This
/// is not store data, so there is no revision: the answer is the machine at the
/// moment the verb ran.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupReply {
    pub report: crate::setup::Report,
    pub home: String,
    pub bikeshed: String,
    pub hosts: Vec<HostRoot>,
}

/// `doctor.report`: ADR 0014's pinned report, one row per check.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReply {
    pub report: crate::wire::DoctorReport,
}

/// What went wrong, in the words the client shows and the code it branches on.
///
/// `parse` is a frame that is not a request; `method` is a method this build
/// does not answer; `domain` is a refusal the domain made (an unknown id, an
/// empty body); `store` is the store failing (a lock held past the window);
/// `setup` is a setup verb refusing (no bundle, an unknown host).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

impl WireError {
    pub fn parse(message: impl Into<String>) -> Self {
        Self {
            code: "parse".to_string(),
            message: message.into(),
        }
    }

    pub fn method(message: impl Into<String>) -> Self {
        Self {
            code: "method".to_string(),
            message: message.into(),
        }
    }

    pub fn domain(message: impl Into<String>) -> Self {
        Self {
            code: "domain".to_string(),
            message: message.into(),
        }
    }

    pub fn store(message: impl Into<String>) -> Self {
        Self {
            code: "store".to_string(),
            message: message.into(),
        }
    }

    /// The setup verbs refused: a missing bundle, an unknown host, or a
    /// filesystem failure the install could not write through.
    pub fn setup(message: impl Into<String>) -> Self {
        Self {
            code: "setup".to_string(),
            message: message.into(),
        }
    }
}

// MARK: - Events

/// The store moved, and a revision it reached.
///
/// Pushed when any writer commits: this stream's own write, the CLI in a
/// terminal, another process under the lock. The event is a hint, not a state:
/// it can name an intermediate revision (a migration commits on its own) and it
/// can be superseded before the client reads. The client re-lists, and the
/// listing's `revision` is authoritative (`TicketListReply`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum Event {
    Changed { revision: i64 },
}

impl Event {
    pub fn changed(revision: i64) -> Self {
        Self::Changed { revision }
    }
}
