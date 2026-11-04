//! `lane move` reorders the declared lanes, the CLI half of the lane journey.

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
            "tandem-lane-move-{name}-{}-{serial}",
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
fn lane_move_reorders_the_declared_lanes() {
    let sandbox = Sandbox::new("reorder");
    sandbox.json(&["lane", "add", "--name", "A"]);
    sandbox.json(&["lane", "add", "--name", "B"]);
    sandbox.json(&["lane", "add", "--name", "C"]);

    let moved = sandbox.json(&["lane", "move", "--name", "C", "--index", "0"]);
    assert_eq!(
        moved["lanes"],
        Value::Array(vec![
            Value::String("C".to_string()),
            Value::String("A".to_string()),
            Value::String("B".to_string())
        ]),
        "C moves to the front"
    );

    let listed = sandbox.json(&["lane", "list"]);
    assert_eq!(
        listed["lanes"], moved["lanes"],
        "the list reads the new order"
    );
}

#[test]
fn lane_move_names_an_unknown_lane_and_the_fix() {
    let sandbox = Sandbox::new("unknown");
    sandbox.json(&["lane", "add", "--name", "A"]);
    let output = sandbox.run(&["lane", "move", "--name", "Nope", "--index", "0"]);
    assert_ne!(output.status.code(), Some(0), "an unknown lane refuses");
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("\"Nope\""),
        "the refusal names the lane: {message}"
    );
    assert!(
        message.contains("lane add") && message.contains("lane list"),
        "the refusal names the fix: {message}"
    );
}
