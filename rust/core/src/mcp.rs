//! The agent face as an MCP server, carried by `tandem serve`'s `POST /mcp`
//! (ADR 0047, which retired the stdio `tandem mcp` child).
//!
//! The streamable HTTP transport over [`crate::rpc::execute`]: JSON responses
//! only and stateless (no `Mcp-Session-Id`, no SSE).
//! [`handle_mcp`] is the whole protocol minus the transport: one frame in, one
//! frame out, or silence for a notification (which `serve` answers `202`).
//! [`crate::rpc::MAX_FRAME`] bounds a frame. The five-method surface
//! (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`,
//! `ping`) is hand-rolled on `serde_json`: five methods are smaller than a
//! dependency.
//!
//! Dispatch is one typed call: a `tools/call` reads its arguments into a
//! [`Method`] and hands it to [`crate::rpc::execute`], then renders the reply
//! as the tool result. One frame in, one frame out; batches are refused.
//!
//! The protocol version is negotiated, not assumed. The pinned host (opencode
//! v2.2.5) speaks `2024-11-05`, `2025-03-26`,
//! `2025-06-18`, `2025-11-25` and `2026-07-28` (probed from the strings of the
//! installed binary); this door negotiates the four classic versions and
//! answers at most `2025-11-25`, echoing the client's version when it is one
//! of them. `2026-07-28` is the stateful protocol the new handshake carries,
//! which a stateless JSON door does not speak. The HTTP transport checks the
//! `MCP-Protocol-Version` header against [`PROTOCOL_VERSIONS`].
//!
//! The door logs through `tracing` events, and the CLI installs a subscriber
//! that writes them under `TANDEM_LOG` (ADR 0046 amendment).
//! The default filter is `warn`, so a healthy session is silent. A line names
//! identifiers and outcomes only: never arguments, results, ticket or note
//! text, or the environment, and every value a client chose (a tool or method
//! name) is Debug-escaped so a control character cannot break the line.

use std::sync::OnceLock;
use std::time::Instant;

use serde_json::{Value, json};

use crate::env::Environment;
use crate::rpc::execute;
use crate::store::{Actor, Store};
use crate::wire::rpc::Reply;
use crate::wire::rpc::{
    CommentAddParams, EmptyParams, LabelAddParams, LabelDeleteParams, LabelMoveParams,
    LabelRenameParams, Method, NoteMoveParams, NotePathParams, NoteWriteParams, ProjectAddParams,
    ProjectDeleteParams, ProjectRenameParams, TicketAddParams, TicketArchiveParams,
    TicketBlockParams, TicketIdParams, TicketListParams, TicketMoveParams, TicketSetParams,
    TicketUnblockParams,
};

/// The server name the `initialize` answer carries.
pub const SERVER_NAME: &str = "tandem-mcp";

/// What a verb is on the wire: the tool that does the same act, and how its
/// result becomes the stdout the local verb prints. The one table behind the
/// remote CLI (ADR 0047): the verb comes from [`crate::wire::rpc::METHODS`],
/// the tool from [`tools`], so no second table can drift from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVerb {
    /// The tool's name, as `tools/call` carries it.
    pub tool: &'static str,
    /// The reply key whose value is the local verb's whole stdout, when the
    /// local verb prints one field of the reply rather than the reply.
    unwrap: Option<&'static str>,
}

/// The tool that answers a CLI verb (`"ticket add"`), or `None` for a verb the
/// wire has no method for (`serve`, `note rename`, `note stats`).
pub fn remote_verb(verb: &str) -> Option<RemoteVerb> {
    let (tag, _) = crate::wire::rpc::METHODS
        .iter()
        .find(|(_, spelled)| !spelled.is_empty() && *spelled == verb)?;
    let tool = tools().iter().find(|tool| tool.method_tag == *tag)?;
    let unwrap = match *tag {
        "ticket.show" | "ticket.add" | "ticket.set" | "ticket.move" | "ticket.block"
        | "ticket.unblock" | "ticket.complete" => Some("ticket"),
        "ticket.comment.add" => Some("comment"),
        "note.write" => Some("write"),
        "note.move" => Some("moved"),
        "note.health" => Some("health"),
        "doctor.report" => Some("report"),
        _ => None,
    };
    Some(RemoteVerb {
        tool: tool.name,
        unwrap,
    })
}

