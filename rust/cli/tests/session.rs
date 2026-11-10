//! The session record's identity, end to end (PRT-29): a record is Tandem's,
//! the harness session is a foreign pointer, and `session show` answers both the
//! pointer and the transport it came from.
//!
//! The external opencode path records through the environment it always did, the
//! hand-driven path names its record and no harness at all, and usage arrives
//! over the harness loopback API at `session end` rather than from a scrape of
//! another application's database.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-session-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("Home")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, None)
    }

    /// A run with the environment naming the harness session, as the external
    /// opencode host exports it.
    fn run_in_harness(&self, args: &[&str]) -> Output {
        self.run_with(args, Some("ses_harness"))
    }

    fn run_with(&self, args: &[&str], harness: Option<&str>) -> Output {
        self.run_with_env(args, harness, &[])
    }

    fn run_with_env(&self, args: &[&str], harness: Option<&str>, extra: &[(&str, &str)]) -> Output {
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home())
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env_remove("TANDEM_HOME")
            .env_remove("OPENCODE_SESSION_ID")
            .env_remove("TANDEM_HARNESS_URL")
            .env_remove("TANDEM_HARNESS_PASSWORD")
            .env_remove("XDG_STATE_HOME")
            .env_remove("OPENCODE_DB");
        if let Some(harness) = harness {
            command.env("OPENCODE_SESSION_ID", harness);
        }
        for (key, value) in extra {
            command.env(key, value);
        }
        // The old SQL path is gone: a database at OPENCODE_DB must never be
        // read. Point it at a missing file so a regression answers a failure,
        // not the usage the stub would have answered.
        command.env("OPENCODE_DB", self.root.join("missing-opencode.db"));
        command.output().expect("the binary runs")
    }

    fn json(&self, args: &[&str]) -> Value {
        json_of(self.run(args), args)
    }

    fn harness_json(&self, args: &[&str]) -> Value {
        json_of(self.run_in_harness(args), args)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn json_of(output: Output, args: &[&str]) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("the verb answers JSON")
}

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

struct Stub {
    base: String,
}

impl Stub {
    fn start(password: &str, session: &str, messages: &str) -> (Self, thread::JoinHandle<()>) {
        Self::start_hits(
            "ses_events",
            password,
            vec![session.to_string()],
            messages,
            2,
        )
    }

    fn start_one(password: &str, session: &str, messages: &str) -> (Self, thread::JoinHandle<()>) {
        Self::start_hits(
            "ses_events",
            password,
            vec![session.to_string()],
            messages,
            1,
        )
    }

    /// A stub whose harness session grows between passes: each `session end`
    /// reads the next snapshot, in order, as one harness session's cumulative
    /// total does.
    fn start_snapshots(
        expected_id: &str,
        password: &str,
        snapshots: Vec<String>,
        messages: &str,
    ) -> (Self, thread::JoinHandle<()>) {
        let hits = snapshots.len() * 2; // one session read and one message page per end
        Self::start_hits(expected_id, password, snapshots, messages, hits)
    }

