//! The session record's identity, end to end (PRT-29): a record is Tandem's,
//! the harness session is a foreign pointer, and `session show` answers both the
//! pointer and the transport it came from.
//!
//! The external opencode path records through the environment it always did, the
//! hand-driven path names its record and no harness at all, and usage arrives
//! from the harness event stream at `session end` rather than from a scrape of
//! another application's database.

use std::fs;
use std::path::{Path, PathBuf};
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
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home())
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env_remove("TANDEM_HOME")
            .env_remove("OPENCODE_SESSION_ID");
        if let Some(harness) = harness {
            command.env("OPENCODE_SESSION_ID", harness);
        }
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

fn events(path: &Path) {
    fs::write(
        path,
        [
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_1","sessionID":"ses_events","role":"assistant","cost":0.25,"tokens":{"input":1000,"output":200,"reasoning":50,"cache":{"read":100,"write":10}},"providerID":"opencode-go","modelID":"deepseek-v4.1-flash"}}}"#,
            r#"{"type":"message.updated","properties":{"info":{"id":"msg_2","sessionID":"ses_events","role":"assistant","cost":0.25,"tokens":{"input":500,"output":100,"reasoning":0,"cache":{"read":0,"write":0}},"providerID":"opencode-go","modelID":"deepseek-v4.1-flash"}}}"#,
        ]
        .join("\n"),
    )
    .expect("the event stream is written");
}

#[test]
fn a_harness_session_is_a_foreign_pointer_show_answers_and_end_reads_the_stream() {
    let sandbox = Sandbox::new("harness-pointer");
    let path = sandbox.root.join("events.jsonl");
    events(&path);

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

    let ended = sandbox.json(&[
        "session",
        "end",
        "--session",
        "ses_pass_1",
        "--outcome",
        "succeeded",
        "--summary",
        "What landed.",
        "--usage-from-harness",
        "--harness-events",
        path.to_str().expect("a UTF-8 path"),
    ]);
    assert_eq!(ended["status"], "ended");
    assert_eq!(
        ended["cost"], 0.5,
        "the stream's cost, summed over messages"
    );
    assert_eq!(ended["tokens"]["input"], 1500);
    assert_eq!(ended["tokens"]["output"], 300);
    assert_eq!(ended["model"], "deepseek-v4.1-flash");
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