impl RemoteVerb {
    /// The local verb's stdout, from the tool's result: the reply without its
    /// revision (the local verb prints none), or the one field it prints whole.
    /// `ticket archive` answers a ticket for one id and `{archived}` for all,
    /// so the ticket is unwrapped when the reply carries one.
    pub fn stdout(&self, result: Value) -> Value {
        let Value::Object(mut reply) = result else {
            return result;
        };
        reply.remove("revision");
        if let Some(key) = self.unwrap
            && let Some(inner) = reply.remove(key)
        {
            return inner;
        }
        if self.tool == "ticket_archive"
            && let Some(inner) = reply.remove("ticket")
        {
            return inner;
        }
        Value::Object(reply)
    }
}

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
    _environment: &Environment,
) -> Option<String> {
    let value: Value = match serde_json::from_str(frame) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(reason = %error, "malformed frame");
            return Some(error_frame(frame_id(frame), PARSE_ERROR, error.to_string()));
        }
    };
    if value.is_array() {
        tracing::warn!("invalid request: a batch");
        return Some(error_frame(
            None,
            INVALID_REQUEST,
            "JSON-RPC batches are not supported",
        ));
    }
    let request = match value.as_object() {
        Some(request) => request,
        None => {
            tracing::warn!("invalid request: the frame is not an object");
            return Some(error_frame(
                None,
                INVALID_REQUEST,
                "the frame is not a JSON-RPC 2.0 request",
            ));
        }
    };
    if request.get("jsonrpc") != Some(&Value::String("2.0".to_string())) {
        tracing::warn!("invalid request: the frame is not JSON-RPC 2.0");
        return Some(error_frame(
            id_of(&value),
            INVALID_REQUEST,
            "the frame is not a JSON-RPC 2.0 request",
        ));
    }
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let id = id_of(&value);
    match method {
        "initialize" => Some(initialize_frame(id, request.get("params"), store, actor)),
        "notifications/initialized" => {
            tracing::debug!("notifications/initialized");
            let id = id?;
            Some(result_frame(Some(id), json!({})))
        }
        "ping" => {
            tracing::debug!("ping");
            let id = id?;
            Some(result_frame(Some(id), json!({})))
        }
        "tools/list" => {
            tracing::debug!("tools/list");
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
                _environment,
            ))
        }
        "" => {
            tracing::warn!("invalid request: the frame names no method");
            Some(error_frame(
                id,
                INVALID_REQUEST,
                "the frame names no method",
            ))
        }
        unknown => {
            let id = id?;
            tracing::warn!(method = ?unknown, "method not found");
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
    actor: &Actor,
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
    tracing::info!(
        protocol = negotiated,
        version = %server_version(),
        actor = ?actor.as_str(),
        "initialize"
    );
    result_frame(
        id,
        json!({
            "protocolVersion": negotiated,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": server_version()},
            "health": health_block(store),
        }),
    )
}

/// The release version the `initialize` answer carries: the stamped build
/// version when the release stamped one, else the crate version. The workspace
/// is set in `Cargo.toml`; the stamp wins when present, and the crate version
/// is the last resort. A dev build answers the crate version and no more (ADR
/// 0045 retired the bundled `VERSION` marker).
pub fn server_version() -> String {
    let version = crate::release_version().trim();
    if version.is_empty() {
        env!("CARGO_PKG_VERSION").to_string()
    } else {
        version.to_string()
    }
}

