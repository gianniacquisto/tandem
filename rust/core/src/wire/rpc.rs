//! The method set behind the MCP door: the typed calls `tandem serve`'s
//! `POST /mcp` dispatches (ADR 0047).
//!
//! A tool call deserialises its arguments straight into a [`Method`]
//! ([`Method::from_call`]) and [`crate::rpc::execute`] answers it with a
//! [`Reply`] or a [`WireError`]: no frame is built, serialised or re-read on
//! the way. The shapes live here, beside the CLI's views, so every face reads
//! one set of types.
//!
//! Two rules the types encode:
//!
//! - **Every reply that carries a revision is a snapshot at that number.** There
//!   is no paging: a read reply reads its rows and its number in one read
//!   transaction; a write receipt reads the number inside the write's own
//!   transaction. **The reply's revision is authoritative**: the payload is the
//!   store at exactly that number.
//! - **A method is a domain verb, never a row.** The names are the CLI's
//!   grammar (`ticket.add` is `ticket add`), and [`METHODS`] is the table the
//!   parity test walks: every write names a CLI verb, so no UI-only write can
//!   appear without the CLI growing the same act. A read is the one method that
//!   may name no verb, and the table says so with an empty one.

use serde::{Deserialize, Serialize};

use crate::bikeshed::health::BikeshedHealth;
use crate::domain::query::Filter;
use crate::domain::ticket::Status;
use crate::wire::views::{
    BacklinkView, CommentView, NoteContentView, NoteMoveView, NoteNodeView, NotePathView,
    NoteWriteView, TicketDetail, TicketSummary,
};

// MARK: - Methods

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
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            /// The method a tool call names, with its arguments read straight
            /// into the params type: no frame is serialised on the way.
            pub fn from_call(tag: &str, params: serde_json::Value) -> Result<Self, WireError> {
                if !METHODS.iter().any(|(known, _)| *known == tag) {
                    return Err(WireError::method(format!(
                        "the wire does not answer the method `{tag}`"
                    )));
                }
                serde_json::from_value(serde_json::json!({"method": tag, "params": params}))
                    .map_err(|error| {
                        WireError::parse(format!(
                            "the frame's params for `{tag}` are not readable: {error}"
                        ))
                    })
            }

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
    TicketArchive: "ticket.archive", TicketArchiveParams, "ticket archive";
    TicketCommentList: "ticket.comment.list", TicketIdParams, "ticket comment list";
    TicketCommentAdd: "ticket.comment.add", CommentAddParams, "ticket comment add";
    LabelList: "label.list", EmptyParams, "label list";
    LabelAdd: "label.add", LabelAddParams, "label add";
    LabelRename: "label.rename", LabelRenameParams, "label rename";
    LabelDelete: "label.delete", LabelDeleteParams, "label delete";
    LabelMove: "label.move", LabelMoveParams, "label move";
    ProjectList: "project.list", EmptyParams, "project list";
    ProjectAdd: "project.add", ProjectAddParams, "project add";
    ProjectRename: "project.rename", ProjectRenameParams, "project rename";
    ProjectDelete: "project.delete", ProjectDeleteParams, "project delete";
    // The Bikeshed face's reads: the CLI spells no verb for them, and they
    // write nothing, so the parity test's empty-verb list names each.
    NoteTree: "note.tree", EmptyParams, "";
    NoteRead: "note.read", NotePathParams, "";
    NoteHealth: "note.health", EmptyParams, "note links";
    NoteBacklinks: "note.backlinks", NotePathParams, "";
    NoteWrite: "note.write", NoteWriteParams, "note write";
    NoteMove: "note.move", NoteMoveParams, "note move";
    NoteCreate: "note.create", NotePathParams, "note create";
    NoteCreateFolder: "note.createFolder", NotePathParams, "note create-folder";
    DoctorReport: "doctor.report", EmptyParams, "doctor";
}

/// A method that takes nothing. The call still carries `{}` arguments, so a
/// client always sends the same three keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmptyParams {}

/// A method that names one note.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotePathParams {
    pub path: Option<String>,
}

/// `note.write`: the path and its whole text. Both optional so the call's
/// empty-params round-trip still parses; a missing path is refused by the
/// domain, never silently written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteWriteParams {
    pub path: Option<String>,
    pub text: Option<String>,
}

/// `note.move`: the note or folder, and the folder it moves into.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteMoveParams {
    pub path: Option<String>,
    pub into: Option<String>,
}

/// `ticket.list`: the CLI's filters, every one optional.
///
/// `filter` is the shipped three-value scope (`open`, `done`, `all`); the named
/// axes narrow it. Absent fields mean the CLI's absent flags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketListParams {
    pub filter: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub archived: bool,
    pub blocked: bool,
    /// At most this many rows, from the top; the reply's `total` says how many
    /// matched when the list was cut. Absent is the whole list.
    pub limit: Option<i64>,
}

