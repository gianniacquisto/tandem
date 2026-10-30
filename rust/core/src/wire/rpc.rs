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
//! - **Every listing is a full snapshot plus a revision.** There is no paging:
//!   the human's board is cheap to send whole, and a revision makes the copy
//!   correct. **The listing's revision is authoritative**: the tickets are the
//!   store at exactly that number. An event is a hint to re-read, never a state
//!   to patch in: it can name an intermediate revision (a migration is a commit
//!   of its own) and it can be superseded before the client lists.
//! - **A method is a domain verb, never a row.** The names are the CLI's
//!   grammar (`ticket.add` is `ticket add`), and [`METHODS`] is the table the
//!   parity test walks: every message names a CLI verb, so no UI-only write can
//!   appear without the CLI growing the same act.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::wire::views::{CommentView, TicketDetail, TicketSummary};

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
}

/// A method that takes nothing. The frame still carries `"params": {}`, so a
/// client always sends the same three keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct EmptyParams {}

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
    Ticket(TicketReply),
    Comment(CommentReply),
    CommentList(CommentListReply),
    LaneList(LaneListReply),
    LaneDeletion(LaneDeletionReply),
    Archived(ArchivedReply),
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

/// `ticket.comment.list`: every comment, append order.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommentListReply {
    pub revision: i64,
    pub comments: Vec<CommentView>,
}

/// `lane.list`, `lane.add`, `lane.rename`: every lane, render order.
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

/// What went wrong, in the words the client shows and the code it branches on.
///
/// `parse` is a frame that is not a request; `method` is a method this build
/// does not answer; `domain` is a refusal the domain made (an unknown id, an
/// empty body); `store` is the store failing (a lock held past the window).
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