/// A small health block for `initialize`: the resolved home and why, whether
/// the store is ok, the release version and the tool count. It reads the store's lock-free health only, never the full `doctor`
/// report, so every call stays cheap.
fn health_block(store: &Store) -> Value {
    let home = store.home();
    let health = store.health();
    let store_ok = matches!(
        health,
        crate::store::Health::Absent | crate::store::Health::Counts { .. }
    );
    let mut block = json!({
        "home": crate::wire::slashed_path(home.directory()),
        "homeSource": home.source().as_str(),
        "storeOk": store_ok,
        "version": server_version(),
        "tools": tools().len(),
    });
    // A newer store is the one refusal an agent host can act on, so the block
    // says why in the store's own words.
    if let crate::store::Health::Newer { store, build } = health {
        block["storeDetail"] = json!(crate::store::StoreError::Newer { store, build }.to_string());
    }
    block
}

/// One `tools/call`: the named tool's arguments become a typed [`Method`], the
/// `rpc` dispatch answers it, and the reply becomes the tool result. A domain
/// refusal is a result with `isError`, in the domain's words; only a call the
/// door cannot route is a JSON-RPC error.
///
/// The server inherits its spawn environment; the call reads nothing else.
fn call_frame(
    id: Option<Value>,
    params: Option<&Value>,
    store: &Store,
    actor: &Actor,
    _environment: &Environment,
) -> String {
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let Some(tool) = tools().iter().find(|tool| tool.name == name) else {
        tracing::warn!(tool = ?name, "unknown tool");
        return error_frame(id, INVALID_PARAMS, format!("Unknown tool: {name}"));
    };
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        tracing::warn!(tool = tool.name, "invalid arguments: not an object");
        return error_frame(
            id,
            INVALID_PARAMS,
            format!("Invalid arguments for tool: {name}"),
        );
    }
    let detail = match Detail::from_arguments(tool, &arguments) {
        Ok(detail) => detail,
        Err(message) => {
            tracing::warn!(tool = tool.name, "invalid arguments: detail");
            return error_frame(id, INVALID_PARAMS, message);
        }
    };
    // A receipt names the title only when the write could have changed it.
    let title_changed = tool.method_tag == "ticket.add"
        || (tool.method_tag == "ticket.set" && arguments.get("title").is_some());
    let started = Instant::now();
    let outcome = Method::from_call(tool.method_tag, arguments)
        .and_then(|method| execute(method, store, actor));
    let latency_ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(reply) => {
            tracing::info!(tool = tool.name, outcome = "ok", latency_ms, "tools/call");
            let result = render(tool.method_tag, &reply, detail, title_changed);
            result_frame(
                id,
                json!({"content": [{"type": "text", "text": result.to_string()}]}),
            )
        }
        Err(error) => {
            // The domain's error kind only, never its message: a message may
            // quote ticket text.
            tracing::info!(tool = tool.name, outcome = "error", kind = ?error.code, latency_ms, "tools/call");
            result_frame(
                id,
                json!({"content": [{"type": "text", "text": error.message}], "isError": true}),
            )
        }
    }
}

// MARK: - Lean and full answers

/// How much of a reply the caller asked for. A call that names nothing gets
/// [`Detail::Full`], the answer the door has always given; `"lean"` is opt-in.
/// The remote CLI still names `"full"`, so its stdout never depends on the
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Detail {
    Lean,
    Full,
}

impl Detail {
    /// The `detail` a call names, for a tool whose schema carries one; a tool
    /// without it ignores the argument and answers as it always did.
    fn from_arguments(tool: &Tool, arguments: &Value) -> Result<Self, String> {
        if tool.input_schema["properties"].get("detail").is_none() {
            return Ok(Self::Full);
        }
        match arguments.get("detail") {
            None | Some(Value::Null) => Ok(Self::Full),
            Some(Value::String(word)) if word == "lean" => Ok(Self::Lean),
            Some(Value::String(word)) if word == "full" => Ok(Self::Full),
            Some(_) => Err(format!(
                "Invalid arguments for tool: {}: detail is \"lean\" or \"full\"",
                tool.name
            )),
        }
    }
}