/// A method that names one ticket.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketIdParams {
    pub id: Option<String>,
}

/// `ticket.add`: the same fields the verb's flags carry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketAddParams {
    pub title: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub origin: Option<String>,
    pub description: Option<String>,
}

/// `ticket.set`: an absent field is unchanged, `description: ""` is the clear,
/// and `appendDescription` adds to the body (an empty addition, or both
/// description fields in one call, is refused).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketSetParams {
    pub id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub description: Option<String>,
    pub append_description: Option<String>,
}

/// `ticket.move`: to the end of the label when `index` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketMoveParams {
    pub id: Option<String>,
    pub label: Option<String>,
    pub index: Option<i64>,
}

/// `ticket.block`: the edge's near end and the ticket it waits on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketBlockParams {
    pub id: Option<String>,
    pub on: Option<String>,
}

/// `ticket.unblock`: every edge from the ticket when `on` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketUnblockParams {
    pub id: Option<String>,
    pub on: Option<String>,
}

/// `ticket.archive`: one ticket, or the done ones when `id` is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TicketArchiveParams {
    pub id: Option<String>,
}

/// `ticket.comment.add`: the comment's own author, not the write's actor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CommentAddParams {
    pub id: Option<String>,
    pub author: Option<String>,
    pub body: Option<String>,
}

/// `label.add`: the name, the ref key it issues, and the parent project.
/// An absent project creates or reuses the project named for the label.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelAddParams {
    pub name: Option<String>,
    pub abbr: Option<String>,
    pub project: Option<String>,
}

/// `label.rename`: from one name to another, optionally re-keying the refs and
/// re-parenting the label. An absent project keeps the label's parent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelRenameParams {
    pub from: Option<String>,
    pub to: Option<String>,
    pub abbr: Option<String>,
    pub project: Option<String>,
}

/// `label.delete`: the name, and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelDeleteParams {
    pub name: Option<String>,
}

/// `label.move`: reorder the declared labels. The index is 0-based in the label
/// list and is clamped, so an absent index appends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelMoveParams {
    pub name: Option<String>,
    pub index: Option<i64>,
}

/// `project.add`: the name, and nothing else. Add appends, there is no move.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectAddParams {
    pub name: Option<String>,
}

/// `project.rename`: from one name to another, moving its labels with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectRenameParams {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// `project.delete`: the name, and nothing else. Refused while labels belong.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectDeleteParams {
    pub name: Option<String>,
}

// MARK: - Input schemas

/// One tool input's shape: an object whose properties are the params struct's
/// fields. Every wire param stays optional at parse (an absent field is the
/// CLI's absent flag, and the domain refuses a missing one in its own words),
/// so `required` is advertised only: it lets a host or model catch the gap
/// before the round-trip and never changes what the door accepts. The registry
/// test pins the property set against the struct's serialized fields and the
/// `required` list against the params the domain refuses without.
fn object_schema(properties: &[(&str, serde_json::Value)], required: &[&str]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, schema) in properties {
        map.insert((*name).to_string(), (*schema).clone());
    }
    serde_json::json!({
        "type": "object",
        "properties": serde_json::Value::Object(map),
        "required": required,
    })
}

/// A string field whose value is one of a closed set, so a host can validate
/// it and a model stops guessing.
fn one_of(values: &[&str], description: &str) -> serde_json::Value {
    serde_json::json!({"type": "string", "enum": values, "description": description})
}

/// A string field.
fn string(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "string", "description": description})
}

/// A whole-number field.
fn integer(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "integer", "description": description})
}

/// A true-or-false field.
fn boolean(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "boolean", "description": description})
}

impl TicketListParams {
    /// The tool input for `ticket.list`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                (
                    "filter",
                    one_of(&Filter::WIRE_VALUES, "open (default), done or all"),
                ),
                (
                    "text",
                    string("a case- and accent-insensitive substring of the title or description"),
                ),
                (
                    "status",
                    string("comma-separated values, e.g. ready,in-progress"),
                ),
                (
                    "assignee",
                    string("an exact name, self for your own, or \"\" for the unowned"),
                ),
                (
                    "label",
                    string("a label name, or \"\" for the default area"),
                ),
                (
                    "archived",
                    boolean("alone it is the archive; with a filter, the archive joins it"),
                ),
                (
                    "blocked",
                    boolean("only the tickets waiting on an open blocker"),
                ),
                (
                    "limit",
                    integer(
                        "at most this many rows from the top; total reports the matches when cut; 0 returns no rows and the total",
                    ),
                ),
            ],
            &[],
        )
    }
}

impl TicketIdParams {
    /// The tool input for the one-ticket reads and writes.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[(
                "id",
                string("the ticket's UUID or its ref (the label key and number, TAN-42)"),
            )],
            &["id"],
        )
    }
}