    fn start_hits(
        expected_id: &str,
        password: &str,
        sessions: Vec<String>,
        messages: &str,
        hits: usize,
    ) -> (Self, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the stub binds loopback");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let expected = format!("Basic {}", basic(password));
        let messages = messages.to_string();
        let wanted = expected_id.to_string();
        let password_check = password.to_string();
        let base_clone = base.clone();
        let handle = thread::spawn(move || {
            let mut session_read = 0usize;
            for stream in listener.incoming().take(hits) {
                let mut stream = stream.expect("a harness connection arrives");
                let mut reader = BufReader::new(stream.try_clone().expect("the stub clones"));
                let mut headline = String::new();
                reader
                    .read_line(&mut headline)
                    .expect("the request line arrives");
                let mut length = 0usize;
                let mut auth = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("headers arrive");
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(colon) = trimmed.find(':') {
                        let (name, value) = trimmed.split_at(colon);
                        if name.eq_ignore_ascii_case("authorization") {
                            auth = value[1..].trim().to_string();
                        }
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value[1..].trim().parse().unwrap_or(0);
                        }
                    }
                }
                if length > 0 {
                    let mut discard = vec![0u8; length];
                    reader.read_exact(&mut discard).ok();
                }
                let path = headline
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let authorized = auth == expected;
                let (status, body) = if !authorized {
                    (
                        401,
                        r#"{"_tag":"UnauthorizedError","message":"Authentication required"}"#
                            .to_string(),
                    )
                } else if path.contains("/message") {
                    if path.contains(wanted.as_str()) {
                        if path.contains("/message?limit=100&cursor=") {
                            (200, r#"{"data":[],"cursor":{"next":null}}"#.to_string())
                        } else {
                            (200, messages.clone())
                        }
                    } else {
                        (
                            404,
                            r#"{"_tag":"SessionNotFoundError","message":"Session not found"}"#
                                .to_string(),
                        )
                    }
                } else if path.contains("/api/session/") {
                    if path.contains(wanted.as_str()) {
                        let body = sessions.get(session_read).cloned().unwrap_or_default();
                        session_read += 1;
                        (200, body)
                    } else {
                        (
                            404,
                            r#"{"_tag":"SessionNotFoundError","message":"Session not found"}"#
                                .to_string(),
                        )
                    }
                } else {
                    (
                        404,
                        r#"{"_tag":"SessionNotFoundError","message":"Session not found"}"#
                            .to_string(),
                    )
                };
                let _ = password_check.clone();
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    _ => "Not Found",
                };
                let reply = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(reply.as_bytes())
                    .expect("the stub answers");
            }
        });
        (Self { base: base_clone }, handle)
    }
}

fn basic(password: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = format!("opencode:{password}").into_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let a = bytes[i] as u32;
        let b = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let c = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if i + 1 < bytes.len() {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < bytes.len() {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

fn session_body() -> String {
    r#"{"data":{"id":"ses_events","tokens":{"input":1500,"output":300,"reasoning":50,"cache":{"read":100,"write":10}},"cost":0.5,"model":{"id":"deepseek-v4.1-flash"}}}"#.to_string()
}

fn messages_body() -> String {
    r#"{"data":[{"id":"msg_1","content":[{"type":"tool","id":"prt_1","name":"skill","state":{"input":{"id":"opencode"}}}]},{"id":"msg_2","content":[{"type":"tool","id":"prt_2","name":"skill","state":{"input":{"id":"tdd"}}},{"type":"tool","id":"prt_3","name":"shell","state":{"input":{"command":"ls"}}}]},{"id":"msg_3","type":"assistant","content":[{"type":"tool","id":"prt_4","name":"shell","state":{"input":{"command":"ls"}}},{"type":"tool","id":"prt_5","name":"read","state":{"input":{"filePath":"a"}}}]},{"id":"msg_4","type":"assistant","content":[{"type":"text","id":"prt_6","text":"done"}]},{"id":"msg_5","type":"user","content":[{"type":"text","id":"prt_7","text":"go"}]}],"cursor":{"next":null}}"#.to_string()
}

#[test]
fn a_harness_session_is_a_foreign_pointer_show_answers_and_end_reads_the_api() {
    let sandbox = Sandbox::new("harness-pointer");
    let (stub, handle) = Stub::start("harness-secret", &session_body(), &messages_body());

    let started = sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_pass_1",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass in the embedded harness.",
    ]);
    assert_eq!(started["id"], "ses_pass_1");
    assert_eq!(started["harnessSessionId"], "ses_events");
    assert_eq!(started["harnessTransport"], "event-stream");

    let shown = sandbox.json(&["session", "show", "--session", "ses_pass_1"]);
    assert_eq!(
        shown["harnessSessionId"], "ses_events",
        "show answers the harness id"
    );
    assert_eq!(
        shown["harnessTransport"], "event-stream",
        "and the transport it came from"
    );

    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_pass_1",
            "--outcome",
            "succeeded",
            "--summary",
            "What landed.",
            "--usage-from-harness",
            "--harness-url",
            &stub.base,
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
    );
    let ended: Value = {
        assert_eq!(
            output.status.code(),
            Some(0),
            "end reads the loopback: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("the verb answers JSON")
    };
    assert_eq!(ended["status"], "ended");
    assert_eq!(ended["cost"], 0.5, "the API cost");
    assert_eq!(ended["tokens"]["input"], 1500);
    assert_eq!(ended["tokens"]["output"], 300);
    assert_eq!(ended["model"], "deepseek-v4.1-flash");
    assert_eq!(ended["skills"]["opencode"], 1);
    assert_eq!(ended["skills"]["tdd"], 1);
    handle.join().expect("the stub ends");
}

#[test]
fn ending_from_the_harness_stamps_the_turns_and_calls_it_counted() {
    let sandbox = Sandbox::new("harness-counts");
    let (stub, handle) = Stub::start("harness-secret", &session_body(), &messages_body());

    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_pass_1",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass in the embedded harness.",
    ]);

    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_pass_1",
            "--outcome",
            "succeeded",
            "--usage-from-harness",
            "--harness-url",
            &stub.base,
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
    );
    let ended = json_of(output, &["session", "end"]);
    assert_eq!(ended["turns"], 2, "two assistant messages are two turns");
    assert_eq!(ended["calls"], 2, "their tool parts are the calls");
    handle.join().expect("the stub ends");
}

