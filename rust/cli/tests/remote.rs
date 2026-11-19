//! The remote CLI over a real socket (ADR 0047): with `TANDEM_SERVER` set, a
//! verb runs against a spawned `tandem serve` and prints what the local verb
//! prints.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-remote-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self, which: &str) -> PathBuf {
        self.root.join(which)
    }

    /// The CLI against `home`, optionally remote.
    fn cli(&self, home: &str, server: Option<&str>, token: Option<&str>) -> Command {
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home(home))
            .env("HOME", self.root.join("user"))
            .env_remove("TANDEM_AGENT")
            .env_remove("TANDEM_SERVER")
            .env_remove("TANDEM_TOKEN")
            .stdin(Stdio::null());
        if let Some(server) = server {
            command.env("TANDEM_SERVER", server);
        }
        if let Some(token) = token {
            command.env("TANDEM_TOKEN", token);
        }
        command
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Server {
    child: Child,
    url: String,
}

impl Server {
    fn start(sandbox: &Sandbox, home: &str, host: &str, token: Option<&str>) -> Self {
        let mut command = sandbox.cli(home, None, token);
        let mut child = command
            .args(["serve", "--port", "0", "--host", host])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the server starts");
        let mut output = BufReader::new(child.stdout.take().expect("stdout is piped"));
        let mut line = String::new();
        output
            .read_line(&mut line)
            .expect("the listening line reads");
        thread::spawn(move || for _ in output.lines() {});
        let port = line
            .trim()
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or_else(|| panic!("the address carries a port: {line:?}"));
        Self {
            child,
            url: format!("http://127.0.0.1:{port}"),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn stdout_of(mut command: Command) -> (Option<i32>, String, String) {
    let output = command.output().expect("the CLI runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// A ticket's volatile fields (id, ref number order aside, timestamps) blanked,
/// so a remote and a local run of the same verb compare on shape and content.
fn normalized(text: &str) -> Value {
    fn walk(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map.iter_mut() {
                    if matches!(
                        key.as_str(),
                        "id" | "createdAt" | "updatedAt" | "completedAt" | "at"
                    ) {
                        *inner = Value::Null;
                    } else {
                        walk(inner);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|error| panic!("stdout is one JSON value ({error}): {text}"));
    walk(&mut value);
    value
}

#[test]
fn remote_verbs_print_what_the_local_verbs_print() {
    let sandbox = Sandbox::new("parity");
    let server = Server::start(&sandbox, "served", "127.0.0.1", None);

    for args in [
        &["ticket", "add", "--title", "Over the wire"][..],
        &["ticket", "list"][..],
        &["ticket", "add", "--title", "Second", "--status", "ready"][..],
    ] {
        let (code, remote, stderr) = stdout_of({
            let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
            command.args(args);
            command
        });
        assert_eq!(code, Some(0), "the remote verb succeeds: {stderr}");
        let (local_code, local, _) = stdout_of({
            let mut command = sandbox.cli("mirror", None, None);
            command.args(args);
            command
        });
        assert_eq!(local_code, Some(0));
        if args[1] != "list" {
            assert_eq!(normalized(&remote), normalized(&local), "{args:?}");
        }
    }

    // The remote list reads the served home, not the CLI's own.
    let (_, listed, _) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
        command.args(["ticket", "list"]);
        command
    });
    let listed: Value = serde_json::from_str(listed.trim()).expect("stdout is JSON");
    let titles: Vec<&str> = listed["tickets"]
        .as_array()
        .expect("a ticket list")
        .iter()
        .filter_map(|ticket| ticket["title"].as_str())
        .collect();
    assert_eq!(titles, ["Second", "Over the wire"]);
    assert!(
        listed.get("revision").is_none(),
        "the local verb prints no revision"
    );
    assert!(
        !sandbox.home("elsewhere").join("tandem.db").exists(),
        "the remote run opened no local store"
    );

    let id = listed["tickets"][1]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let (code, shown, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
        command.args(["ticket", "show", "--id", &id]);
        command
    });
    assert_eq!(code, Some(0), "{stderr}");
    let shown: Value = serde_json::from_str(shown.trim()).expect("stdout is JSON");
    assert_eq!(
        shown["title"], "Over the wire",
        "show prints the ticket whole"
    );

    let (code, commented, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
        command.env("TANDEM_AGENT", "builder");
        command.args([
            "ticket", "comment", "add", "--id", &id, "--author", "builder", "--body", "Landed",
        ]);
        command
    });
    assert_eq!(code, Some(0), "{stderr}");
    let commented: Value = serde_json::from_str(commented.trim()).expect("stdout is JSON");
    assert_eq!(
        commented["body"], "Landed",
        "the flat comment the local verb prints"
    );
}

#[test]
fn a_refusal_exits_non_zero_in_the_servers_words() {
    let sandbox = Sandbox::new("refusal");
    let server = Server::start(&sandbox, "served", "127.0.0.1", None);
    let (code, stdout, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
        command.args(["ticket", "show", "--id", "no-such-ticket"]);
        command
    });
    assert_eq!(code, Some(1));
    assert!(stdout.is_empty());
    assert!(!stderr.trim().is_empty(), "the refusal is named");
}

#[test]
fn a_wrong_token_exits_non_zero_naming_the_refusal_and_never_the_token() {
    let sandbox = Sandbox::new("wrong-token");
    let good = "g".repeat(32);
    let server = Server::start(&sandbox, "served", "0.0.0.0", Some(&good));
    let wrong = "w".repeat(32);
    let (code, _, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), Some(&wrong));
        command.args(["ticket", "list"]);
        command
    });
    assert_ne!(code, Some(0));
    assert!(stderr.contains("refused the token"), "{stderr}");
    assert!(
        !stderr.contains(&wrong) && !stderr.contains(&good),
        "{stderr}"
    );

    let (code, _, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some(&server.url), Some(&good));
        command.args(["ticket", "list"]);
        command
    });
    assert_eq!(code, Some(0), "the right token passes: {stderr}");
}