impl NotePathParams {
    /// The tool input for the note reads and creates that name one path.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[("path", string("the bikeshed-root-relative path"))],
            &["path"],
        )
    }
}

impl NoteWriteParams {
    /// The tool input for `note.write`: the path and its whole text.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("path", string("the bikeshed-root-relative path")),
                (
                    "text",
                    string("the note's whole text; a missing text is refused"),
                ),
            ],
            &["path", "text"],
        )
    }
}

impl NoteMoveParams {
    /// The tool input for `note.move`: what moves, and the folder it lands in.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("path", string("the note or folder that moves")),
                ("into", string("the folder it moves into; the root is \"\"")),
            ],
            &["path", "into"],
        )
    }
}

impl TicketAddParams {
    /// The tool input for `ticket.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("title", string("one line, trimmed; required")),
                (
                    "status",
                    one_of(
                        &Status::WIRE_VALUES,
                        "backlog (default), on-hold, ready, in-progress, in-review or done",
                    ),
                ),
                (
                    "assignee",
                    string("a name, self to claim it, or human for the human"),
                ),
                ("label", string("the label; it must exist")),
                (
                    "origin",
                    string("who filed it, self for this agent; written at add, never changed"),
                ),
                (
                    "description",
                    string("the markdown body; no flag is no body"),
                ),
            ],
            &["title"],
        )
    }
}

impl TicketSetParams {
    /// The tool input for `ticket.set`: every field named is changed in one
    /// write, everything unnamed is left alone.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("id", string("the ticket's UUID or its ref")),
                (
                    "title",
                    string("one line, trimmed; an empty title is refused"),
                ),
                (
                    "status",
                    one_of(
                        &Status::WIRE_VALUES,
                        "backlog, on-hold, ready, in-progress, in-review or done",
                    ),
                ),
                (
                    "assignee",
                    string("a name, self, human, or \"\" to clear the owner"),
                ),
                (
                    "description",
                    string("the markdown body, replacing the old one, or \"\" to clear it"),
                ),
                (
                    "appendDescription",
                    string(
                        "text appended to the body after a blank line; the existing body is kept",
                    ),
                ),
            ],
            &["id"],
        )
    }
}

impl TicketMoveParams {
    /// The tool input for `ticket.move`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("id", string("the ticket's UUID or its ref")),
                (
                    "label",
                    string("the label, or \"\" for the default area; it must exist"),
                ),
                ("index", integer("0-based within the label; omit to append")),
            ],
            &["id"],
        )
    }
}

impl TicketBlockParams {
    /// The tool input for `ticket.block`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("id", string("the ticket's UUID or its ref")),
                ("on", string("the ticket it waits on, as a UUID or a ref")),
            ],
            &["id", "on"],
        )
    }
}

impl TicketUnblockParams {
    /// The tool input for `ticket.unblock`: every edge when `on` is absent.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("id", string("the ticket's UUID or its ref")),
                (
                    "on",
                    string("the edge to remove; omit to remove every edge the ticket has"),
                ),
            ],
            &["id"],
        )
    }
}

impl TicketArchiveParams {
    /// The tool input for `ticket.archive`: one ticket, or the done ones when
    /// `id` is absent.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[(
                "id",
                string("the ticket's UUID or its ref; omit to archive every done ticket"),
            )],
            &[],
        )
    }
}

impl CommentAddParams {
    /// The tool input for `ticket.comment.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("id", string("the ticket's UUID or its ref")),
                ("author", string("who signs the line; write the role out")),
                ("body", string("the comment body; an empty body is refused")),
            ],
            &["id", "body"],
        )
    }
}

impl LabelAddParams {
    /// The tool input for `label.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("name", string("the label name; required")),
                (
                    "abbr",
                    string("the ref key it issues; omit for the first three alphanumerics"),
                ),
                (
                    "project",
                    string(
                        "the parent project; omit to create or reuse the project named for the label",
                    ),
                ),
            ],
            &["name"],
        )
    }
}

impl LabelRenameParams {
    /// The tool input for `label.rename`.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("from", string("the label's current name")),
                ("to", string("the new name")),
                (
                    "abbr",
                    string("the new ref key; refused once the key is frozen"),
                ),
                (
                    "project",
                    string("the new parent project; omit to keep the label's project"),
                ),
            ],
            &["from", "to"],
        )
    }
}

impl LabelDeleteParams {
    /// The tool input for `label.delete`: members fall back to the default
    /// area, never deleted.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the label name"))], &["name"])
    }
}

impl LabelMoveParams {
    /// The tool input for `label.move`: reorder the declared labels.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("name", string("the label name")),
                (
                    "index",
                    integer("0-based in the label list; omit to append"),
                ),
            ],
            &["name"],
        )
    }
}