#[test]
fn the_usage_comment_carries_the_counts_and_the_ratio() {
    let sandbox = Sandbox::new("harness-comment");
    let (stub, handle) = Stub::start("harness-secret", &session_body(), &messages_body());

    let added = sandbox.json(&["ticket", "add", "--title", "Carried"]);
    let ticket = added["id"].as_str().expect("an id").to_string();
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_pass_1",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass in the embedded harness.",
    ]);
    sandbox.json(&[
        "session",
        "link",
        "--session",
        "ses_pass_1",
        "--ticket",
        &ticket,
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_pass_1",
            "--outcome",
            "succeeded",
            "--usage-from-harness",
            "--harness-url",
            &stub.base,
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
    );
    json_of(output, &["session", "end"]);

    let comments = sandbox.json(&["ticket", "comment", "list", "--id", &ticket]);
    let bodies: Vec<&str> = comments["comments"]
        .as_array()
        .expect("comments")
        .iter()
        .filter_map(|comment| comment["body"].as_str())
        .collect();
    let body = bodies
        .iter()
        .find(|body| body.starts_with("Usage"))
        .unwrap_or_else(|| panic!("the usage comment posts: {bodies:?}"));
    assert!(
        body.contains("calls 2"),
        "the comment carries the calls: {body}"
    );
    assert!(
        body.contains("turns 2"),
        "the comment carries the turns: {body}"
    );
    assert!(
        body.contains("per turn 1.00"),
        "the comment carries the derived ratio: {body}"
    );
    handle.join().expect("the stub ends");
}

#[test]
fn ending_with_no_harness_read_carries_no_counts() {
    let sandbox = Sandbox::new("no-harness-counts");

    let added = sandbox.json(&["ticket", "add", "--title", "Carried"]);
    let ticket = added["id"].as_str().expect("an id").to_string();
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_hand",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A hand-driven pass.",
    ]);
    sandbox.json(&[
        "session",
        "link",
        "--session",
        "ses_hand",
        "--ticket",
        &ticket,
    ]);
    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_hand",
        "--outcome",
        "succeeded",
        "--cost",
        "0.5",
    ]);
    assert!(
        ended.get("turns").is_none() && ended.get("calls").is_none(),
        "no harness read stamps neither count: {ended}"
    );

    let comments = sandbox.json(&["ticket", "comment", "list", "--id", &ticket]);
    let bodies: Vec<&str> = comments["comments"]
        .as_array()
        .expect("comments")
        .iter()
        .filter_map(|comment| comment["body"].as_str())
        .collect();
    let body = bodies
        .iter()
        .find(|body| body.starts_with("Usage"))
        .unwrap_or_else(|| panic!("the usage comment posts: {bodies:?}"));
    assert!(
        !body.contains("calls ") && !body.contains("turns ") && !body.contains("per turn"),
        "no counts, no lines: {body}"
    );
}

