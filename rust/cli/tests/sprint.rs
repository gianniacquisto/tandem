//! `sprint open` names the record, not the pointer (TAN-249): the stored
//! session is a Tandem handle, so the dead-sprint rule reads an exact record
//! even when two passes share one harness session.

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
            "tandem-sprint-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("Home")
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
        json_of(self.run_with(args, None), args)
    }

    fn harness_json(&self, args: &[&str]) -> Value {
        json_of(self.run_with(args, Some("ses_harness")), args)
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

#[test]
fn sprint_open_without_session_stores_the_record_not_the_pointer() {
    let sandbox = Sandbox::new("record-not-pointer");

    let started = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "sprint",
        "--brief",
        "The sprint session.",
    ]);
    let record_id = started["id"].as_str().unwrap().to_string();
    assert_ne!(record_id, "ses_harness");

    let opened = sandbox.harness_json(&["sprint", "open", "--lane", "Tandem"]);
    let stored = opened["session"].as_str().unwrap().to_string();
    assert_eq!(
        stored, record_id,
        "the sprint names the session record, not $OPENCODE_SESSION_ID"
    );

    let shown = sandbox.json(&["sprint", "show", "--lane", "Tandem", "--number", "1"]);
    assert_eq!(
        shown["session"].as_str().unwrap(),
        record_id,
        "sprint show answers a Tandem handle, not an opencode id"
    );

    // The dead-sprint rule reads the stored handle exactly.
    let session = sandbox.json(&["session", "show", "--session", &stored]);
    assert_eq!(session["id"].as_str().unwrap(), record_id);
    assert_eq!(session["status"], "running");
}

#[test]
fn two_sprints_on_one_pointer_each_reach_their_own_record() {
    let sandbox = Sandbox::new("two-sprints-one-pointer");

    let first = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "sprint",
        "--brief",
        "The first pass.",
    ]);
    let second = sandbox.harness_json(&[
        "session",
        "start",
        "--project",
        "tandem",
        "--agent",
        "sprint",
        "--brief",
        "The second pass.",
    ]);
    let first_id = first["id"].as_str().unwrap().to_string();
    let second_id = second["id"].as_str().unwrap().to_string();
    assert_ne!(first_id, second_id);

    // Each sprint names its own pass's record, not the shared pointer.
    sandbox.json(&[
        "sprint",
        "open",
        "--lane",
        "Alpha",
        "--session",
        &first_id,
    ]);
    sandbox.json(&[
        "sprint",
        "open",
        "--lane",
        "Beta",
        "--session",
        &second_id,
    ]);

    // One pass ends; the dead-sprint rule must still reach each record's own
    // state through the handle the sprint stored.
    sandbox.json(&[
        "session",
        "end",
        "--session",
        &first_id,
        "--outcome",
        "succeeded",
        "--summary",
        "What landed.",
    ]);

    let alpha = sandbox.json(&["sprint", "show", "--lane", "Alpha", "--number", "1"]);
    let beta = sandbox.json(&["sprint", "show", "--lane", "Beta", "--number", "1"]);
    assert_eq!(alpha["session"].as_str().unwrap(), first_id);
    assert_eq!(beta["session"].as_str().unwrap(), second_id);

    let first_state = sandbox.json(&["session", "show", "--session", &first_id]);
    let second_state = sandbox.json(&["session", "show", "--session", &second_id]);
    assert_eq!(first_state["status"], "ended");
    assert_eq!(second_state["status"], "running");

    let via_alpha = sandbox.json(&[
        "session",
        "show",
        "--session",
        alpha["session"].as_str().unwrap(),
    ]);
    let via_beta = sandbox.json(&[
        "session",
        "show",
        "--session",
        beta["session"].as_str().unwrap(),
    ]);
    assert_eq!(via_alpha["id"].as_str().unwrap(), first_id);
    assert_eq!(via_alpha["status"], "ended");
    assert_eq!(via_beta["id"].as_str().unwrap(), second_id);
    assert_eq!(via_beta["status"], "running");
}