/// The `detail` property a lean-capable tool's schema adds to its params.
fn with_detail(mut schema: Value) -> Value {
    schema["properties"]["detail"] = one_of_detail();
    schema
}

fn one_of_detail() -> Value {
    json!({
        "type": "string",
        "enum": ["lean", "full"],
        "description": "full (default) returns the whole ticket, every field; lean omits what you already hold and empty fields",
    })
}

/// The reply as the tool result. [`Detail::Full`] is the reply as the wire
/// types serialize it, through a `Value` so the keys come out in the order the
/// door has always answered them. [`Detail::Lean`] is derived from that same
/// value, so the lean shape has one mapping and no copy of a field list beside
/// the wire types' own.
fn render(tag: &str, reply: &Reply, detail: Detail, title_changed: bool) -> Value {
    let full = serde_json::to_value(reply).unwrap_or(Value::Null);
    if detail == Detail::Full {
        return full;
    }
    match reply {
        Reply::Ticket(_) if tag == "ticket.show" => map_key(full, "ticket", lean_ticket),
        Reply::Ticket(_) => lean_receipt(&full, title_changed),
        Reply::TicketList(_) => map_key(full, "tickets", |rows| match rows {
            Value::Array(rows) => Value::Array(rows.into_iter().map(lean_ticket).collect()),
            other => other,
        }),
        Reply::Comment(_) => {
            let comment = &full["comment"];
            json!({"revision": full["revision"], "at": comment["at"], "author": comment["author"]})
        }
        _ => full,
    }
}

/// `value` with the entry under `key` replaced by `f` of it.
fn map_key(mut value: Value, key: &str, f: impl FnOnce(Value) -> Value) -> Value {
    if let Some(slot) = value.get_mut(key) {
        let taken = slot.take();
        *slot = f(taken);
    }
    value
}

/// A write's receipt: the revision and enough of the ticket to see where it
/// stands. The description is never echoed, and the title only when the write
/// could have changed it.
fn lean_receipt(full: &Value, title_changed: bool) -> Value {
    let ticket = &full["ticket"];
    let mut kept = serde_json::Map::new();
    for key in ["ref", "id", "status", "assignee", "label", "blocked"] {
        kept.insert(key.to_string(), ticket[key].clone());
    }
    if title_changed {
        kept.insert("title".to_string(), ticket["title"].clone());
    }
    json!({"revision": full["revision"], "ticket": Value::Object(kept)})
}

/// A ticket or a list row without its defaults: a null, a false and an empty
/// array are dropped, `done` (the status says it) goes, the `id` stays only
/// for a ticket with no ref, and a blocker is its ref (or id) and status.
fn lean_ticket(ticket: Value) -> Value {
    let Value::Object(fields) = ticket else {
        return ticket;
    };
    let has_ref = fields.get("ref").is_some_and(|value| !value.is_null());
    let mut kept = serde_json::Map::new();
    for (key, value) in fields {
        let default = match &value {
            Value::Null | Value::Bool(false) => true,
            Value::Array(items) => items.is_empty(),
            _ => false,
        };
        if default || key == "done" || (key == "id" && has_ref) {
            continue;
        }
        let value = if key == "blockedBy" || key == "blocks" {
            lean_blockers(value)
        } else {
            value
        };
        kept.insert(key, value);
    }
    Value::Object(kept)
}

