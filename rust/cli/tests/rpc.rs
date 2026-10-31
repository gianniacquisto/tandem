//! The rpc mode over a real pipe: the app's door, driven the way the app
//! drives it (`docs/port.md`, Transport and the UI contract).
//!
//! Every case spawns the built binary with piped stdin and stdout and speaks
//! one JSON frame per line. The child is a separate process, so these are the
//! claims a unit test over an in-memory stream cannot make:
//!
//! - a full session answers: a snapshot with its revision, a mutation with the
//!   new revision, the change event that carries it, and a listing that reads
//!   the write back;
//! - a write made by another process (the CLI in a terminal) arrives on the
//!   stream as a change event, because the child reads the store and not a
//!   cache of its own;
//! - a malformed frame answers an error frame and leaves the child answering;
//! - a closed stdin ends the child, leaves the store readable and no temp
//!   behind, so nothing outlives the app that spawned it;
//! - after ADR 0031's restore the child re-attaches and reports the restored
//!   revision instead of going silent on the pre-restore board: renamed over the
//!   store while the child runs on unix, and, the way the documented Windows
//!   recipe does it, restored with the app closed and attached on the next open.

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
            std::env::temp_dir().join(format!("tandem-rpc-{name}-{}-{serial}", std::process::id()));
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

/// One running child and its two pipes. Frames that arrive before the caller
/// asks for them are queued, so a change event racing a response cannot make a
/// case flaky.
struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    pending: Vec<Value>,
    home: PathBuf,
}

