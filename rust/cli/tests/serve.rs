//! `tandem serve` over a real socket: the board's door, driven the way a
//! browser drives it (ADR 0046).
//!
//! Every case spawns the built binary with `serve --port 0 --home <sandbox>` and
//! speaks HTTP over loopback. The child is a separate process, so these are the
//! claims an in-process router test cannot make:
//!
//! - a full session renders the board, adds a ticket through the form, and
//!   streams the new revision over `GET /events`;
//! - a write made by another process (the CLI in a terminal) arrives on the
//!   stream;
//! - after ADR 0031's restore the server re-attaches and reports the restored
//!   revision instead of staying on the pre-restore board;
//! - a non-loopback host is refused before binding, and `tandem rpc` is gone.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

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
        let root = std::env::temp_dir().join(format!(
            "tandem-serve-cli-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home())
            // No `env_clear`: Windows needs its system environment for
            // winsock to initialize, and a cleared environment makes the
            // server's bind fail before it prints an address.
            .env("HOME", self.root.join("user"))
            .env("TANDEM_HOME", self.home())
            .env_remove("TANDEM_AGENT");
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// One running server, the port it bound, and every stdout line after the
/// listening line. The log goes to stdout, so a reader drains the pipe on its
/// own thread: an undrained pipe would stall the server on its first full
/// buffer.
struct Server {
    child: Child,
    port: u16,
    /// The listening line as printed, before any log line.
    listening: String,
    log: Receiver<String>,
}

impl Server {
    fn start(sandbox: &Sandbox) -> Self {
        Self::start_with(sandbox, &["--host", "127.0.0.1"])
    }

    fn start_with(sandbox: &Sandbox, extra: &[&str]) -> Self {
        Self::start_with_env(sandbox, extra, &[])
    }

    fn start_with_env(sandbox: &Sandbox, extra: &[&str], env: &[(&str, &str)]) -> Self {
        let mut child = sandbox
            .command()
            .arg("serve")
            .arg("--port")
            .arg("0")
            .args(extra)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the server starts");
        let mut output = BufReader::new(child.stdout.take().expect("stdout is piped"));
        let mut line = String::new();
        output
            .read_line(&mut line)
            .expect("the listening line is readable");
        let (sender, log) = mpsc::channel();
        thread::spawn(move || {
            for line in output.lines() {
                let Ok(line) = line else { break };
                // The receiver may be gone; the drain keeps reading regardless.
                let _ = sender.send(line);
            }
        });
        let url = line
            .trim()
            .rsplit(' ')
            .next()
            .filter(|url| url.starts_with("http://"))
            .unwrap_or_else(|| panic!("the server prints its address: {line:?}"));
        let port = url
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or_else(|| panic!("the address carries a port: {url}"));
        Self {
            child,
            port,
            listening: line.trim_end().to_string(),
            log,
        }
    }

    /// The next stdout line after the listening line, within `within`.
    fn next_line(&self, within: Duration) -> String {
        self.log
            .recv_timeout(within)
            .unwrap_or_else(|error| panic!("no log line arrived: {error}"))
    }

    /// The next stdout line that carries `needle`, within `within`. Lines that
    /// do not carry it are skipped, so a test can name the line it means.
    fn next_line_with(&self, needle: &str, within: Duration) -> String {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.next_line(left);
            if line.contains(needle) {
                return line;
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Answer {
    status: u16,
    body: String,
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("the server accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("the read timeout sets");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream
        .write_all(request.as_bytes())
        .expect("the request reaches the server");
    let mut raw = String::new();
    stream
        .read_to_string(&mut raw)
        .expect("the answer is readable");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("the answer has a status: {head}"));
    Answer {
        status,
        body: body.to_string(),
    }
}

fn get(port: u16, path: &str) -> Answer {
    request(port, "GET", path, &[], None)
}

fn form(port: u16, path: &str, body: &str) -> Answer {
    request(
        port,
        "POST",
        path,
        &[("Content-Type", "application/x-www-form-urlencoded")],
        Some(body),
    )
}

/// The revision the board's page was drawn from.
fn revision_of(port: u16) -> i64 {
    let page = get(port, "/board");
    assert_eq!(page.status, 200, "the board answers: {}", page.body);
    let marker = "data-revision=\"";
    let at = page
        .body
        .find(marker)
        .unwrap_or_else(|| panic!("the board carries its revision: {}", page.body));
    let rest = &page.body[at + marker.len()..];
    let end = rest.find('"').expect("the revision is quoted");
    rest[..end].parse().expect("the revision is a number")
}

/// The `data:` revisions a connected `GET /events` stream carries.
fn events(port: u16) -> Receiver<i64> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("the stream connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("the read timeout sets");
        let mut writer = stream.try_clone().expect("the socket clones");
        writer
            .write_all(
                format!(
                    "GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\n\r\n"
                )
                .as_bytes(),
            )
            .expect("the request reaches the server");
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if let Ok(revision) = data.trim().parse::<i64>()
                && sender.send(revision).is_err()
            {
                break;
            }
        }
    });
    receiver
}

/// How long a case waits for a write's change event. The watch's own bounds
/// are 5 s: the fallback re-read for a filesystem whose stat does not move
/// (`WATCH_FALLBACK`) and the backoff cap while a first write is still
/// creating the store (`WATCH_BACKOFF_MAX`). A 5 s wait raced those bounds on
/// a loaded runner, so the deadline sits well past them; a passing case still
/// returns as soon as its event arrives.
const EVENT_WAIT: Duration = Duration::from_secs(30);

/// The next event matching `wanted`, or a failure naming what arrived.
fn next_event(receiver: &Receiver<i64>, wanted: impl Fn(i64) -> bool, within: Duration) -> i64 {
    let deadline = Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            panic!("no matching change event arrived within {within:?}");
        }
        match receiver.recv_timeout(left) {
            Ok(revision) if wanted(revision) => return revision,
            Ok(_) => {}
            Err(error) => panic!("the stream ended before a matching event: {error}"),
        }
    }
}