impl ProjectAddParams {
    /// The tool input for `project.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the project name; required"))], &["name"])
    }
}

impl ProjectRenameParams {
    /// The tool input for `project.rename`: labels move with the project.
    pub fn schema() -> serde_json::Value {
        object_schema(
            &[
                ("from", string("the project's current name")),
                ("to", string("the new name")),
            ],
            &["from", "to"],
        )
    }
}

impl ProjectDeleteParams {
    /// The tool input for `project.delete`: refused while labels belong.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the project name"))], &["name"])
    }
}

impl EmptyParams {
    /// The tool input for the methods that take nothing.
    pub fn schema() -> serde_json::Value {
        object_schema(&[], &[])
    }
}

// MARK: - Responses

/// What a method answers. Untagged: the client knows which method it called,
/// so the payload needs no second tag on the wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum Reply {
    TicketList(TicketListReply),
    Ticket(TicketReply),
    Comment(CommentReply),
    CommentList(CommentListReply),
    LabelList(LabelListReply),
    LabelDeletion(LabelDeletionReply),
    ProjectList(ProjectListReply),
    Archived(ArchivedReply),
    NotePath(NotePathView),
    Doctor(DoctorReply),
    NoteTree(NoteTreeReply),
    NoteRead(NoteReadReply),
    NoteWrite(NoteWriteReply),
    NoteMove(NoteMoveReply),
    NoteHealth(NoteHealthReply),
    NoteBacklinks(NoteBacklinksReply),
}

/// `note.tree`: the folder and note rows, folders first as the core walks them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteTreeReply {
    pub nodes: Vec<NoteNodeView>,
}

/// `note.read`: the note's text, hash and stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteReadReply {
    pub note: NoteContentView,
}

/// `note.write`: the path written and the hash that landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteWriteReply {
    pub write: NoteWriteView,
}

/// `note.move`: where the note or folder landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMoveReply {
    pub moved: NoteMoveView,
}

/// `note.health`: the same report `note links` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteHealthReply {
    pub health: BikeshedHealth,
}

/// `note.backlinks`: the notes linking to the path, and line of the first link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteBacklinksReply {
    pub path: String,
    pub backlinks: Vec<BacklinkView>,
}

/// `ticket.list`: the full snapshot and the revision it is a snapshot of.
///
/// The `revision` is authoritative: these tickets are the store at exactly this
/// number, and no earlier. A change event may carry a smaller number (an
/// intermediate commit the client saw before it listed) or a larger one (a write
/// that landed after the list); either way the client compares this number, not
/// the event's, and re-lists on the next event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketListReply {
    pub revision: i64,
    pub tickets: Vec<TicketSummary>,
    /// How many tickets matched, present only when `limit` cut the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

/// A whole ticket: `ticket.show`, and every ticket write's receipt.
///
/// The `revision` is the number the ticket is a snapshot of: the write's own for
/// a receipt, the read transaction's for `ticket.show`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketReply {
    pub revision: i64,
    pub ticket: TicketDetail,
}

/// `ticket.comment.add`: the comment, and the revision the write produced.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentReply {
    pub revision: i64,
    pub comment: CommentView,
}

/// `ticket.comment.list`: every comment, append order, at the revision the log
/// was read in.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentListReply {
    pub revision: i64,
    pub comments: Vec<CommentView>,
}

/// `label.list`, `label.add`, `label.rename`: every label, render order.
///
/// The `revision` is the number the labels are a snapshot of: the write's own for
/// a mutation, the read transaction's for `label.list`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelListReply {
    pub revision: i64,
    pub labels: Vec<String>,
}

/// `label.delete`: the labels left, and how many tickets moved to the default.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelDeletionReply {
    pub revision: i64,
    pub labels: Vec<String>,
    pub moved: usize,
}

/// One project and its labels, in render order: the answer to `project list`
/// and every project write's receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    pub name: String,
    pub labels: Vec<String>,
}

/// `project.list`, `project.add`, `project.rename`, `project.delete`: every
/// project in render order, each with its labels in label order.
///
/// The `revision` is the number the projects are a snapshot of: the write's
/// own for a mutation, the read transaction's for `project.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListReply {
    pub revision: i64,
    pub projects: Vec<ProjectEntry>,
}

/// `ticket.archive` without an id: how many tickets the sweep archived.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedReply {
    pub revision: i64,
    pub archived: usize,
}

/// `doctor.report`: ADR 0014's pinned report, one row per check.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReply {
    pub report: crate::wire::DoctorReport,
}

/// What went wrong, in the words the client shows and the code it branches on.
///
/// `parse` is arguments that do not read as the method's params; `method` is a method this build
/// does not answer; `domain` is a refusal the domain made (an unknown id, an
/// empty body); `store` is the store failing (a lock held past the window).
#[derive(Debug, Clone, PartialEq, Serialize)]
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
