//! `tandem mcp`: the agent face as an MCP server over stdio (TAN-274).
//!
//! The same door as [`crate::rpc`], one transport over: newline-delimited
//! JSON-RPC 2.0 on stdio, the same lifecycle (stdin closes, the process ends),
//! [`crate::rpc::MAX_FRAME`] and the stderr-never-carries-a-frame rule carried
//! over. The five-method surface (`initialize`, `notifications/initialized`,
//! `tools/list`, `tools/call`, `ping`) is hand-rolled on `serde_json`, the way
//! `rpc` hand-rolls its frames: five methods are smaller than a dependency.
//!
//! Dispatch shares the `rpc` door by construction: a `tools/call` builds the
//! `(id, method, params)` frame and feeds
//! [`crate::rpc::handle_in`](crate::rpc::handle_in), then renders the answer
//! as the tool result. Strictly sequential, like `rpc`: one frame in, one
//! frame out, no interleaving.
//!
//! The protocol version is negotiated, not assumed. The pinned host (opencode
//! v2.2.5, `docs/port.md`'s harness) speaks `2024-11-05`, `2025-03-26`,
//! `2025-06-18`, `2025-11-25` and `2026-07-28` (probed from the strings of the
//! installed binary); this door negotiates the four classic versions and
//! answers at most `2025-11-25`, echoing the client's version when it is one
//! of them. `2026-07-28` is the stateful protocol the new handshake carries,
//! which a line-delimited stdio door does not speak.

use std::io;

use serde_json::{Value, json};

use crate::rpc::{Stream, handle_in};
use crate::setup::Environment;
use crate::store::{Actor, Store};
use crate::wire::rpc::{
    CommentAddParams, EmptyParams, LaneAddParams, LaneDeleteParams, LaneMoveParams,
    LaneRenameParams, Method, NoteCaptureParams, ProjectAddParams, ProjectDeleteParams,
    ProjectRenameParams, SessionEndParams, SessionLinkParams, SessionListParams, SessionShowParams,
    SessionStartParams, SettingsSetParams, SprintCloseParams, SprintListParams, SprintOpenParams,
    SprintRunParams, SprintShowParams, SprintTicketParams, TicketAddFromNoteParams,
    TicketAddParams, TicketArchiveParams, TicketBlockParams, TicketIdParams, TicketListParams,
    TicketMoveParams, TicketSetParams, TicketUnblockParams, WorkflowIdParams,
    WorkflowRunCloseParams, WorkflowRunListParams, WorkflowRunOpenParams, WorkflowRunRunParams,
    WorkflowRunShowParams, WorkflowRunTicketParams, WorkflowSetParams, WorkflowShowParams,
};

/// The server name the `initialize` answer carries.
pub const SERVER_NAME: &str = "tandem-mcp";

/// The protocol versions this door negotiates, oldest first. Probed from the
/// pinned host's own strings (see the module docs); the newest doubles as the
/// answer when the client names a version this build does not speak.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

/// The newest version this door speaks: the negotiated answer for a client
/// whose version is newer than the door.
pub const NEGOTIATED_DEFAULT: &str = "2025-11-25";

/// JSON-RPC 2.0 error codes, the host's SDK's own numbers.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

// MARK: - The process

