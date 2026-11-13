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
//!   may name no verb, and the table says so with an empty one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bikeshed::health::BikeshedHealth;
use crate::domain::workflow::{Workflow, WorkflowStage};
use crate::wire::views::{
    BacklinkView, CommentView, MarkdownBlockView, NoteContentView, NoteMoveView, NoteNodeView,
    NotePathView, NoteWriteView, RecoveryView, TicketDetail, TicketSummary,
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
    TicketArchive: "ticket.archive", TicketArchiveParams, "ticket archive";
    TicketCommentList: "ticket.comment.list", TicketIdParams, "ticket comment list";
    TicketCommentAdd: "ticket.comment.add", CommentAddParams, "ticket comment add";
    LaneList: "lane.list", EmptyParams, "lane list";
    LaneAdd: "lane.add", LaneAddParams, "lane add";
    LaneRename: "lane.rename", LaneRenameParams, "lane rename";
    LaneDelete: "lane.delete", LaneDeleteParams, "lane delete";
    LaneMove: "lane.move", LaneMoveParams, "lane move";
    ProjectList: "project.list", EmptyParams, "project list";
    ProjectAdd: "project.add", ProjectAddParams, "project add";
    ProjectRename: "project.rename", ProjectRenameParams, "project rename";
    ProjectDelete: "project.delete", ProjectDeleteParams, "project delete";
    // The workflow model (TAN-328): the stage set as data. The verbs are the
    // CLI's own, so the app and an agent configure a workflow through the same
    // acts and the board reads one truth.
    WorkflowList: "workflow.list", EmptyParams, "workflow list";
    WorkflowShow: "workflow.show", WorkflowShowParams, "workflow show";
    WorkflowSet: "workflow.set", WorkflowSetParams, "workflow set";
    WorkflowUse: "workflow.use", WorkflowIdParams, "workflow use";
    WorkflowDelete: "workflow.delete", WorkflowIdParams, "workflow delete";
    NoteCapture: "note.capture", NoteCaptureParams, "note capture";
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
    NoteCreate: "note.create", NotePathParams, "note create";
    NoteCreateFolder: "note.createFolder", NotePathParams, "note create-folder";
    NoteDaily: "note.daily", EmptyParams, "note daily";
    DoctorReport: "doctor.report", EmptyParams, "doctor";
    // The settings the faces read, in one message: the sheet's assignee picker
    // and the board's filter both offer the store's roster, and the board
    // draws the active project beside it. Both are `settings` rows; the read
    // writes nothing, so no UI-only write hides behind its empty verb. The
    // write below names the CLI verb it shares with the Settings face's Save
    // and the focus control.
    SettingsGet: "settings.get", EmptyParams, "";
    // The settings write: the roster's whole-list replace and the
    // active-project focus, each applied only when its key is present, so a
    // roster Save leaves the focus alone and a focus pick leaves the roster
    // alone. The bare frame clears the focus and leaves the roster alone.
    // The CLI spells it `settings set`, so no UI-only write hides
    // here.
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

/// `lane.add`: the name, the ref key it issues, and the parent project.
/// An absent project creates or reuses the project named for the lane.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneAddParams {
    pub name: Option<String>,
    pub abbr: Option<String>,
    pub project: Option<String>,
}

/// `lane.rename`: from one name to another, optionally re-keying the refs and
/// re-parenting the lane. An absent project keeps the lane's parent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneRenameParams {
    pub from: Option<String>,
    pub to: Option<String>,
    pub abbr: Option<String>,
    pub project: Option<String>,
}

/// `lane.delete`: the name, and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneDeleteParams {
    pub name: Option<String>,
}

/// `lane.move`: reorder the declared lanes. The index is 0-based in the lane
/// list and is clamped, so an absent index appends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LaneMoveParams {
    pub name: Option<String>,
    pub index: Option<i64>,
}

/// `project.add`: the name, and nothing else. Add appends, there is no move.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectAddParams {
    pub name: Option<String>,
}