fn add_through_cli(sandbox: &Sandbox, title: &str) -> String {
    let output = sandbox
        .command()
        .args(["ticket", "add", "--title", title])
        .stdin(Stdio::null())
        .output()
        .expect("the CLI writes the same home");
    assert!(
        output.status.success(),
        "the other writer's verb succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn serve_refusal(sandbox: &Sandbox, token: Option<&str>) -> (Option<i32>, String) {
    let mut command = sandbox.command();
    command
        .args(["serve", "--host", "0.0.0.0", "--port", "0"])
        .env_remove("TANDEM_TOKEN")
        .stdin(Stdio::null());
    if let Some(token) = token {
        command.env("TANDEM_TOKEN", token);
    }
    let output = command.output().expect("the binary runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn serve_refuses_a_non_loopback_host_without_a_token_or_the_flag() {
    let sandbox = Sandbox::new("refuse");
    let (code, stderr) = serve_refusal(&sandbox, None);
    assert_eq!(code, Some(2), "a non-loopback host is a usage refusal");
    assert!(
        stderr.contains("TANDEM_TOKEN")
            && stderr.contains("TLS terminator")
            && stderr.contains("--allow-unauthenticated"),
        "the refusal names the token, the terminator and the opt-out: {stderr}"
    );
}

#[test]
fn serve_refuses_a_non_loopback_host_with_a_short_token() {
    let sandbox = Sandbox::new("short-token");
    let short = "x".repeat(31);
    let (code, stderr) = serve_refusal(&sandbox, Some(&short));
    assert_eq!(
        code,
        Some(2),
        "a 31-character token is refused before binding"
    );
    assert!(
        !stderr.contains(&short),
        "the token is not echoed: {stderr}"
    );
}

#[test]
fn serve_allows_a_non_loopback_host_when_allow_unauthenticated_is_set() {
    let sandbox = Sandbox::new("allow");
    let server = Server::start_with(&sandbox, &["--host", "0.0.0.0", "--allow-unauthenticated"]);
    let page = get(server.port, "/");
    assert_eq!(
        page.status, 200,
        "the opted-in server answers with no token"
    );
    assert!(page.body.contains("board__column"), "the board is drawn");
    let mcp = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
    );
    assert_eq!(mcp.status, 200, "/mcp is open on an opted-in bind too");
    // A phone on the tailnet names the machine, not a loopback authority.
    let named = request(
        server.port,
        "GET",
        "/",
        &[("Host", "machine.tailnet.ts.net:7700")],
        None,
    );
    assert_eq!(named.status, 200, "no Host check off loopback");
}

#[test]
fn serve_on_a_non_loopback_host_with_a_token_gates_every_route_but_health() {
    let sandbox = Sandbox::new("token-serve");
    let token = "t".repeat(32);
    let server = Server::start_with_env(
        &sandbox,
        &["--host", "0.0.0.0"],
        &[("TANDEM_TOKEN", token.as_str())],
    );
    assert_eq!(
        get(server.port, "/").status,
        401,
        "the board wants the token"
    );
    assert_eq!(get(server.port, "/health").status, 200);
    let bearer = format!("Bearer {token}");
    let page = request(
        server.port,
        "GET",
        "/",
        &[("Authorization", bearer.as_str())],
        None,
    );
    assert_eq!(page.status, 200, "the token opens the board");
}

fn mcp_post(port: u16, body: &str) -> Answer {
    request(
        port,
        "POST",
        "/mcp",
        &[("Content-Type", "application/json")],
        Some(body),
    )
}

#[test]
fn mcp_over_a_real_socket_handshakes_lists_tools_and_answers_frames() {
    let sandbox = Sandbox::new("mcp-handshake");
    let server = Server::start(&sandbox);

    let hello = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
    );
    assert_eq!(hello.status, 200, "{}", hello.body);
    let hello: serde_json::Value = serde_json::from_str(&hello.body).expect("one frame");
    assert_eq!(hello["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(hello["result"]["serverInfo"]["name"], "tandem-mcp");

    let notification = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    assert_eq!(notification.status, 202);

    let listed = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    );
    let listed: serde_json::Value = serde_json::from_str(&listed.body).expect("one frame");
    assert_eq!(
        listed["result"]["tools"].as_array().map(Vec::len),
        Some(28),
        "the door covers every covered wire method"
    );

    // A malformed frame is answered as a frame and the server keeps answering.
    let malformed = mcp_post(server.port, "this is not a frame");
    assert_eq!(malformed.status, 200);
    let malformed: serde_json::Value = serde_json::from_str(&malformed.body).expect("one frame");
    assert_eq!(malformed["error"]["code"], -32700);

    let batch = mcp_post(
        server.port,
        r#"[{"jsonrpc":"2.0","id":8,"method":"ping","params":{}}]"#,
    );
    let batch: serde_json::Value = serde_json::from_str(&batch.body).expect("one frame");
    assert_eq!(batch["error"]["code"], -32600);

    let unknown = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","id":7,"method":"ticket.bogus","params":{}}"#,
    );
    let unknown: serde_json::Value = serde_json::from_str(&unknown.body).expect("one frame");
    assert_eq!(unknown["error"]["code"], -32601);

    let ping = mcp_post(
        server.port,
        r#"{"jsonrpc":"2.0","id":9,"method":"ping","params":{}}"#,
    );
    assert!(ping.body.contains("\"result\""), "{}", ping.body);
}