#[test]
fn an_explicit_url_wins_over_the_environment_and_the_rendezvous() {
    let sandbox = Sandbox::new("url-precedence");
    let (good, good_handle) = Stub::start("harness-secret", &session_body(), &messages_body());
    let (decoy, decoy_handle) = Stub::start_hits(
        "ses_events",
        "harness-secret",
        vec![session_body()],
        &messages_body(),
        0,
    );
    // A rendezvous file the explicit URL must beat: it points at the decoy,
    // so a read through it would still answer, but with the decoy's port.
    let rendezvous = sandbox
        .home()
        .join("harness")
        .join("state")
        .join("opencode");
    std::fs::create_dir_all(&rendezvous).expect("the rendezvous dir is creatable");
    std::fs::write(
        rendezvous.join("service.json"),
        format!(r#"{{"url":"{}","password":"harness-secret"}}"#, decoy.base),
    )
    .expect("the rendezvous is writable");

    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_url",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_url",
            "--summary",
            "What landed.",
            "--usage-from-harness",
            "--harness-url",
            &good.base,
        ],
        None,
        &[
            ("TANDEM_HARNESS_URL", decoy.base.as_str()),
            ("TANDEM_HARNESS_PASSWORD", "harness-secret"),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the flag wins: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    good_handle.join().expect("the good stub ends");
    decoy_handle
        .join()
        .expect("the decoy is never read, so it still ends");
}

#[test]
fn the_environment_url_wins_over_the_rendezvous_file() {
    let sandbox = Sandbox::new("env-wins");
    let (good, good_handle) = Stub::start("harness-secret", &session_body(), &messages_body());
    let (decoy, decoy_handle) = Stub::start_hits(
        "ses_events",
        "harness-secret",
        vec![session_body()],
        &messages_body(),
        0,
    );
    let rendezvous = sandbox
        .home()
        .join("harness")
        .join("state")
        .join("opencode");
    std::fs::create_dir_all(&rendezvous).expect("the rendezvous dir is creatable");
    std::fs::write(
        rendezvous.join("service.json"),
        format!(r#"{{"url":"{}","password":"harness-secret"}}"#, decoy.base),
    )
    .expect("the rendezvous is writable");

    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_env",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_env",
            "--summary",
            "What landed.",
            "--usage-from-harness",
        ],
        None,
        &[
            ("TANDEM_HARNESS_URL", good.base.as_str()),
            ("TANDEM_HARNESS_PASSWORD", "harness-secret"),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the env wins: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    good_handle.join().expect("the good stub ends");
    decoy_handle
        .join()
        .expect("the decoy is never read, so it still ends");
}

#[test]
fn the_rendezvous_file_answers_when_no_url_is_given() {
    let sandbox = Sandbox::new("rendezvous");
    let (stub, handle) = Stub::start("file-secret", &session_body(), &messages_body());
    let rendezvous = sandbox
        .home()
        .join("harness")
        .join("state")
        .join("opencode");
    std::fs::create_dir_all(&rendezvous).expect("the rendezvous dir is creatable");
    std::fs::write(
        rendezvous.join("service.json"),
        format!(
            r#"{{"url":"{}","password":"file-secret","extra":"ignored"}}"#,
            stub.base
        ),
    )
    .expect("the rendezvous is writable");

    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_file",
        "--harness-session",
        "ses_events",
        "--harness-transport",
        "event-stream",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_file",
            "--summary",
            "What landed.",
            "--usage-from-harness",
        ],
        None,
        &[],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the rendezvous answers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ended: Value = serde_json::from_slice(&output.stdout).expect("the verb answers JSON");
    assert_eq!(ended["model"], "deepseek-v4.1-flash");
    handle.join().expect("the stub ends");
}

#[test]
fn a_wrong_password_names_the_password_sources_and_leaks_nothing() {
    let sandbox = Sandbox::new("wrong-password");
    let (stub, handle) = Stub::start_one("harness-secret", &session_body(), &messages_body());
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_pw",
        "--harness-session",
        "ses_events",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_pw",
            "--summary",
            "What landed.",
            "--usage-from-harness",
            "--harness-url",
            &stub.base,
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "wrong-secret")],
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refused the password"),
        "the failure names the password: {stderr}"
    );
    assert!(
        stderr.contains("TANDEM_HARNESS_PASSWORD"),
        "and where it came from: {stderr}"
    );
    assert!(
        !stderr.contains("wrong-secret") && !stderr.contains("harness-secret"),
        "no password leaks: {stderr}"
    );
    handle.join().expect("the stub ends");
}