/// `project.rename`: from one name to another, moving its lanes with it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectRenameParams {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// `project.delete`: the name, and nothing else. Refused while lanes belong.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectDeleteParams {
    pub name: Option<String>,
}

/// `workflow.show`: the workflow to read. Absent reads the active one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkflowShowParams {
    pub id: Option<String>,
}

/// `workflow.use` and `workflow.delete`: the workflow a handle names, its UUID
/// or its name. Absent is refused by the domain, never guessed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkflowIdParams {
    pub id: Option<String>,
}

/// `workflow.set`: the whole document. An absent id creates a workflow; a
/// present one updates the workflow it names. `name` and `stages` are required
/// by the domain and refused when missing, so the empty frame cannot wipe a
/// workflow.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkflowSetParams {
    pub id: Option<String>,
    pub name: Option<String>,
    pub scope: Option<String>,
    pub stages: Vec<WorkflowStage>,
}

/// `note.capture`: one raw memory note. The body rides a JSON string, as the
/// stdin bodies ride the CLI; an empty one is the domain's own refusal. An
/// absent project is the working directory's last component and an absent
/// session is `$OPENCODE_SESSION_ID`, as the verb defaults them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteCaptureParams {
    pub kind: Option<String>,
    pub project: Option<String>,
    pub session: Option<String>,
    pub body: Option<String>,
}

// MARK: - Input schemas

/// One tool input's shape: an object whose properties are the params struct's
/// fields. Every wire param is optional at parse (an absent field is the CLI's
/// absent flag), so `required` is always empty; the registry test pins the
/// property set against the struct's serialized fields, so a field added to
/// one without the other fails the build.
fn object_schema(properties: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, schema) in properties {
        map.insert((*name).to_string(), (*schema).clone());
    }
    serde_json::json!({
        "type": "object",
        "properties": serde_json::Value::Object(map),
        "required": [],
    })
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
        object_schema(&[
            ("filter", string("open (default), done or all")),
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
                "lane",
                string("a cycle lane name, or \"\" for the default area"),
            ),
            (
                "archived",
                boolean("alone it is the archive; with a filter, the archive joins it"),
            ),
            (
                "blocked",
                boolean("only the tickets waiting on an open blocker"),
            ),
        ])
    }
}

impl TicketIdParams {
    /// The tool input for the one-ticket reads and writes.
    pub fn schema() -> serde_json::Value {
        object_schema(&[(
            "id",
            string("the ticket's UUID or its ref (the lane key and number, TAN-42)"),
        )])
    }
}

impl TicketAddParams {
    /// The tool input for `ticket.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("title", string("one line, trimmed; required")),
            (
                "status",
                string(
                    "backlog (default), on-hold, selected, ready, in-progress, in-review or done",
                ),
            ),
            (
                "assignee",
                string("a name, self to claim it, or human for the human"),
            ),
            ("lane", string("the cycle lane; it must exist")),
            (
                "origin",
                string("who filed it, self for this agent; written at add, never changed"),
            ),
            (
                "description",
                string("the markdown body; no flag is no body"),
            ),
        ])
    }
}

impl TicketAddFromNoteParams {
    /// The tool input for `ticket.file`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            (
                "note",
                string("the bikeshed-root-relative note the text came from"),
            ),
            ("text", string("the selection to file as a ticket")),
            (
                "status",
                string(
                    "backlog (default), on-hold, selected, ready, in-progress, in-review or done",
                ),
            ),
            (
                "assignee",
                string("a name, self to claim it, or human for the human"),
            ),
            ("lane", string("the cycle lane; it must exist")),
            (
                "origin",
                string("who filed it, self for this agent; written at add, never changed"),
            ),
        ])
    }
}

impl TicketSetParams {
    /// The tool input for `ticket.set`: every field named is changed in one
    /// write, everything unnamed is left alone.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the ticket's UUID or its ref")),
            (
                "title",
                string("one line, trimmed; an empty title is refused"),
            ),
            (
                "status",
                string("backlog, on-hold, selected, ready, in-progress, in-review or done"),
            ),
            (
                "assignee",
                string("a name, self, human, or \"\" to clear the owner"),
            ),
            (
                "description",
                string("the markdown body, or \"\" to clear it"),
            ),
        ])
    }
}