#[test]
fn mcp_writes_land_in_the_store_a_cli_in_the_same_home_reads() {
    let sandbox = Sandbox::new("mcp-ticket-loop");
    let server = Server::start(&sandbox);
    let added = request(
        server.port,
        "POST",
        "/mcp",
        &[
            ("Content-Type", "application/json"),
            ("X-Tandem-Agent", "builder"),
        ],
        Some(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ticket_add","arguments":{"title":"Over HTTP"}}}"#,
        ),
    );
    assert_eq!(added.status, 200, "{}", added.body);
    let output = sandbox
        .command()
        .args(["ticket", "list"])
        .stdin(Stdio::null())
        .output()
        .expect("the CLI reads the same home");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Over HTTP"),
        "the HTTP write is the store's write"
    );
}

#[test]
fn the_mcp_log_line_names_the_route_and_never_the_body_or_the_actor() {
    let sandbox = Sandbox::new("mcp-log");
    let server = Server::start(&sandbox);
    request(
        server.port,
        "POST",
        "/mcp",
        &[
            ("Content-Type", "application/json"),
            ("X-Tandem-Agent", "secret-actor-name"),
        ],
        Some(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ticket_add","arguments":{"title":"Never logged"}}}"#,
        ),
    );
    let line = server.next_line_with("/mcp", Duration::from_secs(10));
    assert!(!line.contains("Never logged"), "{line}");
    assert!(!line.contains("secret-actor-name"), "{line}");
}

