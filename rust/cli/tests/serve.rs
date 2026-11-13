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

/// One running server and the port it bound.
struct Server {
    child: Child,
    port: u16,
    _output: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn start(sandbox: &Sandbox) -> Self {
        Self::start_with(sandbox, &["--host", "127.0.0.1"])
    }

    fn start_with(sandbox: &Sandbox, extra: &[&str]) -> Self {
        let mut child = sandbox
            .command()
            .arg("serve")
            .arg("--port")
            .arg("0")
            .args(extra)
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
            _output: output,
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

#[test]
fn serve_refuses_non_loopback_host_without_allow_unauthenticated() {
    let sandbox = Sandbox::new("refuse");
    let output = sandbox
        .command()
        .args(["serve", "--host", "0.0.0.0", "--port", "0"])
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a non-loopback host is a usage refusal"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--allow-unauthenticated"),
        "the refusal names the escape: {stderr}"
    );
}

#[test]
fn serve_allows_non_loopback_host_when_allow_unauthenticated_is_set() {
    let sandbox = Sandbox::new("allow");
    let server = Server::start_with(&sandbox, &["--host", "0.0.0.0", "--allow-unauthenticated"]);
    let page = get(server.port, "/");
    assert_eq!(page.status, 200, "the opted-in server answers");
    assert!(page.body.contains("board__column"), "the board is drawn");
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
        "title=First&status=ready&lane=&expected_revision=0",
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
    let event = next_event(
        &stream,
        |revision| revision == current,
        Duration::from_secs(5),
    );
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
    let event = next_event(
        &stream,
        |revision| revision == after,
        Duration::from_secs(5),
    );
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
        "title=Sheet&status=ready&lane=&expected_revision=0",
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
        &format!("/tickets/{id}/description"),
        &format!("addition=More&expected_revision={revision}"),
    );
    assert_eq!(answer.status, 200);
    assert!(
        answer.body.contains("More"),
        "the append is redrawn: {}",
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
        &format!("title=Later&status=backlog&lane=&expected_revision={revision}"),
    );
    let current = revision_of(server.port);
    next_event(
        &stream,
        |revision| revision == current,
        Duration::from_secs(5),
    );
}