impl TicketMoveParams {
    /// The tool input for `ticket.move`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the ticket's UUID or its ref")),
            (
                "lane",
                string("the cycle lane, or \"\" for the default area; it must exist"),
            ),
            ("index", integer("0-based within the lane; omit to append")),
        ])
    }
}

impl TicketBlockParams {
    /// The tool input for `ticket.block`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the ticket's UUID or its ref")),
            ("on", string("the ticket it waits on, as a UUID or a ref")),
        ])
    }
}

impl TicketUnblockParams {
    /// The tool input for `ticket.unblock`: every edge when `on` is absent.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the ticket's UUID or its ref")),
            (
                "on",
                string("the edge to remove; omit to remove every edge the ticket has"),
            ),
        ])
    }
}

impl TicketArchiveParams {
    /// The tool input for `ticket.archive`: one ticket, or the done ones when
    /// `id` is absent.
    pub fn schema() -> serde_json::Value {
        object_schema(&[(
            "id",
            string("the ticket's UUID or its ref; omit to archive every done ticket"),
        )])
    }
}

impl CommentAddParams {
    /// The tool input for `ticket.comment.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the ticket's UUID or its ref")),
            ("author", string("who signs the line; write the role out")),
            ("body", string("the comment body; an empty body is refused")),
        ])
    }
}

impl LaneAddParams {
    /// The tool input for `lane.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("name", string("the lane name; required")),
            (
                "abbr",
                string("the ref key it issues; omit for the first three alphanumerics"),
            ),
            (
                "project",
                string(
                    "the parent project; omit to create or reuse the project named for the lane",
                ),
            ),
        ])
    }
}

impl LaneRenameParams {
    /// The tool input for `lane.rename`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("from", string("the lane's current name")),
            ("to", string("the new name")),
            (
                "abbr",
                string("the new ref key; refused once the key is frozen"),
            ),
            (
                "project",
                string("the new parent project; omit to keep the lane's project"),
            ),
        ])
    }
}

impl LaneDeleteParams {
    /// The tool input for `lane.delete`: members fall back to the default
    /// area, never deleted.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the lane name"))])
    }
}

impl LaneMoveParams {
    /// The tool input for `lane.move`: reorder the declared lanes.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("name", string("the lane name")),
            ("index", integer("0-based in the lane list; omit to append")),
        ])
    }
}

impl ProjectAddParams {
    /// The tool input for `project.add`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the project name; required"))])
    }
}

impl ProjectRenameParams {
    /// The tool input for `project.rename`: lanes move with the project.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("from", string("the project's current name")),
            ("to", string("the new name")),
        ])
    }
}

impl ProjectDeleteParams {
    /// The tool input for `project.delete`: refused while lanes belong.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("name", string("the project name"))])
    }
}

impl EmptyParams {
    /// The tool input for the methods that take nothing.
    pub fn schema() -> serde_json::Value {
        object_schema(&[])
    }
}

impl NoteCaptureParams {
    /// The tool input for `note.capture`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("kind", string("decision, gotcha, fact or thread; required")),
            (
                "project",
                string("the project; omit for the working directory's last component"),
            ),
            (
                "session",
                string("the session folder; omit for $OPENCODE_SESSION_ID"),
            ),
            ("body", string("the capture body; an empty body is refused")),
        ])
    }
}

impl SettingsSetParams {
    /// The tool input for `settings.set`: each axis applies only when its key
    /// is present, so one write spells one axis.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            (
                "roster",
                string("the whole roster replacement; omit to leave the stored list alone"),
            ),
            (
                "project",
                string(
                    "the lane to focus on, or \"\" to clear back to every lane; omit to leave the focus alone",
                ),
            ),
        ])
    }
}

impl WorkflowShowParams {
    /// The tool input for `workflow.show`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[(
            "id",
            string("the workflow's UUID or its name; omit for the active workflow"),
        )])
    }
}