#[test]
fn tandem_rpc_subcommand_is_retired_and_exits_usage_error() {
    let sandbox = Sandbox::new("retired");
    let output = sandbox
        .command()
        .arg("rpc")
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unknown subcommand is a usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("tandem-cli") && stderr.contains("Usage:"),
        "the refusal prints the grammar: {stderr}"
    );
}

#[test]
fn a_serve_session_renders_board_adds_ticket_and_streams_sse_changed_events() {
    let sandbox = Sandbox::new("session");
    let server = Server::start(&sandbox);
    let page = get(server.port, "/");
    assert_eq!(page.status, 200, "the board answers");
    for status in [
        "backlog",
        "on-hold",
        "ready",
        "in-progress",
        "in-review",
        "done",
    ] {
        assert!(
            page.body.contains(&format!("data-status=\"{status}\"")),
            "the {status} column is drawn"
        );
    }

    let stream = events(server.port);
    let answer = form(
        server.port,
        "/tickets",
        "title=First&status=ready&label=&expected_revision=0",
    );
    assert_eq!(
        answer.status, 200,
        "the add answers the board: {}",
        answer.body
    );
    assert!(
        answer.body.contains("First"),
        "the new card is drawn: {}",
        answer.body
    );
    let current = revision_of(server.port);
    assert!(current > 0, "the write moved the revision");
    let event = next_event(&stream, |revision| revision == current, EVENT_WAIT);
    assert_eq!(event, current, "the stream carries the write's revision");
    let board = get(server.port, "/board");
    assert!(
        board.body.contains("First"),
        "the board reads the write back"
    );
}

#[test]
fn a_write_from_another_process_arrives_on_the_sse_stream() {
    let sandbox = Sandbox::new("another-process");
    let server = Server::start(&sandbox);
    let before = revision_of(server.port);
    let stream = events(server.port);

    add_through_cli(&sandbox, "Outside");

    let after = revision_of(server.port);
    assert!(after > before, "the other process moved the revision");
    let event = next_event(&stream, |revision| revision == after, EVENT_WAIT);
    assert_eq!(event, after, "the stream carries the listing's revision");
    let board = get(server.port, "/board");
    assert!(
        board.body.contains("Outside"),
        "the board reads the other process's write"
    );
}