#[test]
fn an_http_non_loopback_url_with_a_token_is_refused_before_any_request() {
    let sandbox = Sandbox::new("clear-token");
    let token = "c".repeat(32);
    // Nothing listens there: the refusal must come before a connection.
    let (code, _, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some("http://board.invalid:7700"), Some(&token));
        command.args(["ticket", "list"]);
        command
    });
    assert_eq!(code, Some(1));
    assert!(stderr.contains("plain http"), "{stderr}");
    assert!(!stderr.contains(&token), "{stderr}");
}

#[test]
fn a_verb_with_no_tool_refuses_in_remote_mode_by_name() {
    let sandbox = Sandbox::new("no-tool");
    let (code, _, stderr) = stdout_of({
        let mut command = sandbox.cli("elsewhere", Some("http://127.0.0.1:1"), None);
        command.args(["note", "stats"]);
        command
    });
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no remote form"), "{stderr}");
}

#[test]
fn export_and_import_refuse_by_name_under_a_remote_target() {
    let sandbox = Sandbox::new("no-export");
    for (verb, args) in [
        ("export", vec!["export", "--out", "x.tar.gz"]),
        ("import", vec!["import", "x.tar.gz"]),
    ] {
        let (code, _, stderr) = stdout_of({
            let mut command = sandbox.cli("elsewhere", Some("http://127.0.0.1:1"), None);
            command.args(&args);
            command
        });
        assert_eq!(code, Some(1), "{verb}");
        assert!(stderr.contains(&format!("tandem {verb}")), "{stderr}");
        assert!(stderr.contains("no remote form"), "{stderr}");
    }
}

/// MCP-3: the MCP door answers an agent the lean shape, so the remote CLI asks
/// for `detail: "full"` and its stdout must stay what the local verb prints.
/// The same script runs against a served home through the remote CLI and
/// against a mirror home locally; every verb's stdout must match, list and
/// archive included.
#[test]
fn remote_stdout_matches_local_stdout_for_the_verbs_with_a_lean_tool_answer() {
    let sandbox = Sandbox::new("lean-parity");
    let server = Server::start(&sandbox, "served", "127.0.0.1", None);
    let script: &[&[&str]] = &[
        &["label", "add", "--name", "epic", "--abbr", "LN"],
        &[
            "ticket",
            "add",
            "--title",
            "  First  ",
            "--label",
            "epic",
            "--description",
            "A brief\nwith two lines",
        ],
        &[
            "ticket",
            "add",
            "--title",
            "Unlabelled",
            "--assignee",
            "builder",
        ],
        &["ticket", "list"],
        &["ticket", "set", "--id", "LN-1", "--status", "ready"],
        &[
            "ticket",
            "set",
            "--id",
            "LN-1",
            "--title",
            "Renamed",
            "--description",
            "New brief",
        ],
        &["ticket", "block", "--id", "LN-1", "--on", "LN-1"],
        &["ticket", "list"],
        &["ticket", "list", "--filter", "all"],
        &[
            "ticket", "comment", "add", "--id", "LN-1", "--author", "builder", "--body", "Landed",
        ],
        &["ticket", "show", "--id", "LN-1"],
        &["ticket", "complete", "--id", "LN-1"],
        &["ticket", "archive", "--id", "LN-1"],
        &["ticket", "list", "--archived"],
        &["ticket", "archive"],
    ];
    for args in script {
        let (remote_code, remote, remote_err) = stdout_of({
            let mut command = sandbox.cli("elsewhere", Some(&server.url), None);
            command.args(*args);
            command
        });
        let (local_code, local, local_err) = stdout_of({
            let mut command = sandbox.cli("mirror", None, None);
            command.args(*args);
            command
        });
        assert_eq!(
            remote_code, local_code,
            "{args:?}: {remote_err} / {local_err}"
        );
        if remote.trim().is_empty() {
            assert_eq!(remote, local, "{args:?}");
            continue;
        }
        assert_eq!(normalized(&remote), normalized(&local), "{args:?}");
    }
}
