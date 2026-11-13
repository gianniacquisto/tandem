//! The sprint record's CLI door: the session pointer it stores, the batch
//! order it keeps and the refusal an absent record names.

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
fn sprint_open_without_a_flag_stores_the_environments_pointer() {
    let sandbox = Sandbox::new("pointer");

    let opened = sandbox.harness_json(&["sprint", "open", "--lane", "Tandem"]);
    assert_eq!(
        opened["session"].as_str(),
        Some("ses_harness"),
        "the sprint stores the pointer it opened under"
    );

    let shown = sandbox.json(&["sprint", "show", "--lane", "Tandem", "--number", "1"]);
    assert_eq!(shown["session"].as_str(), Some("ses_harness"));
}

#[test]
fn two_sprints_keep_their_own_session_pointers() {
    let sandbox = Sandbox::new("two-sprints");

    sandbox.json(&[
        "sprint",
        "open",
        "--lane",
        "Alpha",
        "--session",
        "ses_first",
    ]);
    sandbox.json(&[
        "sprint",
        "open",
        "--lane",
        "Beta",
        "--session",
        "ses_second",
    ]);

    let alpha = sandbox.json(&["sprint", "show", "--lane", "Alpha", "--number", "1"]);
    let beta = sandbox.json(&["sprint", "show", "--lane", "Beta", "--number", "1"]);
    assert_eq!(alpha["session"].as_str(), Some("ses_first"));
    assert_eq!(beta["session"].as_str(), Some("ses_second"));
}

#[test]
fn sprint_show_on_an_epic_lane_with_no_record_names_the_lane() {
    let sandbox = Sandbox::new("epic-show-missing");
    sandbox.json(&["lane", "add", "--name", "Tandem"]);
    sandbox.json(&["lane", "add", "--name", "ui"]);
    sandbox.json(&["lane", "add", "--name", "core"]);
    let output = sandbox.run_with(&["sprint", "show", "--lane", "ui", "--number", "1"], None);
    assert_ne!(
        output.status.code(),
        Some(0),
        "no record in the epic lane refuses"
    );
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("\"ui\""),
        "the refusal names the epic lane: {message}"
    );
    assert!(
        message.contains("No sprint is recorded") || message.contains("holds"),
        "the refusal names what the lane holds: {message}"
    );
}

#[test]
fn a_batch_ticket_lands_in_drawn_order() {
    let sandbox = Sandbox::new("batch-order");
    let first = sandbox.json(&["ticket", "add", "--title", "First"]);
    let second = sandbox.json(&["ticket", "add", "--title", "Second"]);
    let first_id = first["id"].as_str().unwrap().to_string();
    let second_id = second["id"].as_str().unwrap().to_string();

    sandbox.json(&["sprint", "open", "--lane", "Tandem"]);
    sandbox.json(&[
        "sprint", "ticket", "--lane", "Tandem", "--ticket", &first_id,
    ]);
    sandbox.json(&[
        "sprint", "ticket", "--lane", "Tandem", "--ticket", &second_id,
    ]);

    let shown = sandbox.json(&["sprint", "show", "--lane", "Tandem", "--number", "1"]);
    assert_eq!(
        shown["batch"],
        serde_json::json!([first_id, second_id]),
        "the batch keeps the order the tickets were drawn"
    );
}