// The restore while the server runs: the backup is renamed over the database
// the watch holds open, which Windows refuses, so the case is unix only. The
// app-closed restore a Windows human runs is the next case.
#[cfg(unix)]
#[test]
fn a_restored_store_re_attaches_and_reports_the_restored_revision_over_sse() {
    let sandbox = Sandbox::new("restore");
    let server = Server::start(&sandbox);
    add_through_cli(&sandbox, "One");
    add_through_cli(&sandbox, "Two");
    let before = revision_of(server.port);
    assert!(before > 1, "the two writes moved the revision");

    let home = sandbox.home();
    fs::remove_file(home.join("tandem.db-wal")).ok();
    fs::remove_file(home.join("tandem.db-shm")).ok();
    fs::remove_file(home.join("tandem.db")).expect("the database is removable");
    fs::rename(home.join("tandem.db.bak"), home.join("tandem.db"))
        .expect("the backup moves over the path");

    let stream = events(server.port);
    let restored = next_event(
        &stream,
        |revision| revision < before,
        Duration::from_secs(20),
    );
    let board = get(server.port, "/board");
    assert!(
        board.body.contains("One"),
        "the restored board carries the backup's ticket: {}",
        board.body
    );
    assert!(
        !board.body.contains("Two"),
        "the backup predates the second write: {}",
        board.body
    );

    add_through_cli(&sandbox, "After");
    let moved = revision_of(server.port);
    assert!(
        moved > restored,
        "the write moved past the restored revision"
    );
    let event = next_event(
        &stream,
        |revision| revision == moved,
        Duration::from_secs(60),
    );
    assert_eq!(
        event, moved,
        "the stream keeps reporting after the re-attach"
    );
}

#[test]
fn a_restored_store_is_re_attached_on_the_next_serve_start() {
    let sandbox = Sandbox::new("restore-next-open");
    {
        let server = Server::start(&sandbox);
        add_through_cli(&sandbox, "One");
        add_through_cli(&sandbox, "Two");
        let before = revision_of(server.port);
        assert!(before > 1, "the two writes moved the revision");
        drop(server);
    }

    let home = sandbox.home();
    fs::remove_file(home.join("tandem.db-wal")).ok();
    fs::remove_file(home.join("tandem.db-shm")).ok();
    fs::remove_file(home.join("tandem.db")).expect("the database is removable");
    fs::rename(home.join("tandem.db.bak"), home.join("tandem.db"))
        .expect("the backup moves over the path");

    let server = Server::start(&sandbox);
    let restored = revision_of(server.port);
    let stream = events(server.port);
    let reported = next_event(
        &stream,
        |revision| revision == restored,
        Duration::from_secs(15),
    );
    assert_eq!(
        reported, restored,
        "the next open reports the restored revision"
    );
    let board = get(server.port, "/board");
    assert!(board.body.contains("One"), "the restored board is read");
    assert!(
        !board.body.contains("Two"),
        "the backup predates the second write"
    );

    add_through_cli(&sandbox, "After");
    let moved = revision_of(server.port);
    assert!(
        moved > restored,
        "the write moved past the restored revision"
    );
    let event = next_event(
        &stream,
        |revision| revision == moved,
        Duration::from_secs(60),
    );
    assert_eq!(
        event, moved,
        "the stream reports the write after the restore"
    );
}