#[test]
fn an_unknown_harness_session_names_the_id() {
    let sandbox = Sandbox::new("unknown-session");
    let (stub, handle) = Stub::start_one("harness-secret", &session_body(), &messages_body());
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_gone",
        "--harness-session",
        "ses_missing",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_gone",
            "--summary",
            "What landed.",
            "--usage-from-harness",
            "--harness-url",
            &stub.base,
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ses_missing"),
        "the failure names the id: {stderr}"
    );
    handle.join().expect("the stub ends");
}

#[test]
fn an_unreachable_harness_names_the_flag_and_the_env() {
    let sandbox = Sandbox::new("unreachable");
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_down",
        "--harness-session",
        "ses_events",
        "--project",
        "tandem",
        "--brief",
        "A pass.",
    ]);
    let output = sandbox.run_with_env(
        &[
            "session",
            "end",
            "--session",
            "ses_down",
            "--summary",
            "What landed.",
            "--usage-from-harness",
            "--harness-url",
            "http://127.0.0.1:1",
        ],
        None,
        &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the failure names what was tried: {stderr}"
    );
    assert!(
        stderr.contains("--harness-url") && stderr.contains("TANDEM_HARNESS_URL"),
        "and how to override it: {stderr}"
    );
}

#[test]
fn a_hand_driven_run_records_with_no_harness_id() {
    let sandbox = Sandbox::new("hand-driven");

    let started = sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_hand",
        "--project",
        "tandem",
        "--brief",
        "A hand-driven pass.",
    ]);
    assert_eq!(started["id"], "ses_hand");
    assert!(
        started.get("harnessSessionId").is_none(),
        "no harness id is absent, not null: {started}"
    );
    assert!(started.get("harnessTransport").is_none());

    let shown = sandbox.json(&["session", "show", "--session", "ses_hand"]);
    assert_eq!(shown["status"], "running");
    assert!(shown.get("harnessSessionId").is_none());
    assert!(shown.get("harnessTransport").is_none());
}

#[test]
fn two_passes_in_one_harness_session_each_record_and_show_answers_the_newest() {
    let sandbox = Sandbox::new("two-passes");

    let first = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "The first pass.",
    ]);
    let second = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "refiner",
        "--brief",
        "The second pass.",
    ]);

    assert_ne!(
        first["id"], second["id"],
        "each pass opens a record of its own"
    );
    assert_eq!(first["harnessSessionId"], "ses_harness");
    assert_eq!(second["harnessSessionId"], "ses_harness");
    assert_eq!(first["harnessTransport"], "database");
    assert_eq!(second["harnessTransport"], "database");

    let listed = sandbox.harness_json(&["session", "list"]);
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 2);

    let newest = sandbox.harness_json(&["session", "show"]);
    assert_eq!(
        newest["id"], second["id"],
        "the harness pointer answers the newest record that names it"
    );
    assert_eq!(newest["agent"], "refiner");

    let first_id = first["id"].as_str().unwrap().to_string();
    let first = sandbox.json(&["session", "show", "--session", &first_id]);
    assert_eq!(
        first["agent"], "builder",
        "the first record is not overwritten"
    );
    assert_eq!(first["harnessSessionId"], "ses_harness");

    // The external wrapper's end, by the harness id it has always spoken,
    // reaches the newest record through the same column.
    let ended = sandbox.harness_json(&[
        "session",
        "end",
        "--session",
        "ses_harness",
        "--outcome",
        "succeeded",
        "--summary",
        "What landed.",
    ]);
    assert_eq!(ended["id"], second["id"]);
    assert_eq!(ended["status"], "ended");
}

