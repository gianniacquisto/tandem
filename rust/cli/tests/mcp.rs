//! The mcp mode over a real pipe: the agent host's door, driven the way the
//! host drives it (TAN-274).
//!
//! Every case spawns the built binary with piped stdin and stdout and speaks
//! newline-delimited JSON-RPC 2.0. The child is a separate process, so these
//! are the claims a unit test over an in-memory stream cannot make:
//!
//! - the stdio handshake negotiates, lists the 43 tools, and drives the
//!   ticket loop (file, read, set, complete) with no shell call;
//! - an unknown method, a batch and a malformed frame are answered as frames
//!   and leave the child answering;
//! - a closed stdin ends the child and leaves no lock or temp behind.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway home: `root/home` is the Tandem home, `root/user` the sandbox
/// user home, so no run resolves `~/.tandem`.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("tandem-mcp-{name}-{}-{serial}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// One running child and its two pipes.
struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Session {
    fn start(sandbox: &Sandbox) -> Self {
        let home = sandbox.home();
        let mut child = Command::new(BINARY)
            .arg("mcp")
            .arg("--home")
            .arg(&home)
            .env_clear()
            .env("HOME", sandbox.root.join("user"))
            .env("TANDEM_HOME", &home)
            .env("TANDEM_AGENT", "builder")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the binary starts");
        let input = child.stdin.take().expect("stdin is piped");
        let output = BufReader::new(child.stdout.take().expect("stdout is piped"));
        Self {
            child,
            input: Some(input),
            output,
        }
    }

    fn send(&mut self, frame: Value) {
        self.send_raw(&frame.to_string());
    }

    fn send_raw(&mut self, line: &str) {
        let input = self.input.as_mut().expect("stdin is still open");
        writeln!(input, "{line}").expect("the frame is writable");
        input.flush().expect("the frame reaches the child");
    }

    fn receive(&mut self) -> Value {
        let mut line = String::new();
        let read = self.output.read_line(&mut line).expect("the stream reads");
        assert!(
            read > 0,
            "the child closed the stream while a frame was expected"
        );
        serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!("the child wrote a frame that is JSON ({error}): {line}")
        })
    }

    fn close_stdin(&mut self) {
        self.input.take();
    }

    /// Wait for the child to exit, killing it if it does not: a child that
    /// outlives a closed stdin is the bug this file exists to catch.
    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().expect("the child is waitable") {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("the child did not exit after its stdin closed");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn call(id: i64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
}

fn tool_text(answer: &Value) -> String {
    assert!(
        answer["error"].is_null(),
        "the call answers a result: {answer}"
    );
    assert!(
        answer["result"]["isError"].is_null(),
        "the call succeeds: {answer}"
    );
    answer["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("the result carries text: {answer}"))
        .to_string()
}

#[test]
fn the_handshake_tools_and_ticket_loop_need_no_shell() {
    let sandbox = Sandbox::new("loop");
    let mut session = Session::start(&sandbox);

    session.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                        "params": {"protocolVersion": "2025-11-25"}}));
    let hello = session.receive();
    assert_eq!(hello["id"], 1);
    assert_eq!(hello["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(hello["result"]["serverInfo"]["name"], "tandem-mcp");

    session.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    session.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}));
    let listed = session.receive();
    assert_eq!(listed["id"], 2);
    let tools = listed["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 43, "the door covers every covered wire method");

    session.send(call(3, "ticket_add", json!({"title": "First"})));
    let added: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    let id = added["ticket"]["id"].as_str().expect("an id").to_string();

    session.send(call(4, "ticket_show", json!({"id": id})));
    let shown: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(shown["ticket"]["title"], "First");

    session.send(call(
        5,
        "ticket_set",
        json!({"id": id, "status": "in-progress", "assignee": ""}),
    ));
    let moved: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(moved["ticket"]["status"], "in-progress");

    session.send(call(6, "ticket_complete", json!({"id": id})));
    let done: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(done["ticket"]["status"], "done");
}

#[test]
fn the_workflow_tools_round_trip_over_the_server() {
    let sandbox = Sandbox::new("workflow");
    let mut session = Session::start(&sandbox);

    session.send(call(1, "workflow_list", json!({})));
    let listed: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(listed["workflows"][0]["name"], "agile");
    let active = listed["active"].as_str().expect("an active id").to_string();

    session.send(call(2, "workflow_show", json!({})));
    let shown: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(shown["workflow"]["id"], active);

    let stages = json!([
        {"key": "inbox", "label": "Inbox", "role": "intake", "binding": null},
        {"key": "parked", "label": "Parked", "role": "parked", "binding": null},
        {"key": "queued", "label": "Queued", "role": "ready", "binding": {"role": "builder", "pass": "build"}},
        {"key": "doing", "label": "Doing", "role": "active", "binding": {"role": "builder", "pass": "build"}},
        {"key": "checking", "label": "Checking", "role": "review", "binding": {"role": "reviewer", "pass": "review"}},
        {"key": "shipped", "label": "Shipped", "role": "finished", "binding": null}
    ]);
    session.send(call(
        3,
        "workflow_set",
        json!({"name": "flow", "scope": "home", "stages": stages}),
    ));
    let created: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    let id = created["workflow"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    session.send(call(4, "workflow_use", json!({"id": id})));
    let used: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(used["workflow"]["name"], "flow");

    session.send(call(5, "workflow_list", json!({})));
    let relisted: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(relisted["active"], id, "the switch is the active row");
    assert_eq!(
        relisted["workflows"].as_array().expect("workflows").len(),
        2
    );
}

#[test]
fn an_unknown_method_a_batch_and_a_malformed_frame_leave_the_child_answering() {
    let sandbox = Sandbox::new("frames");
    let mut session = Session::start(&sandbox);

    session.send_raw(r#"{"jsonrpc": "2.0", "id": 7, "method": "ticket.bogus", "params": {}}"#);
    let unknown = session.receive();
    assert_eq!(unknown["id"], 7);
    assert_eq!(unknown["error"]["code"], -32601);

    session.send_raw(r#"[{"jsonrpc": "2.0", "id": 8, "method": "ping", "params": {}}]"#);
    let batch = session.receive();
    assert!(batch["id"].is_null());
    assert_eq!(batch["error"]["code"], -32600);

    session.send_raw("this is not a frame");
    let refused = session.receive();
    assert!(refused["id"].is_null());
    assert_eq!(refused["error"]["code"], -32700);

    session.send(json!({"jsonrpc": "2.0", "id": 9, "method": "ping", "params": {}}));
    let answered = session.receive();
    assert_eq!(answered["id"], 9);
    assert!(answered["result"].is_object(), "the child still answers");
}

#[test]
fn closing_stdin_exits_the_child_and_leaves_no_lock_or_temp() {
    let sandbox = Sandbox::new("lifecycle");
    let mut session = Session::start(&sandbox);
    let home = sandbox.home();

    session.send(call(1, "ticket_add", json!({"title": "Kept"})));
    let added: Value =
        serde_json::from_str(&tool_text(&session.receive())).expect("the tool result is JSON");
    assert_eq!(added["ticket"]["title"], "Kept");

    session.close_stdin();
    let status = session.wait_for_exit();
    assert!(
        status.success(),
        "a closed stdin is a clean exit: {status:?}"
    );

    let lock = home.join("tandem.lock");
    assert!(
        !lock.exists(),
        "the store writes no sidecar lock at {}",
        lock.display()
    );

    let leftovers: Vec<String> = fs::read_dir(&home)
        .expect("the home exists")
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with(".tandem-") && name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temp outlives the child: {leftovers:?}"
    );
}