/// Serve MCP frames from `stream` until it ends, strictly sequentially.
///
/// End of stream is success: a closed stdin is the host going away, which is
/// the normal way this child dies. An I/O failure on the stream is the only
/// error; everything a frame can get wrong is answered as a frame, and a
/// notification is answered with nothing at all.
pub fn serve<S: Stream>(
    stream: S,
    store: Store,
    actor: Actor,
    environment: Environment,
) -> io::Result<()> {
    loop {
        match stream.read_frame() {
            Ok(None) => break Ok(()),
            Ok(Some(frame)) => {
                if let Some(answer) = handle_mcp(&frame, &store, &actor, &environment) {
                    stream.write_frame(&answer)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                let answer = error_frame(None, PARSE_ERROR, error.to_string());
                stream.write_frame(&answer)?;
            }
            Err(error) => break Err(error),
        }
    }
}

// MARK: - One frame

/// Answer one MCP frame: a response frame, or `None` for a notification,
/// which JSON-RPC answers with silence.
///
/// This is the whole protocol minus the transport, so the suite drives it
/// without a process and the pipe test drives it through one.
pub fn handle_mcp(
    frame: &str,
    store: &Store,
    actor: &Actor,
    environment: &Environment,
) -> Option<String> {
    let value: Value = match serde_json::from_str(frame) {
        Ok(value) => value,
        Err(error) => return Some(error_frame(frame_id(frame), PARSE_ERROR, error.to_string())),
    };
    if value.is_array() {
        return Some(error_frame(
            None,
            INVALID_REQUEST,
            "JSON-RPC batches are not supported",
        ));
    }
    let request = match value.as_object() {
        Some(request) => request,
        None => {
            return Some(error_frame(
                None,
                INVALID_REQUEST,
                "the frame is not a JSON-RPC 2.0 request",
            ));
        }
    };
    if request.get("jsonrpc") != Some(&Value::String("2.0".to_string())) {
        return Some(error_frame(
            id_of(&value),
            INVALID_REQUEST,
            "the frame is not a JSON-RPC 2.0 request",
        ));
    }
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let id = id_of(&value);
    match method {
        "initialize" => Some(initialize_frame(
            id,
            request.get("params"),
            store,
            environment,
        )),
        "notifications/initialized" => {
            let id = id?;
            Some(result_frame(Some(id), json!({})))
        }
        "ping" => {
            let id = id?;
            Some(result_frame(Some(id), json!({})))
        }
        "tools/list" => {
            let id = id?;
            let tools: Vec<Value> = tools().iter().map(Tool::wire).collect();
            Some(result_frame(Some(id), json!({"tools": tools})))
        }
        "tools/call" => {
            let id = id?;
            Some(call_frame(
                Some(id),
                request.get("params"),
                store,
                actor,
                environment,
            ))
        }
        "" => Some(error_frame(
            id,
            INVALID_REQUEST,
            "the frame names no method",
        )),
        unknown => {
            let id = id?;
            Some(error_frame(
                Some(id),
                METHOD_NOT_FOUND,
                format!("Method not found: {unknown}"),
            ))
        }
    }
}

/// The `initialize` answer: the negotiated protocol version, no more than the
/// door speaks and no less than the client named when the door speaks it, with
/// the tools capability and the server's own name and version.
///
/// A small `health` block rides beside them (the resolved home and why, the
/// store's ok, the version and the tool count), so a fresh session can see the
/// default path is wired without running a full `doctor`.
fn initialize_frame(
    id: Option<Value>,
    params: Option<&Value>,
    store: &Store,
    environment: &crate::setup::Environment,
) -> String {
    let asked = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let negotiated = if PROTOCOL_VERSIONS.contains(&asked) {
        asked
    } else {
        NEGOTIATED_DEFAULT
    };
    result_frame(
        id,
        json!({
            "protocolVersion": negotiated,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": server_version(store, environment)},
            "health": health_block(store, environment),
        }),
    )
}

/// The release version the `initialize` answer carries: the stamped build
/// version when the release stamped one, else the agent face `VERSION` marker
/// beside the binary (or the installed copy), else the crate version. The
/// workspace sits at 0.1.0 and never moves with releases, so the binary's own
/// version is the last resort, never the answer on an installed app.
pub fn server_version(store: &Store, environment: &crate::setup::Environment) -> String {
    if let Some(stamped) = option_env!("TANDEM_VERSION")
        .map(str::trim)
        .filter(|version| !version.is_empty())
    {
        return stamped.to_string();
    }
    if let Some(bundle) = crate::setup::bundled_face(environment)
        && let Some(version) = crate::setup::version_text(&bundle)
    {
        return version;
    }
    let face = crate::setup::Context::new(store.home().directory(), environment).face();
    if let Some(version) = crate::setup::version_text(&face) {
        return version;
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// A small health block for `initialize` and `session_start`: the resolved
/// home and why, whether the store is ok, the release version and the tool
/// count. It reads the store's lock-free health only, never the full `doctor`
/// report, so every call stays cheap.
fn health_block(store: &Store, environment: &crate::setup::Environment) -> Value {
    let home = store.home();
    let store_ok = matches!(
        store.health(),
        crate::store::Health::Absent | crate::store::Health::Counts { .. }
    );
    json!({
        "home": crate::wire::slashed_path(home.directory()),
        "homeSource": home.source().as_str(),
        "storeOk": store_ok,
        "version": server_version(store, environment),
        "tools": tools().len(),
    })
}

/// One `tools/call`: the named tool becomes the `(id, method, params)` frame
/// the `rpc` door answers, and the answer becomes the tool result. A domain
/// refusal is a result with `isError`, in the domain's words; only a call the
/// door cannot route is a JSON-RPC error.
///
/// The call carries its harness session beside `arguments`: the host sends
/// `_meta: {"ai.opencode/sessionID": ...}` on every call its session makes,
/// and the call runs with that pointer as its `$OPENCODE_SESSION_ID`, so the
/// session verbs resolve the caller the way the CLI does. A long-lived server
/// inherits only its spawn environment, which names no session; the call's
/// pointer wins over that environment when both name one, because the caller
/// is fresher than the spawn.
///
/// `session_start` carries the same small health block `initialize` does, so
/// the pass that opened the record sees the wired path without a `doctor`.
fn call_frame(
    id: Option<Value>,
    params: Option<&Value>,
    store: &Store,
    actor: &Actor,
    environment: &Environment,
) -> String {
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let Some(tool) = tools().into_iter().find(|tool| tool.name == name) else {
        return error_frame(id, INVALID_PARAMS, format!("Unknown tool: {name}"));
    };
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return error_frame(
            id,
            INVALID_PARAMS,
            format!("Invalid arguments for tool: {name}"),
        );
    }
    let frame = json!({"id": 1, "method": tool.method_tag, "params": arguments}).to_string();
    let overridden;
    let environment = match call_harness_session(params) {
        Some(id) => {
            overridden = {
                let mut owned = environment.clone();
                owned.insert("OPENCODE_SESSION_ID".to_string(), id);
                owned
            };
            &overridden
        }
        None => environment,
    };
    let answer: Value = serde_json::from_str(&handle_in(&frame, store, actor, environment))
        .unwrap_or_else(|_| json!({"id": 1, "error": {"code": "parse", "message": "the answer is not readable"}}));
    if let Some(error) = answer.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the call failed");
        return result_frame(
            id,
            json!({"content": [{"type": "text", "text": message}], "isError": true}),
        );
    }
    let mut result = answer.get("result").cloned().unwrap_or(Value::Null);
    if tool.method_tag == "session.start"
        && let Some(object) = result.as_object_mut()
    {
        object.insert("health".to_string(), health_block(store, environment));
    }
    result_frame(
        id,
        json!({"content": [{"type": "text", "text": result.to_string()}]}),
    )
}

/// The harness session the caller runs in, as the host reports it: opencode
/// sends `_meta: {"ai.opencode/sessionID": ...}` on every `tools/call` its
/// session makes. An absent, non-string or blank pointer is no pointer, so the
/// call runs on the server's own environment exactly as before.
fn call_harness_session(params: Option<&Value>) -> Option<String> {
    params
        .and_then(|params| params.get("_meta"))
        .and_then(|meta| meta.get("ai.opencode/sessionID"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// A success frame.
fn result_frame(id: Option<Value>, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

/// An error frame.
fn error_frame(id: Option<Value>, code: i64, message: impl Into<String>) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
        .to_string()
}

/// The frame's id when it carries one: a JSON-RPC id is a string or a number,
/// and a frame that failed to parse may still name it.
fn frame_id(frame: &str) -> Option<Value> {
    serde_json::from_str::<Value>(frame)
        .ok()
        .and_then(|value| id_of(&value))
}

/// The id a parsed frame carries, when it carries one.
fn id_of(value: &Value) -> Option<Value> {
    match value.get("id") {
        Some(Value::String(_)) | Some(Value::Number(_)) => value.get("id").cloned(),
        _ => None,
    }
}

// MARK: - The registry

/// One tool: the covered method it calls, under its snake-case name, with the
/// documentation the agent reads and the schema the params struct derives.
pub struct Tool {
    /// The tool's name: the method tag in snake case, except `ticket_file`,
    /// which is the CLI verb the method shares.
    pub name: &'static str,
    /// The wire method the call feeds to the `rpc` door.
    pub method_tag: &'static str,
    /// What the tool does, in the words the agent reads.
    pub description: &'static str,
    /// The input shape, derived from the params struct (never a hand copy).
    pub input_schema: Value,
    /// The params the schema derives from, so the registry test can pin one
    /// against the other.
    pub method: Method,
}

impl Tool {
    /// The `tools/list` row.
    fn wire(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }

    /// The params the schema derives from, as JSON, so the registry test pins
    /// the schema's property set against the struct's field set.
    pub fn default_params(&self) -> Value {
        serde_json::to_value(&self.method)
            .ok()
            .and_then(|value| value.get("params").cloned())
            .unwrap_or(Value::Null)
    }
}

/// The 48 tools: one per covered wire method. The schema beside each entry is
/// the params struct's own derivation, so the list cannot drift from the wire
/// without the registry test failing.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ticket_list",
            method_tag: "ticket.list",
            description: "List the tickets in view, in stored order. Every filter is optional and they combine.",
            input_schema: TicketListParams::schema(),
            method: Method::TicketList(TicketListParams::default()),
        },
        Tool {
            name: "ticket_show",
            method_tag: "ticket.show",
            description: "Read one ticket whole: the only read that carries the description.",
            input_schema: TicketIdParams::schema(),
            method: Method::TicketShow(TicketIdParams::default()),
        },
        Tool {
            name: "ticket_add",
            method_tag: "ticket.add",
            description: "File a ticket. A title alone lands backlog, unowned, on top of the default area.",
            input_schema: TicketAddParams::schema(),
            method: Method::TicketAdd(TicketAddParams::default()),
        },
        Tool {
            name: "ticket_file",
            method_tag: "ticket.addFromNote",
            description: "File a note's selection as a ticket, linked to the note it came from.",
            input_schema: TicketAddFromNoteParams::schema(),
            method: Method::TicketAddFromNote(TicketAddFromNoteParams::default()),
        },
        Tool {
            name: "ticket_set",
            method_tag: "ticket.set",
            description: "Change the fields named in one write; everything unnamed is left alone.",
            input_schema: TicketSetParams::schema(),
            method: Method::TicketSet(TicketSetParams::default()),
        },
        Tool {
            name: "ticket_move",
            method_tag: "ticket.move",
            description: "Move between lanes and/or within one. Omit the index to append.",
            input_schema: TicketMoveParams::schema(),
            method: Method::TicketMove(TicketMoveParams::default()),
        },
        Tool {
            name: "ticket_block",
            method_tag: "ticket.block",
            description: "Record that one ticket waits on another, as an edge in the store.",
            input_schema: TicketBlockParams::schema(),
            method: Method::TicketBlock(TicketBlockParams::default()),
        },
        Tool {
            name: "ticket_unblock",
            method_tag: "ticket.unblock",
            description: "Remove one edge, or every edge the ticket has when `on` is absent.",
            input_schema: TicketUnblockParams::schema(),
            method: Method::TicketUnblock(TicketUnblockParams::default()),
        },
        Tool {
            name: "ticket_complete",
            method_tag: "ticket.complete",
            description: "The short path to done: idempotent, no reopen verb.",
            input_schema: TicketIdParams::schema(),
            method: Method::TicketComplete(TicketIdParams::default()),
        },
        Tool {
            name: "ticket_archive",
            method_tag: "ticket.archive",
            description: "Archive one ticket, or every done ticket when `id` is absent.",
            input_schema: TicketArchiveParams::schema(),
            method: Method::TicketArchive(TicketArchiveParams::default()),
        },
        Tool {
            name: "ticket_comment_add",
            method_tag: "ticket.comment.add",
            description: "Append to the ticket's discussion log. Write the role out as the author.",
            input_schema: CommentAddParams::schema(),
            method: Method::TicketCommentAdd(CommentAddParams::default()),
        },
        Tool {
            name: "ticket_comment_list",
            method_tag: "ticket.comment.list",
            description: "List the ticket's discussion log in append order.",
            input_schema: TicketIdParams::schema(),
            method: Method::TicketCommentList(TicketIdParams::default()),
        },
        Tool {
            name: "lane_list",
            method_tag: "lane.list",
            description: "List every cycle lane, in render order.",
            input_schema: EmptyParams::schema(),
            method: Method::LaneList(EmptyParams {}),
        },
        Tool {
            name: "lane_add",
            method_tag: "lane.add",
            description: "Declare a cycle lane. A lane must exist before tickets move to it.",
            input_schema: LaneAddParams::schema(),
            method: Method::LaneAdd(LaneAddParams::default()),
        },
        Tool {
            name: "lane_rename",
            method_tag: "lane.rename",
            description: "Rename a lane, rewriting every ticket in it in one write.",
            input_schema: LaneRenameParams::schema(),
            method: Method::LaneRename(LaneRenameParams::default()),
        },
        Tool {
            name: "lane_delete",
            method_tag: "lane.delete",
            description: "Delete a lane; members fall back to the default area, never deleted.",
            input_schema: LaneDeleteParams::schema(),
            method: Method::LaneDelete(LaneDeleteParams::default()),
        },
        Tool {
            name: "lane_move",
            method_tag: "lane.move",
            description: "Reorder the declared lanes; the index is 0-based and clamps.",
            input_schema: LaneMoveParams::schema(),
            method: Method::LaneMove(LaneMoveParams::default()),
        },
        Tool {
            name: "project_list",
            method_tag: "project.list",
            description: "List every project with its lanes, in render order.",
            input_schema: EmptyParams::schema(),
            method: Method::ProjectList(EmptyParams {}),
        },
        Tool {
            name: "project_add",
            method_tag: "project.add",
            description: "Declare a project. A project must exist before lanes name it as parent.",
            input_schema: ProjectAddParams::schema(),
            method: Method::ProjectAdd(ProjectAddParams::default()),
        },
        Tool {
            name: "project_rename",
            method_tag: "project.rename",
            description: "Rename a project, moving its lanes with it.",
            input_schema: ProjectRenameParams::schema(),
            method: Method::ProjectRename(ProjectRenameParams::default()),
        },
        Tool {
            name: "project_delete",
            method_tag: "project.delete",
            description: "Delete an empty project; refused while lanes belong to it.",
            input_schema: ProjectDeleteParams::schema(),
            method: Method::ProjectDelete(ProjectDeleteParams::default()),
        },
        Tool {
            name: "session_start",
            method_tag: "session.start",
            description: "Open the session record for this pass: one record per pass, never a ticket.",
            input_schema: SessionStartParams::schema(),
            method: Method::SessionStart(SessionStartParams::default()),
        },
        Tool {
            name: "session_link",
            method_tag: "session.link",
            description: "Record that this session carries a ticket, in link order.",
            input_schema: SessionLinkParams::schema(),
            method: Method::SessionLink(SessionLinkParams::default()),
        },
        Tool {
            name: "session_end",
            method_tag: "session.end",
            description: "Stamp the session ended, once, with the outcome and the harness usage. Reads the harness row by default; a harness with no row stamps without usage.",
            input_schema: SessionEndParams::schema(),
            method: Method::SessionEnd(SessionEndParams::default()),
        },
        Tool {
            name: "session_list",
            method_tag: "session.list",
            description: "List the session records, newest first.",
            input_schema: SessionListParams::schema(),
            method: Method::SessionList(SessionListParams::default()),
        },
        Tool {
            name: "session_show",
            method_tag: "session.show",
            description: "Read one session record, with the note paths it filed.",
            input_schema: SessionShowParams::schema(),
            method: Method::SessionShow(SessionShowParams::default()),
        },
        Tool {
            name: "sprint_open",
            method_tag: "sprint.open",
            description: "Open the lane's sprint record, once per lane; a second open keeps the first.",
            input_schema: SprintOpenParams::schema(),
            method: Method::SprintOpen(SprintOpenParams::default()),
        },
        Tool {
            name: "sprint_ticket",
            method_tag: "sprint.ticket",
            description: "Draw one ticket into the lane's open sprint batch, in stored order.",
            input_schema: SprintTicketParams::schema(),
            method: Method::SprintTicket(SprintTicketParams::default()),
        },
        Tool {
            name: "sprint_run",
            method_tag: "sprint.run",
            description: "Append one dated entry to the lane's open sprint run log.",
            input_schema: SprintRunParams::schema(),
            method: Method::SprintRun(SprintRunParams::default()),
        },
        Tool {
            name: "sprint_close",
            method_tag: "sprint.close",
            description: "Close the lane's sprint record, once, with what landed.",
            input_schema: SprintCloseParams::schema(),
            method: Method::SprintClose(SprintCloseParams::default()),
        },
        Tool {
            name: "sprint_list",
            method_tag: "sprint.list",
            description: "List the lane's sprint records, newest opened first.",
            input_schema: SprintListParams::schema(),
            method: Method::SprintList(SprintListParams::default()),
        },
        Tool {
            name: "sprint_show",
            method_tag: "sprint.show",
            description: "Read the lane's open sprint record.",
            input_schema: SprintShowParams::schema(),
            method: Method::SprintShow(SprintShowParams::default()),
        },
        Tool {
            name: "workflow_run_open",
            method_tag: "workflow.run.open",
            description: "Open the lane's workflow run, once per lane; a second open keeps the first.",
            input_schema: WorkflowRunOpenParams::schema(),
            method: Method::WorkflowRunOpen(WorkflowRunOpenParams::default()),
        },
        Tool {
            name: "workflow_run_ticket",
            method_tag: "workflow.run.ticket",
            description: "Draw one ticket into the lane's open workflow run, in stored order.",
            input_schema: WorkflowRunTicketParams::schema(),
            method: Method::WorkflowRunTicket(WorkflowRunTicketParams::default()),
        },
        Tool {
            name: "workflow_run_run",
            method_tag: "workflow.run.run",
            description: "Append one dated entry to the lane's open workflow run log.",
            input_schema: WorkflowRunRunParams::schema(),
            method: Method::WorkflowRunRun(WorkflowRunRunParams::default()),
        },
        Tool {
            name: "workflow_run_close",
            method_tag: "workflow.run.close",
            description: "Close the lane's workflow run, once, with what landed.",
            input_schema: WorkflowRunCloseParams::schema(),
            method: Method::WorkflowRunClose(WorkflowRunCloseParams::default()),
        },
        Tool {
            name: "workflow_run_list",
            method_tag: "workflow.run.list",
            description: "List the workflow's runs, newest opened first.",
            input_schema: WorkflowRunListParams::schema(),
            method: Method::WorkflowRunList(WorkflowRunListParams::default()),
        },
        Tool {
            name: "workflow_run_show",
            method_tag: "workflow.run.show",
            description: "Read the lane's open workflow run, or the run number named.",
            input_schema: WorkflowRunShowParams::schema(),
            method: Method::WorkflowRunShow(WorkflowRunShowParams::default()),
        },
        Tool {
            name: "note_capture",
            method_tag: "note.capture",
            description: "File one capture: a decision, gotcha, fact or thread the next session would otherwise re-derive.",
            input_schema: NoteCaptureParams::schema(),
            method: Method::NoteCapture(NoteCaptureParams::default()),
        },
        Tool {
            name: "doctor_report",
            method_tag: "doctor.report",
            description: "Report where this machine's Tandem state lives and whether it is healthy.",
            input_schema: EmptyParams::schema(),
            method: Method::DoctorReport(EmptyParams {}),
        },
        Tool {
            name: "settings_get",
            method_tag: "settings.get",
            description: "Read the faces' settings: the roster the pickers offer and the active project focus.",
            input_schema: EmptyParams::schema(),
            method: Method::SettingsGet(EmptyParams {}),
        },
        Tool {
            name: "settings_set",
            method_tag: "settings.set",
            description: "Focus the faces on a declared lane, or clear back to every lane; optionally replace the roster.",
            input_schema: SettingsSetParams::schema(),
            method: Method::SettingsSet(SettingsSetParams::default()),
        },
        Tool {
            name: "note_health",
            method_tag: "note.health",
            description: "Check the vault's link graph: broken links, orphans and counts, the same report note links prints.",
            input_schema: EmptyParams::schema(),
            method: Method::NoteHealth(EmptyParams {}),
        },
        Tool {
            name: "workflow_list",
            method_tag: "workflow.list",
            description: "List every workflow and the active one's id.",
            input_schema: EmptyParams::schema(),
            method: Method::WorkflowList(EmptyParams {}),
        },
        Tool {
            name: "workflow_show",
            method_tag: "workflow.show",
            description: "Read one workflow whole. Omit the id for the active workflow.",
            input_schema: WorkflowShowParams::schema(),
            method: Method::WorkflowShow(WorkflowShowParams::default()),
        },
        Tool {
            name: "workflow_set",
            method_tag: "workflow.set",
            description: "Write a workflow's whole document atomically. Omit the id to create one.",
            input_schema: WorkflowSetParams::schema(),
            method: Method::WorkflowSet(WorkflowSetParams::default()),
        },
        Tool {
            name: "workflow_use",
            method_tag: "workflow.use",
            description: "Activate a workflow. Refused while a sprint is open or tickets sit in stages it lacks.",
            input_schema: WorkflowIdParams::schema(),
            method: Method::WorkflowUse(WorkflowIdParams::default()),
        },
        Tool {
            name: "workflow_delete",
            method_tag: "workflow.delete",
            description: "Delete a non-default, inactive workflow; refused for the default or the active one.",
            input_schema: WorkflowIdParams::schema(),
            method: Method::WorkflowDelete(WorkflowIdParams::default()),
        },
    ]
}