/// The server's second face: the ticket sheet and the bikeshed notes, driven
/// over a real socket the way a browser drives them. The in-process router test
/// proves the markup; this proves the routes answer a separate process with the
/// home it was given.
#[test]
fn a_serve_session_drives_the_sheet_and_notes_faces() {
    let sandbox = Sandbox::new("sheet-notes");
    let server = Server::start(&sandbox);

    let answer = form(
        server.port,
        "/tickets",
        "title=Sheet&status=ready&label=&expected_revision=0",
    );
    assert_eq!(answer.status, 200, "the add answers: {}", answer.body);

    let board = get(server.port, "/board");
    let marker = "data-ticket=\"";
    let at = board
        .body
        .find(marker)
        .unwrap_or_else(|| panic!("the board carries a card id: {}", board.body));
    let rest = &board.body[at + marker.len()..];
    let id = &rest[..rest.find('"').expect("the id is quoted")];

    let sheet = get(server.port, &format!("/tickets/{id}"));
    assert_eq!(sheet.status, 200, "the sheet answers: {}", sheet.body);
    assert!(
        sheet.body.contains("<!doctype html>"),
        "the sheet rides the shell"
    );
    assert!(sheet.body.contains("sheet-backdrop"), "the sheet is drawn");
    assert!(sheet.body.contains("Sheet"), "the ticket title is drawn");

    let revision = revision_of(server.port);
    let answer = form(
        server.port,
        &format!("/tickets/{id}"),
        &format!(
            "title=Sheet&status=ready&assignee=&description=More&expected_revision={revision}"
        ),
    );
    assert_eq!(answer.status, 200);
    assert!(
        answer.body.contains("More"),
        "the description save is redrawn: {}",
        answer.body
    );

    let answer = form(
        server.port,
        &format!("/tickets/{id}/comments"),
        "body=Hello",
    );
    assert_eq!(answer.status, 200);
    assert!(
        answer.body.contains("Hello"),
        "the comment is redrawn: {}",
        answer.body
    );

    let answer = form(server.port, "/notes/create", "path=Index.md");
    assert_eq!(answer.status, 200, "the note is created: {}", answer.body);
    assert!(
        answer.body.contains("Index"),
        "the tree redraws the note: {}",
        answer.body
    );

    let edit = get(server.port, "/notes/edit?path=Index.md");
    assert_eq!(edit.status, 200, "the editor answers: {}", edit.body);
    assert!(edit.body.contains("note-editor"), "the editor is drawn");
    let hash_marker = "name=\"baseline_hash\" value=\"";
    let at = edit
        .body
        .find(hash_marker)
        .unwrap_or_else(|| panic!("the editor carries the baseline: {}", edit.body));
    let rest = &edit.body[at + hash_marker.len()..];
    let hash = &rest[..rest.find('"').expect("the hash is quoted")];

    let answer = form(
        server.port,
        "/notes/save",
        &format!("path=Index.md&text=Hello&baseline_hash={hash}"),
    );
    assert_eq!(answer.status, 200, "the save answers: {}", answer.body);
    let notes = get(server.port, "/notes?path=Index.md");
    assert_eq!(notes.status, 200);
    assert!(
        notes.body.contains("Hello"),
        "the note reads back: {}",
        notes.body
    );

    let stream = events(server.port);
    let revision = revision_of(server.port);
    form(
        server.port,
        "/tickets",
        &format!("title=Later&status=backlog&label=&expected_revision={revision}"),
    );
    let current = revision_of(server.port);
    next_event(&stream, |revision| revision == current, EVENT_WAIT);
}

#[test]
fn the_listening_line_is_first_and_each_request_is_a_timestamped_log_line() {
    let sandbox = Sandbox::new("log-request");
    let server = Server::start(&sandbox);
    assert_eq!(
        server.listening,
        format!(
            "tandem serve: listening on http://127.0.0.1:{}",
            server.port
        )
    );
    let answer = get(server.port, "/");
    assert_eq!(answer.status, 200);
    let line = server.next_line_with("route=", Duration::from_secs(10));
    assert!(
        line.starts_with(|c: char| c.is_ascii_digit()),
        "a log line starts with its timestamp: {line}"
    );
    assert!(line.contains(" INFO "), "the request is info: {line}");
    assert!(
        line.contains("route=\"/\""),
        "the route is the template: {line}"
    );
    assert!(line.contains("status=200"), "the status is logged: {line}");
    assert!(
        line.contains("latency_ms="),
        "the latency is logged: {line}"
    );
}