impl WorkflowIdParams {
    /// The tool input for `workflow.use` and `workflow.delete`.
    pub fn schema() -> serde_json::Value {
        object_schema(&[("id", string("the workflow's UUID or its name; required"))])
    }
}

impl WorkflowSetParams {
    /// The tool input for `workflow.set`: the whole document. The id names the
    /// workflow to update; omit it to create one.
    pub fn schema() -> serde_json::Value {
        object_schema(&[
            ("id", string("the workflow to update; omit to create one")),
            ("name", string("the workflow's name; required")),
            (
                "scope",
                string("home (the only scope today); omit for home"),
            ),
            (
                "stages",
                serde_json::json!({
                    "type": "array",
                    "description": "the ordered stage list; at least one intake, one parked and one finished",
                    "items": {
                        "type": "object",
                        "properties": {
                            "key": {"type": "string", "description": "the stable status key"},
                            "label": {"type": "string", "description": "the word every face draws"},
                            "role": {"type": "string", "description": "intake, parked, ready, active, review or finished"},
                            "binding": {
                                "type": "object",
                                "description": "the roster role and pass a stage hands to"
                            }
                        }
                    }
                }),
            ),
        ])
    }
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
    Settings(SettingsReply),
    Ticket(TicketReply),
    Comment(CommentReply),
    CommentList(CommentListReply),
    LaneList(LaneListReply),
    LaneDeletion(LaneDeletionReply),
    ProjectList(ProjectListReply),
    WorkflowList(WorkflowListReply),
    Workflow(WorkflowReply),
    Archived(ArchivedReply),
    NotePath(NotePathView),
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

/// `settings.get`: the settings a face reads, and `settings.set`'s receipt.
///
/// The roster is the one setting the app reads (`TandemSettings.roster`): the
/// names the sheet's assignee picker and the board's filter offer. It is the
/// stored list, or the default when none was ever stored; the typing is the
/// core's (`domain::Settings`), so neither face parses a settings row. A
/// setting added later is one more field here, never a second method: the
/// active project answers beside the roster, resolved against the declared
/// lanes, so a stale focus reads as no focus; the write is its own method
/// (`settings.set`), because only reads share this reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsReply {
    pub roster: Vec<String>,
    pub active_project: Option<String>,
}

/// `settings.set`: the roster's whole replacement and the focus to store, each
/// applied only when its key is present, so one write spells one axis.
///
/// `roster`, when present, replaces the stored list whole: absent is not the
/// empty list but "leave the stored list alone", and an explicitly emptied
/// list is the human's removal rather than a value no build can read.
/// `project`, when present, focuses the faces on one declared lane; a blank
/// value clears back to every lane, and a name no declared lane carries is
/// refused with nothing written. An absent project leaves the focus alone,
/// unless the frame spells no roster either: the bare frame is the legacy
/// bare write, which clears the focus and leaves the roster alone (an
/// explicit null reads as absent on the wire, so clearing the focus never
/// empties the roster). Optional throughout so the frame's
/// empty-params round-trip still parses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsSetParams {
    pub roster: Option<Vec<String>>,
    pub project: Option<String>,
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

/// One project and its lanes, in render order: the answer to `project list`
/// and every project write's receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    pub name: String,
    pub lanes: Vec<String>,
}

/// `project.list`, `project.add`, `project.rename`, `project.delete`: every
/// project in render order, each with its lanes in lane order.
///
/// The `revision` is the number the projects are a snapshot of: the write's
/// own for a mutation, the read transaction's for `project.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListReply {
    pub revision: i64,
    pub projects: Vec<ProjectEntry>,
}

/// `workflow.list`, and `workflow.delete`'s receipt: every workflow and the
/// active workflow's id.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListReply {
    pub revision: i64,
    pub workflows: Vec<Workflow>,
    pub active: String,
}

/// `workflow.show`, `workflow.set` and `workflow.use`: one whole workflow.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowReply {
    pub revision: i64,
    pub workflow: Workflow,
}

/// `ticket.archive` without an id: how many tickets the sweep archived.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedReply {
    pub revision: i64,
    pub archived: usize,
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
