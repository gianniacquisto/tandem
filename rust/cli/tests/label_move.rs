//! `label move` reorders the declared labels, the CLI half of the label journey.

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
            "tandem-label-move-{name}-{}-{serial}",
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
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home())
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env_remove("TANDEM_HOME")
            .env_remove("OPENCODE_SESSION_ID");
        command.output().expect("the binary runs")
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("the verb answers JSON")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn label_move_reorders_the_declared_labels() {
    let sandbox = Sandbox::new("reorder");
    sandbox.json(&["label", "add", "--name", "A"]);
    sandbox.json(&["label", "add", "--name", "B"]);
    sandbox.json(&["label", "add", "--name", "C"]);

    let moved = sandbox.json(&["label", "move", "--name", "C", "--index", "0"]);
    assert_eq!(
        moved["labels"],
        Value::Array(vec![
            Value::String("C".to_string()),
            Value::String("A".to_string()),
            Value::String("B".to_string())
        ]),
        "C moves to the front"
    );

    let listed = sandbox.json(&["label", "list"]);
    assert_eq!(
        listed["labels"], moved["labels"],
        "the list reads the new order"
    );
}

#[test]
fn label_move_names_an_unknown_label_and_the_fix() {
    let sandbox = Sandbox::new("unknown");
    sandbox.json(&["label", "add", "--name", "A"]);
    let output = sandbox.run(&["label", "move", "--name", "Nope", "--index", "0"]);
    assert_ne!(output.status.code(), Some(0), "an unknown label refuses");
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("\"Nope\""),
        "the refusal names the label: {message}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "the refusal names the fix: {message}"
    );

    // The epic labels use the same refusal at the CLI seam.
    let epic = Sandbox::new("unknown-epic");
    epic.json(&["label", "add", "--name", "Tandem"]);
    epic.json(&["label", "add", "--name", "ui"]);
    epic.json(&["label", "add", "--name", "core"]);
    let output = epic.run(&["label", "move", "--name", "docs", "--index", "0"]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "an unknown epic label refuses"
    );
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("\"docs\""),
        "the refusal names the epic label: {message}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "the refusal names the fix: {message}"
    );
}

#[test]
fn the_lane_verb_and_the_lane_flag_answer_as_unknown() {
    let sandbox = Sandbox::new("lane-is-gone");
    sandbox.json(&["label", "add", "--name", "A"]);

    for args in [
        &["lane", "list"][..],
        &["ticket", "add", "--title", "T", "--lane", "A"][..],
    ] {
        let output = sandbox.run(args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{} is a usage refusal",
            args.join(" ")
        );
        let usage = String::from_utf8_lossy(&output.stderr).to_lowercase();
        assert!(usage.contains("usage:"), "the refusal prints the usage");
        assert!(
            !usage.contains("lane"),
            "the usage no longer offers the old name: {usage}"
        );
    }
    let listed = sandbox.json(&["ticket", "list"]);
    assert_eq!(
        listed["tickets"].as_array().map(Vec::len),
        Some(0),
        "the refused add filed nothing"
    );
}