/// A blocker edge's far end: `{ref, status}`, or the id for a ticket with no ref.
fn lean_blockers(blockers: Value) -> Value {
    let Value::Array(blockers) = blockers else {
        return blockers;
    };
    Value::Array(
        blockers
            .into_iter()
            .map(|blocker| {
                let name = match blocker["ref"].as_str() {
                    Some(reference) => ("ref", json!(reference)),
                    None => ("id", blocker["id"].clone()),
                };
                json!({name.0: name.1, "status": blocker["status"]})
            })
            .collect(),
    )
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
    /// The tool's name: the method tag in snake case.
    pub name: &'static str,
    /// The wire method the call feeds to the `rpc` door.
    pub method_tag: &'static str,
    /// What the tool does, in the words the agent reads.
    pub description: &'static str,
    /// The input shape, derived from the params struct (never a hand copy).
    pub input_schema: Value,
    /// What a host may assume about the call without asking the human.
    pub annotations: Annotations,
    /// The params the schema derives from, so the registry test can pin one
    /// against the other.
    pub method: Method,
}

/// The MCP tool annotations (`readOnlyHint`, `destructiveHint`,
/// `idempotentHint`, `openWorldHint`: defined since `2025-03-26`, unchanged in
/// `2025-11-25`; a `2024-11-05` client ignores the unknown key). They are hints
/// a host may use to skip or force a prompt, so a wrong `read_only` on a
/// writing tool is an approval bypass: the registry test pins every flag.
///
/// The spec defaults `destructiveHint` to true and `readOnlyHint` to false, so
/// every flag is written out on every tool and a later tool cannot inherit a
/// default by omission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Annotations {
    /// The call changes nothing in the store or the vault.
    pub read_only: bool,
    /// The call can lose what was there: a delete, an overwrite, a bulk act.
    pub destructive: bool,
    /// Calling again with the same arguments changes nothing more.
    pub idempotent: bool,
}

impl Annotations {
    /// A read: no write at all.
    const READ: Self = Self {
        read_only: true,
        destructive: false,
        idempotent: true,
    };
    /// A write that adds or edits and loses nothing it cannot rebuild.
    const WRITE: Self = Self {
        read_only: false,
        destructive: false,
        idempotent: false,
    };
    /// A write that can lose what was there.
    const DESTRUCTIVE: Self = Self {
        read_only: false,
        destructive: true,
        idempotent: false,
    };
    /// The same write, safe to repeat.
    const fn idempotent(self) -> Self {
        Self {
            idempotent: true,
            ..self
        }
    }

    /// The wire object.
    fn wire(&self) -> Value {
        json!({
            "readOnlyHint": self.read_only,
            "destructiveHint": self.destructive,
            "idempotentHint": self.idempotent,
            "openWorldHint": false,
        })
    }
}