#[test]
fn tandem_log_sets_the_filter_and_warn_hides_the_info_request_line() {
    let sandbox = Sandbox::new("log-filter");
    let server = Server::start_with_env(
        &sandbox,
        &["--host", "127.0.0.1"],
        &[("TANDEM_LOG", "warn")],
    );
    assert_eq!(get(server.port, "/").status, 200);
    assert_eq!(get(server.port, "/no-such-route").status, 404);
    // The info request for `/` and the startup line are filtered out, so the
    // first line that reaches stdout is the 404's warning.
    let line = server.next_line(Duration::from_secs(10));
    assert!(
        line.contains(" WARN "),
        "the warning is the first line: {line}"
    );
    assert!(line.contains("status=404"), "the 404 is logged: {line}");
}

#[test]
fn an_unparsable_tandem_log_still_serves_and_warns_once_without_echoing_it() {
    let sandbox = Sandbox::new("log-invalid");
    let server = Server::start_with_env(
        &sandbox,
        &["--host", "127.0.0.1"],
        &[("TANDEM_LOG", "not[valid")],
    );
    assert_eq!(get(server.port, "/").status, 200, "the board still serves");
    let line = server.next_line(Duration::from_secs(10));
    assert!(line.contains(" WARN "), "the fallback warns: {line}");
    assert!(
        line.contains("TANDEM_LOG"),
        "the warning names the variable: {line}"
    );
    assert!(
        !line.contains("not[valid"),
        "the value is never echoed: {line}"
    );
}

// CUT-37: --public-origin.

const PUBLIC_ORIGIN: &str = "https://ws.example.com";

/// A serve run that must refuse before binding: its exit code and stderr.
fn origin_refusal(
    sandbox: &Sandbox,
    host: &str,
    flag: Option<&str>,
    variable: Option<&str>,
) -> (Option<i32>, String) {
    let mut command = sandbox.command();
    command
        .args(["serve", "--host", host, "--port", "0"])
        .env("TANDEM_TOKEN", "t".repeat(32))
        .env_remove("TANDEM_PUBLIC_ORIGIN")
        .stdin(Stdio::null());
    if let Some(flag) = flag {
        command.args(["--public-origin", flag]);
    }
    if let Some(variable) = variable {
        command.env("TANDEM_PUBLIC_ORIGIN", variable);
    }
    let output = command.output().expect("the binary runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn assert_origin_refused(flag: Option<&str>, variable: Option<&str>, host: &str, wanted: &str) {
    let sandbox = Sandbox::new("origin-refusal");
    let (code, stderr) = origin_refusal(&sandbox, host, flag, variable);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains(wanted), "wanted {wanted:?} in {stderr}");
    for value in [flag, variable].into_iter().flatten() {
        if !value.is_empty() {
            assert!(!stderr.contains(value), "the value is not echoed: {stderr}");
        }
    }
}

#[test]
fn serve_refuses_a_public_origin_that_is_http() {
    assert_origin_refused(
        Some("http://ws.example.com"),
        None,
        "0.0.0.0",
        "must be https",
    );
}

#[test]
fn serve_refuses_a_public_origin_with_a_path_or_trailing_slash() {
    assert_origin_refused(
        Some("https://ws.example.com/"),
        None,
        "0.0.0.0",
        "write https://workspace.example, not https://workspace.example/",
    );
    assert_origin_refused(Some("https://ws.example.com/x"), None, "0.0.0.0", "path");
}

#[test]
fn serve_refuses_a_public_origin_with_userinfo() {
    assert_origin_refused(
        Some("https://me:secret@ws.example.com"),
        None,
        "0.0.0.0",
        "userinfo",
    );
}

#[test]
fn serve_refuses_a_public_origin_on_a_loopback_bind() {
    assert_origin_refused(Some(PUBLIC_ORIGIN), None, "127.0.0.1", "non-loopback");
    assert_origin_refused(None, Some(PUBLIC_ORIGIN), "127.0.0.1", "non-loopback");
}