#[test]
fn the_environment_keys_an_end_to_the_newest_record_on_its_pointer() {
    let sandbox = Sandbox::new("env-keyed-end");

    let first = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "The first pass.",
    ]);
    let second = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "refiner",
        "--brief",
        "The second pass.",
    ]);
    assert_ne!(
        first["id"], second["id"],
        "each pass opens a record of its own on one pointer"
    );

    // The host exports the harness id and names no --session: the end resolves
    // the environment's pointer to the record that opened last.
    let ended = sandbox.harness_json(&[
        "session",
        "end",
        "--outcome",
        "succeeded",
        "--summary",
        "What landed.",
    ]);
    assert_eq!(
        ended["id"], second["id"],
        "the env-keyed end stamps the newest record naming the pointer"
    );
    assert_eq!(ended["status"], "ended");

    let first_id = first["id"].as_str().unwrap();
    let first = sandbox.json(&["session", "show", "--session", first_id]);
    assert_eq!(
        first["status"], "running",
        "the older record on the pointer is not stamped"
    );
}

#[test]
fn a_run_with_no_session_and_no_harness_id_still_records() {
    let sandbox = Sandbox::new("minted");

    let started = sandbox.json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A hand-driven pass.",
    ]);
    assert!(
        started["id"].as_str().is_some_and(|id| !id.is_empty()),
        "Tandem mints the handle: {started}"
    );
    assert!(started.get("harnessSessionId").is_none());
    assert!(started.get("harnessTransport").is_none());
    assert_eq!(
        sandbox.json(&["session", "list"])["sessions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn ending_with_a_skills_flag_round_trips_through_show() {
    let sandbox = Sandbox::new("skills-round-trip");
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_skills",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass that loaded two skills.",
    ]);

    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_skills",
        "--summary",
        "What landed.",
        "--skills",
        "grilling=2,tandem-memory=1",
    ]);
    assert_eq!(ended["skills"]["grilling"], 2);
    assert_eq!(ended["skills"]["tandem-memory"], 1);

    let shown = sandbox.json(&["session", "show", "--session", "ses_skills"]);
    assert_eq!(
        shown["skills"], ended["skills"],
        "show reads back the stamped map: {shown}"
    );
}

#[test]
fn a_malformed_skills_flag_is_a_domain_failure_and_writes_nothing() {
    let sandbox = Sandbox::new("skills-malformed");
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_skills_bad",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass with a mistyped flag.",
    ]);

    for bad in [
        "grilling",
        "=2",
        "grilling=x",
        "grilling=1,grilling=2",
        "a=1,",
    ] {
        let output = sandbox.run(&[
            "session",
            "end",
            "--session",
            "ses_skills_bad",
            "--summary",
            "What landed.",
            "--skills",
            bad,
        ]);
        assert_eq!(output.status.code(), Some(1), "a domain failure is exit 1");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--skills must be comma-separated name=count pairs"),
            "the failure names the flag for {bad:?}: {stderr}"
        );
        assert!(
            stderr.contains(&format!("(got \"{bad}\")")),
            "the failure names the value for {bad:?}: {stderr}"
        );
    }

    let shown = sandbox.json(&["session", "show", "--session", "ses_skills_bad"]);
    assert_eq!(
        shown["status"], "running",
        "the failed ends wrote nothing: {shown}"
    );

    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_skills_bad",
        "--summary",
        "What landed.",
        "--skills",
        "grilling=1",
    ]);
    assert_eq!(ended["skills"]["grilling"], 1);
    assert_eq!(ended["status"], "ended");
}

#[test]
fn an_end_without_a_skills_flag_carries_no_skills_key() {
    let sandbox = Sandbox::new("skills-absent");
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_no_skills",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass that loaded no skill.",
    ]);
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_empty_skills",
        "--project",
        "tandem",
        "--agent",
        "builder",
        "--brief",
        "A pass with an empty flag.",
    ]);

    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_no_skills",
        "--summary",
        "What landed.",
    ]);
    assert!(
        ended.get("skills").is_none(),
        "no flag is an absent key, not a map: {ended}"
    );
    let shown = sandbox.json(&["session", "show", "--session", "ses_no_skills"]);
    assert!(
        shown.get("skills").is_none(),
        "show prints no skills key either: {shown}"
    );

    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_empty_skills",
        "--summary",
        "What landed.",
        "--skills",
        "",
    ]);
    assert!(
        ended.get("skills").is_none(),
        "an empty flag is absent too: {ended}"
    );
    let shown = sandbox.json(&["session", "show", "--session", "ses_empty_skills"]);
    assert!(
        shown.get("skills").is_none(),
        "show prints no skills key either: {shown}"
    );
}