impl Session {
    fn start(sandbox: &Sandbox) -> Self {
        let home = sandbox.home();
        let mut child = Command::new(BINARY)
            .arg("rpc")
            .arg("--home")
            .arg(&home)
            .env_clear()
            .env("HOME", sandbox.root.join("user"))
            .env("TANDEM_HOME", &home)
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
            pending: Vec::new(),
            home,
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

    /// The next frame, whatever it is.
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

    /// The next frame matching `wanted`, queueing the others.
    fn receive_where(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.pending.iter().position(&wanted) {
            return self.pending.remove(index);
        }
        loop {
            let frame = self.receive();
            if wanted(&frame) {
                return frame;
            }
            self.pending.push(frame);
        }
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

fn revision(frame: &Value) -> i64 {
    frame["result"]["revision"]
        .as_i64()
        .unwrap_or_else(|| panic!("the frame carries a revision: {frame}"))
}

/// A request padded with trailing spaces (legal JSON) to exactly `len` bytes,
/// so a case can sit on the frame cap without megabytes of text.
fn request_of_len(id: u64, len: usize) -> String {
    let mut frame = json!({"id": id, "method": "ticket.list", "params": {}}).to_string();
    assert!(frame.len() <= len, "the request fits under the cap");
    frame.push_str(&" ".repeat(len - frame.len()));
    frame
}

#[test]
fn a_frame_of_exactly_the_cap_is_answered() {
    let sandbox = Sandbox::new("cap");
    let mut session = Session::start(&sandbox);

    session.send_raw(&request_of_len(11, tandem_core::rpc::MAX_FRAME));
    let answered = session.receive_where(|frame| frame["id"] == 11);
    assert!(
        answered["result"].is_object(),
        "a frame of exactly the cap is a frame: {answered}"
    );
}

#[test]
fn a_frame_over_the_cap_is_refused_without_eating_the_next_request() {
    let sandbox = Sandbox::new("over-cap");
    let mut session = Session::start(&sandbox);

    session.send_raw(&request_of_len(12, tandem_core::rpc::MAX_FRAME + 1));
    let refused = session.receive_where(|frame| frame["error"].is_object());
    assert!(
        refused["id"].is_null(),
        "the refused frame carries no id: {refused}"
    );
    assert_eq!(refused["error"]["code"], "parse");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("longer than"),
        "the error names the cap: {refused}"
    );

    // The drain stopped at the refused frame's own newline, so the request
    // after it is still a request and still answered.
    session.send(json!({"id": 13, "method": "ticket.list", "params": {}}));
    let answered = session.receive_where(|frame| frame["id"] == 13);
    assert!(
        answered["result"].is_object(),
        "the request after a refused frame is answered: {answered}"
    );
}

#[test]
fn a_pipe_session_snapshots_mutates_and_reports_the_change() {
    let sandbox = Sandbox::new("session");
    let mut session = Session::start(&sandbox);

    session.send(json!({"id": 1, "method": "ticket.list", "params": {}}));
    let snapshot = session.receive_where(|frame| frame["id"] == 1);
    assert_eq!(revision(&snapshot), 0, "a home with no store reads empty");
    assert_eq!(snapshot["result"]["tickets"], json!([]));

    session.send(json!({"id": 2, "method": "ticket.add", "params": {"title": "First"}}));
    let added = session.receive_where(|frame| frame["id"] == 2);
    let written = revision(&added);
    assert!(written > 0, "the write moved the revision");
    assert_eq!(added["result"]["ticket"]["title"], "First");

    // The listing's revision is authoritative, and an event is a hint to
    // re-read. A first write on a fresh home also migrates, and a migration is
    // a commit of its own, so the stream may carry an intermediate revision
    // before this one: match the number the write produced.
    let event =
        session.receive_where(|frame| frame["event"] == "changed" && frame["revision"] == written);
    assert_eq!(
        event["revision"], written,
        "the event carries the revision the write produced"
    );

    session.send(json!({"id": 3, "method": "ticket.list", "params": {}}));
    let after = session.receive_where(|frame| frame["id"] == 3);
    assert_eq!(revision(&after), written);
    assert_eq!(after["result"]["tickets"][0]["title"], "First");
}

#[test]
fn a_write_from_another_process_arrives_as_a_change_event() {
    let sandbox = Sandbox::new("another-process");
    let mut session = Session::start(&sandbox);

    session.send(json!({"id": 1, "method": "ticket.list", "params": {}}));
    let before = revision(&session.receive_where(|frame| frame["id"] == 1));

    let status = Command::new(BINARY)
        .args(["ticket", "add", "--title", "Outside"])
        .arg("--home")
        .arg(sandbox.home())
        .env_clear()
        .env("HOME", sandbox.root.join("user"))
        .env("TANDEM_HOME", sandbox.home())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("the CLI writes the same home");
    assert!(status.success(), "the other writer's verb succeeds");

    // The listing is authoritative: it names the revision its tickets are a
    // snapshot of. An event may have carried an earlier number, because the
    // other process's first write on a fresh home also migrates, and a
    // migration commits on its own. So list first, then pin that the stream
    // carries exactly the revision the listing answered.
    session.send(json!({"id": 2, "method": "ticket.list", "params": {}}));
    let after = session.receive_where(|frame| frame["id"] == 2);
    let moved = revision(&after);
    assert!(
        moved > before,
        "the other process moved the revision ({before} to {moved})"
    );
    assert_eq!(after["result"]["tickets"][0]["title"], "Outside");

    let event =
        session.receive_where(|frame| frame["event"] == "changed" && frame["revision"] == moved);
    assert_eq!(
        event["revision"], moved,
        "the stream carries the revision the listing is a snapshot of"
    );
}

// The restore while the child runs: the backup is renamed over the database the
// watch holds open, which Windows refuses (SQLite's VFS does not ask for
// `FILE_SHARE_DELETE`), so the case is unix only. The app-closed restore a
// Windows human runs, and the re-attach on the next open it leaves, is
// `a_restored_store_is_re_attached_on_the_next_open` below (PRT-26).
#[cfg(unix)]
#[test]
fn a_restored_store_re_attaches_and_reports_the_restored_revision() {
    let sandbox = Sandbox::new("restore");
    let mut session = Session::start(&sandbox);

    session.send(json!({"id": 1, "method": "ticket.add", "params": {"title": "One"}}));
    session.receive_where(|frame| frame["id"] == 1);
    session.send(json!({"id": 2, "method": "ticket.add", "params": {"title": "Two"}}));
    session.receive_where(|frame| frame["id"] == 2);

    session.send(json!({"id": 3, "method": "ticket.list", "params": {}}));
    let before = revision(&session.receive_where(|frame| frame["id"] == 3));

    // ADR 0031's restore, while the child runs: the backup is the
    // pre-second-write state and is written in rollback-journal mode.
    let home = sandbox.home();
    let _ = std::fs::remove_file(home.join("tandem.db-wal"));
    let _ = std::fs::remove_file(home.join("tandem.db-shm"));
    std::fs::remove_file(home.join("tandem.db")).expect("the database is removable");
    std::fs::rename(home.join("tandem.db.bak"), home.join("tandem.db"))
        .expect("the backup moves over the path");

    // The child notices the replacement, re-attaches to the rollback-journal
    // store and reports the restored revision, which is earlier than the one it
    // held: the app re-lists instead of staying on the pre-restore board.
    let event = session.receive_where(|frame| {
        frame["event"] == "changed" && frame["revision"].as_i64().is_some_and(|at| at < before)
    });
    let restored = event["revision"]
        .as_i64()
        .expect("the event carries a revision");

    session.send(json!({"id": 4, "method": "ticket.list", "params": {}}));
    let after = session.receive_where(|frame| frame["id"] == 4);
    assert_eq!(
        revision(&after),
        restored,
        "the listing is the restored state"
    );
    assert_eq!(
        after["result"]["tickets"][0]["title"], "One",
        "the backup predates the second write"
    );

    // The held watch does not block the next write's switch back to WAL, and
    // the stream carries the write's revision.
    session.send(json!({"id": 5, "method": "ticket.add", "params": {"title": "After"}}));
    let added = session.receive_where(|frame| frame["id"] == 5);
    let moved = revision(&added);
    assert!(
        moved > restored,
        "the write moved past the restored revision"
    );
    let event =
        session.receive_where(|frame| frame["event"] == "changed" && frame["revision"] == moved);
    assert_eq!(event["revision"], moved);
}

// The restore the docs tell a Windows human to run: the app is closed while
// the backup moves over the store, so nothing renames over a held file. The
// next child attaches to the rollback-journal store and reports it, and the
// stream keeps reporting once a write switches WAL back on.
#[test]
fn a_restored_store_is_re_attached_on_the_next_open() {
    let sandbox = Sandbox::new("restore-next-open");
    let mut closed = Session::start(&sandbox);
    closed.send(json!({"id": 1, "method": "ticket.add", "params": {"title": "One"}}));
    closed.receive_where(|frame| frame["id"] == 1);
    closed.send(json!({"id": 2, "method": "ticket.add", "params": {"title": "Two"}}));
    closed.receive_where(|frame| frame["id"] == 2);
    closed.send(json!({"id": 3, "method": "ticket.list", "params": {}}));
    let before = revision(&closed.receive_where(|frame| frame["id"] == 3));

    // The app is closed for the restore, which is the documented recipe on
    // Windows: a live watch holds `tandem.db` open and the platform refuses the
    // remove and the rename.
    drop(closed);

    let home = sandbox.home();
    let _ = fs::remove_file(home.join("tandem.db-wal"));
    let _ = fs::remove_file(home.join("tandem.db-shm"));
    fs::remove_file(home.join("tandem.db")).expect("the database is removable");
    fs::rename(home.join("tandem.db.bak"), home.join("tandem.db"))
        .expect("the backup moves over the path");

    // The next open attaches to the rollback-journal store and reads it.
    let mut session = Session::start(&sandbox);
    session.send(json!({"id": 4, "method": "ticket.list", "params": {}}));
    let restored_frame = session.receive_where(|frame| frame["id"] == 4);
    let restored = revision(&restored_frame);
    assert!(
        restored < before,
        "the backup is an earlier revision ({restored} is not below {before})"
    );
    assert_eq!(
        restored_frame["result"]["tickets"][0]["title"], "One",
        "the backup predates the second write"
    );

    // The child watches the restored store, and its first write switches WAL
    // back on: the event carries that write's revision.
    session.send(json!({"id": 5, "method": "ticket.add", "params": {"title": "After"}}));
    let added = session.receive_where(|frame| frame["id"] == 5);
    let moved = revision(&added);
    assert!(
        moved > restored,
        "the write moved past the restored revision"
    );
    let event =
        session.receive_where(|frame| frame["event"] == "changed" && frame["revision"] == moved);
    assert_eq!(event["revision"], moved);
}

#[test]
fn a_malformed_frame_answers_an_error_and_the_child_stays_alive() {
    let sandbox = Sandbox::new("malformed");
    let mut session = Session::start(&sandbox);

    session.send_raw("this is not a frame");
    let refused = session.receive_where(|frame| frame["error"].is_object());
    assert!(refused["id"].is_null(), "no id to echo: {refused}");
    assert_eq!(refused["error"]["code"], "parse");
    assert!(
        !refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "the error says what was wrong"
    );

    session.send_raw(r#"{"id": 7, "method": "ticket.bogus", "params": {}}"#);
    let unknown = session.receive_where(|frame| frame["error"].is_object());
    assert_eq!(unknown["id"], 7, "a frame with an id echoes it");
    assert_eq!(unknown["error"]["code"], "method");

    session.send(json!({"id": 8, "method": "ticket.list", "params": {}}));
    let answered = session.receive_where(|frame| frame["id"] == 8);
    assert!(answered["result"].is_object(), "the child still answers");
}

#[test]
fn closing_stdin_exits_the_child_and_leaves_no_lock_or_temp() {
    let sandbox = Sandbox::new("lifecycle");
    let mut session = Session::start(&sandbox);

    session.send(json!({"id": 1, "method": "ticket.add", "params": {"title": "Kept"}}));
    let added = session.receive_where(|frame| frame["id"] == 1);
    assert_eq!(added["result"]["ticket"]["title"], "Kept");
    let home = session.home.clone();

    session.close_stdin();
    let status = session.wait_for_exit();
    assert!(
        status.success(),
        "a closed stdin is a clean exit: {status:?}"
    );

    // No orphan: the wait above returned, and a second wait agrees the child is
    // reaped rather than left running.
    assert!(
        session
            .child
            .try_wait()
            .expect("the child is waitable")
            .is_some(),
        "the child is reaped"
    );

    // The store takes no sidecar lock (ADR 0031, PRT-23): a write is one
    // transaction, and a `tandem.lock` beside a ported home would be inert.
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

    // And the write it made is readable by the next process, which is what
    // "released the store" means in practice.
    let listed = Command::new(BINARY)
        .args(["ticket", "list"])
        .arg("--home")
        .arg(&home)
        .env_clear()
        .env("HOME", sandbox.root.join("user"))
        .env("TANDEM_HOME", &home)
        .stdin(Stdio::null())
        .output()
        .expect("the CLI reads the same home");
    assert!(
        listed.status.success(),
        "the store is readable after the child"
    );
    let board: Value = serde_json::from_slice(&listed.stdout).expect("the list is JSON");
    assert_eq!(board["tickets"][0]["title"], "Kept");
}