impl Tool {
    /// The `tools/list` row.
    fn wire(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": self.annotations.wire(),
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

/// The tools: one per covered wire method. The schema beside each entry is
/// the params struct's own derivation, so the list cannot drift from the wire
/// without the registry test failing.
///
/// Built once: a `tools/call` looks its tool up by name in the same list
/// `tools/list` serves.
pub fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(build_tools)
}

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ticket_list",
            method_tag: "ticket.list",
            description: "List the tickets in view, in stored order. Every filter is optional and they combine. Rows carry every field; detail \"lean\" omits null, false and empty fields.",
            input_schema: with_detail(TicketListParams::schema()),
            annotations: Annotations::READ,
            method: Method::TicketList(TicketListParams::default()),
        },
        Tool {
            name: "ticket_show",
            method_tag: "ticket.show",
            description: "Read one ticket whole: the only read that carries the description. Answers every field; detail \"lean\" omits null, false and empty fields.",
            input_schema: with_detail(TicketIdParams::schema()),
            annotations: Annotations::READ,
            method: Method::TicketShow(TicketIdParams::default()),
        },
        Tool {
            name: "ticket_add",
            method_tag: "ticket.add",
            description: "File a ticket. A title alone lands backlog, unowned, on top of the default area. Answers the whole ticket; detail \"lean\" answers a short receipt without the description.",
            input_schema: with_detail(TicketAddParams::schema()),
            annotations: Annotations::WRITE,
            method: Method::TicketAdd(TicketAddParams::default()),
        },
        Tool {
            name: "ticket_set",
            method_tag: "ticket.set",
            description: "Change the fields named in one write; everything unnamed is left alone. To add to the description send appendDescription (the body is kept); description replaces the whole body, and the two cannot be sent together. Answers the whole ticket; detail \"lean\" answers a short receipt without the description.",
            input_schema: with_detail(TicketSetParams::schema()),
            annotations: Annotations::WRITE.idempotent(),
            method: Method::TicketSet(TicketSetParams::default()),
        },
        Tool {
            name: "ticket_move",
            method_tag: "ticket.move",
            description: "Move between labels and/or within one. Omit the index to append. Answers the whole ticket; detail \"lean\" answers a short receipt.",
            input_schema: with_detail(TicketMoveParams::schema()),
            annotations: Annotations::WRITE,
            method: Method::TicketMove(TicketMoveParams::default()),
        },
        Tool {
            name: "ticket_block",
            method_tag: "ticket.block",
            description: "Record that one ticket waits on another, as an edge in the store. Answers the whole ticket; detail \"lean\" answers a short receipt.",
            input_schema: with_detail(TicketBlockParams::schema()),
            annotations: Annotations::WRITE.idempotent(),
            method: Method::TicketBlock(TicketBlockParams::default()),
        },
        Tool {
            name: "ticket_unblock",
            method_tag: "ticket.unblock",
            description: "Remove one edge, or every edge the ticket has when `on` is absent. Answers the whole ticket; detail \"lean\" answers a short receipt.",
            input_schema: with_detail(TicketUnblockParams::schema()),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::TicketUnblock(TicketUnblockParams::default()),
        },
        Tool {
            name: "ticket_complete",
            method_tag: "ticket.complete",
            description: "The short path to done: idempotent, no reopen verb. Answers the whole ticket; detail \"lean\" answers a short receipt.",
            input_schema: with_detail(TicketIdParams::schema()),
            annotations: Annotations::WRITE.idempotent(),
            method: Method::TicketComplete(TicketIdParams::default()),
        },
        Tool {
            name: "ticket_archive",
            method_tag: "ticket.archive",
            description: "Archive one ticket, or every done ticket when `id` is absent. One ticket answers the whole ticket; detail \"lean\" answers a short receipt.",
            input_schema: with_detail(TicketArchiveParams::schema()),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::TicketArchive(TicketArchiveParams::default()),
        },
        Tool {
            name: "ticket_comment_add",
            method_tag: "ticket.comment.add",
            description: "Append to the ticket's discussion log. Write the role out as the author. Echoes the comment; detail \"lean\" answers the time and author, not the body.",
            input_schema: with_detail(CommentAddParams::schema()),
            annotations: Annotations::WRITE,
            method: Method::TicketCommentAdd(CommentAddParams::default()),
        },
        Tool {
            name: "ticket_comment_list",
            method_tag: "ticket.comment.list",
            description: "List the ticket's discussion log in append order.",
            input_schema: TicketIdParams::schema(),
            annotations: Annotations::READ,
            method: Method::TicketCommentList(TicketIdParams::default()),
        },
        Tool {
            name: "label_list",
            method_tag: "label.list",
            description: "List every label, in render order.",
            input_schema: EmptyParams::schema(),
            annotations: Annotations::READ,
            method: Method::LabelList(EmptyParams {}),
        },
        Tool {
            name: "label_add",
            method_tag: "label.add",
            description: "Declare a label. A label must exist before tickets move to it.",
            input_schema: LabelAddParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::LabelAdd(LabelAddParams::default()),
        },
        Tool {
            name: "label_rename",
            method_tag: "label.rename",
            description: "Rename a label, rewriting every ticket in it in one write.",
            input_schema: LabelRenameParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::LabelRename(LabelRenameParams::default()),
        },
        Tool {
            name: "label_delete",
            method_tag: "label.delete",
            description: "Delete a label; members fall back to the default area, never deleted.",
            input_schema: LabelDeleteParams::schema(),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::LabelDelete(LabelDeleteParams::default()),
        },
        Tool {
            name: "label_move",
            method_tag: "label.move",
            description: "Reorder the declared labels; the index is 0-based and clamps.",
            input_schema: LabelMoveParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::LabelMove(LabelMoveParams::default()),
        },
        Tool {
            name: "project_list",
            method_tag: "project.list",
            description: "List every project with its labels, in render order.",
            input_schema: EmptyParams::schema(),
            annotations: Annotations::READ,
            method: Method::ProjectList(EmptyParams {}),
        },
        Tool {
            name: "project_add",
            method_tag: "project.add",
            description: "Declare a project. A project must exist before labels name it as parent.",
            input_schema: ProjectAddParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::ProjectAdd(ProjectAddParams::default()),
        },
        Tool {
            name: "project_rename",
            method_tag: "project.rename",
            description: "Rename a project, moving its labels with it.",
            input_schema: ProjectRenameParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::ProjectRename(ProjectRenameParams::default()),
        },
        Tool {
            name: "project_delete",
            method_tag: "project.delete",
            description: "Delete an empty project; refused while labels belong to it.",
            input_schema: ProjectDeleteParams::schema(),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::ProjectDelete(ProjectDeleteParams::default()),
        },
        Tool {
            name: "doctor_report",
            method_tag: "doctor.report",
            description: "Report where this machine's Tandem state lives and whether it is healthy.",
            input_schema: EmptyParams::schema(),
            annotations: Annotations::READ,
            method: Method::DoctorReport(EmptyParams {}),
        },
        Tool {
            name: "note_tree",
            method_tag: "note.tree",
            description: "Read the vault's folder tree: folders first, notes after, hidden entries skipped.",
            input_schema: EmptyParams::schema(),
            annotations: Annotations::READ,
            method: Method::NoteTree(EmptyParams {}),
        },
        Tool {
            name: "note_read",
            method_tag: "note.read",
            description: "Read one note: its whole text, the content hash a save compares and its mtime.",
            input_schema: NotePathParams::schema(),
            annotations: Annotations::READ,
            method: Method::NoteRead(NotePathParams::default()),
        },
        Tool {
            name: "note_write",
            method_tag: "note.write",
            description: "Write a note's whole text, atomically, under the vault lock. A missing text is refused.",
            input_schema: NoteWriteParams::schema(),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::NoteWrite(NoteWriteParams::default()),
        },
        Tool {
            name: "note_create",
            method_tag: "note.create",
            description: "Create an empty note. A taken path is refused, never overwritten.",
            input_schema: NotePathParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::NoteCreate(NotePathParams::default()),
        },
        Tool {
            name: "note_create_folder",
            method_tag: "note.createFolder",
            description: "Create a folder, with no note in it.",
            input_schema: NotePathParams::schema(),
            annotations: Annotations::WRITE,
            method: Method::NoteCreateFolder(NotePathParams::default()),
        },
        Tool {
            name: "note_move",
            method_tag: "note.move",
            description: "Move a note or folder into another folder, rewriting every wikilink that resolves to it.",
            input_schema: NoteMoveParams::schema(),
            annotations: Annotations::DESTRUCTIVE,
            method: Method::NoteMove(NoteMoveParams::default()),
        },
        Tool {
            name: "note_health",
            method_tag: "note.health",
            description: "Check the vault's link graph: broken links, orphans and counts, the same report note links prints.",
            input_schema: EmptyParams::schema(),
            annotations: Annotations::READ,
            method: Method::NoteHealth(EmptyParams {}),
        },
    ]
}
