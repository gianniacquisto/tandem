//! The workflow verbs end to end: list, show, set from stdin, use, delete, and
//! the board's status read against the active workflow (TAN-328).
//!
//! Every child is a sandbox `HOME` and `TANDEM_HOME`, so no run touches a real
//! home.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-workflow-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(BINARY)
            .args(args)
            .env_clear()
            .env("HOME", self.root.join("user"))
            .env("USERPROFILE", self.root.join("user"))
            .env("TANDEM_HOME", self.home())
            .output()
            .expect("the binary runs")
    }

    fn run_stdin(&self, args: &[&str], body: &str) -> Output {
        let mut child = Command::new(BINARY)
            .args(args)
            .env_clear()
            .env("HOME", self.root.join("user"))
            .env("USERPROFILE", self.root.join("user"))
            .env("TANDEM_HOME", self.home())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        child
            .stdin
            .as_mut()
            .expect("a piped stdin")
            .write_all(body.as_bytes())
            .expect("the body is writable");
        child.wait_with_output().expect("the run finishes")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "the run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON object")
}

fn document(name: &str, with_selected: bool) -> String {
    let mut stages = vec![
        json!({"key": "inbox", "label": "Inbox", "role": "intake", "binding": null}),
        json!({"key": "parked", "label": "Parked", "role": "parked", "binding": null}),
        json!({"key": "queued", "label": "Queued", "role": "ready", "binding": {"role": "builder", "pass": "build"}}),
        json!({"key": "doing", "label": "Doing", "role": "active", "binding": {"role": "builder", "pass": "build"}}),
        json!({"key": "checking", "label": "Checking", "role": "review", "binding": {"role": "reviewer", "pass": "review"}}),
        json!({"key": "shipped", "label": "Shipped", "role": "finished", "binding": null}),
    ];
    if with_selected {
        stages.insert(
            1,
            json!({"key": "selected", "label": "Selected for development", "role": "intake", "binding": null}),
        );
    }
    serde_json::to_string(&json!({"name": name, "scope": "home", "stages": stages}))
        .expect("the document serializes")
}

#[test]
fn a_fresh_home_lists_the_agile_default_as_active() {
    let sandbox = Sandbox::new("fresh");
    let listed = json(&sandbox.run(&["workflow", "list"]));

    let workflows = listed["workflows"].as_array().expect("a workflow list");
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["name"], "agile");
    assert_eq!(listed["active"], workflows[0]["id"]);
    assert_eq!(
        workflows[0]["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .map(|stage| stage["key"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec![
            "backlog",
            "on-hold",
            "selected",
            "ready",
            "in-progress",
            "in-review",
            "done"
        ]
    );
}