#[test]
fn ending_with_usage_from_harness_without_a_pointer_names_the_failure() {
    let sandbox = Sandbox::new("no-pointer");
    sandbox.json(&[
        "session",
        "start",
        "--session",
        "ses_hand",
        "--brief",
        "A hand-driven pass.",
    ]);

    let output = sandbox.run(&[
        "session",
        "end",
        "--session",
        "ses_hand",
        "--summary",
        "What landed.",
        "--usage-from-harness",
    ]);
    assert_eq!(output.status.code(), Some(1), "a domain failure is exit 1");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("harness session"),
        "the failure names the missing pointer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A re-dispatched pass opens its own record under the same harness pointer
/// (ADR 0027), so two records share one harness session's cumulative total. The
/// second pass's `usage` comment states its own share, and the record keeps the
/// snapshot the harness answered.
#[test]
fn a_second_pass_in_one_harness_session_posts_its_own_share() {
    let sandbox = Sandbox::new("shared-harness-share");
    let snapshots = vec![
        r#"{"data":{"id":"ses_harness","tokens":{"input":1500},"cost":0.5,"model":{"id":"deepseek-v4.1-flash"}}}"#.to_string(),
        r#"{"data":{"id":"ses_harness","tokens":{"input":2000},"cost":1.0,"model":{"id":"deepseek-v4.1-flash"}}}"#.to_string(),
    ];
    let (stub, handle) =
        Stub::start_snapshots("ses_harness", "harness-secret", snapshots, &messages_body());

    let first = sandbox.json(&["ticket", "add", "--title", "The first pass's ticket"]);
    let second = sandbox.json(&["ticket", "add", "--title", "The second pass's ticket"]);
    let first_ticket = first["id"].as_str().expect("the first ticket's id");
    let second_ticket = second["id"].as_str().expect("the second ticket's id");

    for (record, ticket) in [("ses_pass_1", first_ticket), ("ses_pass_2", second_ticket)] {
        sandbox.json(&[
            "session",
            "start",
            "--session",
            record,
            "--harness-session",
            "ses_harness",
            "--harness-transport",
            "event-stream",
            "--project",
            "tandem",
            "--agent",
            "builder",
            "--brief",
            "A pass.",
        ]);
        sandbox.json(&["session", "link", "--session", record, "--ticket", ticket]);
    }

    let end = |record: &str| -> Value {
        let output = sandbox.run_with_env(
            &[
                "session",
                "end",
                "--session",
                record,
                "--outcome",
                "succeeded",
                "--summary",
                "What landed.",
                "--usage-from-harness",
                "--harness-url",
                &stub.base,
            ],
            None,
            &[("TANDEM_HARNESS_PASSWORD", "harness-secret")],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "end reads the loopback: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("the verb answers JSON")
    };

    let first_end = end("ses_pass_1");
    assert_eq!(first_end["tokens"]["input"], 1500, "the first snapshot");
    let second_end = end("ses_pass_2");
    assert_eq!(
        second_end["tokens"]["input"], 2000,
        "the record keeps the snapshot it stamped"
    );

    let comments = sandbox.json(&["ticket", "comment", "list", "--id", second_ticket]);
    let body = comments["comments"][0]["body"]
        .as_str()
        .expect("the second ticket carries one usage comment");
    assert!(
        body.contains("input 500"),
        "the second pass's share, not its 2,000 snapshot: {body}"
    );
    assert!(!body.contains("input 2,000"), "not the snapshot: {body}");

    let shown = sandbox.json(&["session", "show", "--session", "ses_pass_2"]);
    assert_eq!(
        shown["tokens"]["input"], 2000,
        "show still answers the harness's snapshot"
    );
    handle.join().expect("the stub ends");
}