#[test]
fn serve_refuses_a_public_origin_that_is_empty_as_a_flag() {
    assert_origin_refused(Some(""), None, "0.0.0.0", "--public-origin is empty");
}

#[test]
fn serve_refuses_a_public_origin_malformed_in_the_env_even_with_the_flag() {
    assert_origin_refused(
        Some(PUBLIC_ORIGIN),
        Some("http://ws.example.com"),
        "0.0.0.0",
        "TANDEM_PUBLIC_ORIGIN",
    );
}

#[test]
fn serve_prints_the_public_origin_after_the_listening_line_and_the_flag_wins() {
    let sandbox = Sandbox::new("origin-line");
    let token = "t".repeat(32);
    let server = Server::start_with_env(
        &sandbox,
        &[
            "--host",
            "0.0.0.0",
            "--public-origin",
            "HTTPS://WS.Example.com:443",
        ],
        &[
            ("TANDEM_TOKEN", token.as_str()),
            ("TANDEM_PUBLIC_ORIGIN", "https://other.example"),
        ],
    );
    assert!(
        server.listening.contains("listening on http://"),
        "{}",
        server.listening
    );
    let line = server.next_line(Duration::from_secs(10));
    assert_eq!(line, format!("tandem serve: public origin {PUBLIC_ORIGIN}"));
}

#[test]
fn an_empty_env_origin_is_unset_and_the_env_alone_configures_the_origin() {
    let sandbox = Sandbox::new("origin-env");
    let token = "t".repeat(32);
    let server = Server::start_with_env(
        &sandbox,
        &["--host", "0.0.0.0"],
        &[
            ("TANDEM_TOKEN", token.as_str()),
            ("TANDEM_PUBLIC_ORIGIN", ""),
        ],
    );
    let started = server.next_line_with("tandem serve started", Duration::from_secs(10));
    assert!(started.contains("origin=\"none\""), "{started}");
    drop(server);

    let server = Server::start_with_env(
        &sandbox,
        &["--host", "0.0.0.0"],
        &[
            ("TANDEM_TOKEN", token.as_str()),
            ("TANDEM_PUBLIC_ORIGIN", PUBLIC_ORIGIN),
        ],
    );
    assert_eq!(
        server.next_line(Duration::from_secs(10)),
        format!("tandem serve: public origin {PUBLIC_ORIGIN}")
    );
}

#[test]
fn a_configured_origin_passes_a_form_write_and_mcp_that_it_refuses_unconfigured() {
    let token = "t".repeat(32);
    let bearer = format!("Bearer {token}");
    let form_headers = [
        ("Authorization", bearer.as_str()),
        ("Origin", PUBLIC_ORIGIN),
        ("Content-Type", "application/x-www-form-urlencoded"),
    ];
    let mcp_headers = [
        ("Authorization", bearer.as_str()),
        ("Origin", PUBLIC_ORIGIN),
        ("Content-Type", "application/json"),
    ];
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
    for (extra, configured) in [
        (&["--host", "0.0.0.0"][..], false),
        (
            &["--host", "0.0.0.0", "--public-origin", PUBLIC_ORIGIN][..],
            true,
        ),
    ] {
        let sandbox = Sandbox::new("origin-live");
        let server = Server::start_with_env(&sandbox, extra, &[("TANDEM_TOKEN", token.as_str())]);
        // The request's Host is the upstream's, as behind nginx's default.
        let form = request(
            server.port,
            "POST",
            "/tickets",
            &form_headers,
            Some("title=x"),
        );
        let mcp = request(server.port, "POST", "/mcp", &mcp_headers, Some(ping));
        if configured {
            assert_ne!(form.status, 403, "the form write passes: {}", form.body);
            assert_eq!(mcp.status, 200, "/mcp passes");
        } else {
            assert_eq!(form.status, 403, "the form write is refused");
            assert_eq!(mcp.status, 403, "/mcp is refused");
        }
    }
}