#[test]
fn workflow_show_reports_the_seven_stage_default() {
    let sandbox = Sandbox::new("show-default");
    let shown = json(&sandbox.run(&["workflow", "show", "--id", "agile"]));

    let stages = shown["workflow"]["stages"].as_array().expect("stages");
    assert_eq!(
        stages
            .iter()
            .map(|stage| stage["key"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec![
            "backlog",
            "on-hold",
            "selected",
            "ready",
            "in-progress",
            "in-review",
            "done"
        ]
    );
    let selected = stages
        .iter()
        .find(|stage| stage["key"] == "selected")
        .expect("selected is declared");
    assert_eq!(selected["label"], "Selected for development");
    assert_eq!(selected["role"], "intake");
    assert_eq!(selected["binding"], Value::Null, "no binding on selected");
}

/// The help's status line is a static string, so it drifts the moment the
/// default workflow gains a stage. Pin it to the default a fresh home declares:
/// the same keys, in the same order, or the contract lies to the caller.
#[test]
fn the_help_status_line_names_every_default_stage_in_workflow_order() {
    let sandbox = Sandbox::new("help-status");
    let shown = json(&sandbox.run(&["workflow", "show", "--id", "agile"]));
    let keys = shown["workflow"]["stages"]
        .as_array()
        .expect("stages")
        .iter()
        .map(|stage| stage["key"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();

    let help = sandbox.run(&["--help"]);
    assert!(help.status.success(), "`--help` answers");
    let text = String::from_utf8(help.stdout).expect("the help is UTF-8");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("status    "))
        .expect("the help prints a status line");
    let named = line
        .split_once("status    ")
        .expect("the status label")
        .1
        .split(" | ")
        .map(str::trim)
        .collect::<Vec<_>>();

    assert_eq!(
        named, keys,
        "`--help`'s status line names the default workflow's stages, in its order"
    );
}

#[test]
fn workflow_set_round_trips_a_second_intake_stage() {
    let sandbox = Sandbox::new("custom");
    let created = json(&sandbox.run_stdin(&["workflow", "set"], &document("flow", true)));
    assert_eq!(created["workflow"]["name"], "flow");
    let id = created["workflow"]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let used = json(&sandbox.run(&["workflow", "use", "--id", &id]));
    assert_eq!(used["workflow"]["name"], "flow");

    let shown = json(&sandbox.run(&["workflow", "show", "--id", "flow"]));
    assert!(
        shown["workflow"]["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .any(|stage| stage["key"] == "selected")
    );

    let added = json(&sandbox.run(&[
        "ticket",
        "add",
        "--title",
        "a custom one",
        "--status",
        "selected",
    ]));
    assert_eq!(added["status"], "selected");

    let listed = json(&sandbox.run(&["ticket", "list", "--filter", "all", "--status", "selected"]));
    assert_eq!(
        listed["tickets"]
            .as_array()
            .expect("tickets")
            .iter()
            .map(|ticket| ticket["title"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec!["a custom one"]
    );
}

#[test]
fn a_workflow_read_reports_the_stores_revision() {
    let sandbox = Sandbox::new("revision");
    let added = json(&sandbox.run(&["ticket", "add", "--title", "moved"]));
    assert_eq!(added["status"], "backlog");

    let shown = json(&sandbox.run(&["workflow", "show"]));
    let revision = shown["revision"]
        .as_i64()
        .expect("the reply carries a revision");
    assert!(
        revision > 0,
        "the read carries the store's revision, got {revision}"
    );

    let listed = json(&sandbox.run(&["workflow", "list"]));
    assert_eq!(
        listed["revision"].as_i64(),
        Some(revision),
        "list and show report the same snapshot revision"
    );
}

#[test]
fn ticket_set_and_list_round_trip_selected() {
    let sandbox = Sandbox::new("selected-roundtrip");
    let added = json(&sandbox.run(&["ticket", "add", "--title", "promoted"]));
    let id = added["id"].as_str().expect("an id");
    assert_eq!(added["status"], "backlog", "a new ticket lands backlog");

    let moved = json(&sandbox.run(&["ticket", "set", "--id", id, "--status", "selected"]));
    assert_eq!(moved["status"], "selected");

    let listed = json(&sandbox.run(&["ticket", "list", "--filter", "all", "--status", "selected"]));
    assert_eq!(
        listed["tickets"]
            .as_array()
            .expect("tickets")
            .iter()
            .map(|ticket| ticket["title"].as_str().unwrap_or(""))
            .collect::<Vec<_>>(),
        vec!["promoted"]
    );
}

#[test]
fn ticket_add_lands_in_the_first_intake_stage() {
    let sandbox = Sandbox::new("landing");
    let created = json(&sandbox.run_stdin(&["workflow", "set"], &document("flow", true)));
    let id = created["workflow"]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    json(&sandbox.run(&["workflow", "use", "--id", &id]));

    let added = json(&sandbox.run(&["ticket", "add", "--title", "landed"]));
    assert_eq!(added["status"], "inbox", "the first intake stage lands it");
}

#[test]
fn an_unknown_stage_is_refused_with_the_valid_keys() {
    let sandbox = Sandbox::new("unknown-stage");
    let output = sandbox.run(&["ticket", "set", "--id", "x", "--status", "nowhere"]);
    assert_eq!(output.status.code(), Some(1));
    let failure = String::from_utf8_lossy(&output.stderr);
    assert!(failure.contains("nowhere"), "{failure}");
    assert!(
        failure.contains("backlog"),
        "the refusal names the stages: {failure}"
    );
    assert!(
        failure.contains("selected"),
        "the refusal names selected among the stages: {failure}"
    );
}

#[test]
fn deleting_an_occupied_stage_is_refused_with_its_count() {
    let sandbox = Sandbox::new("occupied");
    json(&sandbox.run(&["ticket", "add", "--title", "waiting"]));

    let shown = json(&sandbox.run(&["workflow", "show", "--id", "agile"]));
    let mut stages = shown["workflow"]["stages"]
        .as_array()
        .expect("stages")
        .iter()
        .filter(|stage| stage["key"] != "backlog")
        .cloned()
        .collect::<Vec<_>>();
    stages.insert(
        0,
        json!({"key": "inbox", "label": "Inbox", "role": "intake", "binding": null}),
    );
    let body = json!({"name": "agile", "scope": "home", "stages": stages});
    let output = sandbox.run_stdin(&["workflow", "set", "--id", "agile"], &body.to_string());
    assert_eq!(output.status.code(), Some(1));
    let failure = String::from_utf8_lossy(&output.stderr);
    assert!(failure.contains("backlog"), "{failure}");
    assert!(
        failure.contains("1 ticket"),
        "the refusal names the count: {failure}"
    );
}

#[test]
fn activating_a_workflow_that_lacks_an_occupied_stage_is_refused_with_counts() {
    let sandbox = Sandbox::new("missing");
    json(&sandbox.run(&["ticket", "add", "--title", "waiting"]));
    json(&sandbox.run_stdin(&["workflow", "set"], &document("flow", false)));

    let output = sandbox.run(&["workflow", "use", "--id", "flow"]);
    assert_eq!(output.status.code(), Some(1));
    let failure = String::from_utf8_lossy(&output.stderr);
    assert!(failure.contains("backlog"), "{failure}");
    assert!(
        failure.contains("(1)"),
        "the refusal names the count: {failure}"
    );
}

#[test]
fn the_default_cannot_be_deleted() {
    let sandbox = Sandbox::new("default-delete");
    let output = sandbox.run(&["workflow", "delete", "--id", "agile"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("default"),
        "the refusal says why"
    );
}

#[test]
fn deleting_an_inactive_workflow_answers_the_remaining_list() {
    let sandbox = Sandbox::new("delete");
    json(&sandbox.run_stdin(&["workflow", "set"], &document("flow", false)));
    let deleted = json(&sandbox.run(&["workflow", "delete", "--id", "flow"]));
    let names: Vec<String> = deleted["workflows"]
        .as_array()
        .expect("workflows")
        .iter()
        .map(|workflow| workflow["name"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(names, vec!["agile"]);
}
