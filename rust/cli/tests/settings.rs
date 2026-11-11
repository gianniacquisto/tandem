//! The active-project focus, end to end: the CLI sets, shows and clears the
//! one value the faces draw, and an undeclared project is refused with the
//! projects the human may pick.
//!
//! Every test gives the child a sandbox `HOME` and `TANDEM_HOME`, so the runs
//! touch no real home. `lane add` with no `--project` declares the project the
//! lane is named for, so the focus names a project the board carries.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory that is this test's `HOME` and `TANDEM_HOME`.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-settings-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox root is creatable");
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

/// Run the binary with a sandbox `HOME` and `TANDEM_HOME`.
fn tandem(sandbox: &Sandbox, args: &[&str]) -> Output {
    let user_home = sandbox.root.join("user");
    fs::create_dir_all(&user_home).expect("the sandbox user home is creatable");
    Command::new(BINARY)
        .args(args)
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .env("TANDEM_HOME", sandbox.home())
        .output()
        .expect("the binary runs")
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "the run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON object")
}

fn declare(sandbox: &Sandbox, name: &str) {
    let output = tandem(sandbox, &["lane", "add", "--name", name]);
    assert!(
        output.status.success(),
        "the lane is declared: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_fresh_home_shows_no_focus() {
    let sandbox = Sandbox::new("fresh");

    let shown = json(&tandem(&sandbox, &["settings", "get"]));

    assert_eq!(
        shown["activeProject"],
        Value::Null,
        "a home that never stored a focus has none: {shown}"
    );
    assert!(
        shown["roster"].is_array(),
        "the roster answers beside the focus: {shown}"
    );
}

#[test]
fn the_cli_sets_shows_and_clears_the_active_project() {
    let sandbox = Sandbox::new("round-trip");
    declare(&sandbox, "Tandem");

    let set = json(&tandem(
        &sandbox,
        &["settings", "set", "--project", "Tandem"],
    ));
    assert_eq!(set["activeProject"], Value::String("Tandem".to_string()));

    let shown = json(&tandem(&sandbox, &["settings", "get"]));
    assert_eq!(
        shown["activeProject"],
        Value::String("Tandem".to_string()),
        "the read answers the stored focus: {shown}"
    );

    let cleared = json(&tandem(&sandbox, &["settings", "set"]));
    assert_eq!(
        cleared["activeProject"],
        Value::Null,
        "an absent --project clears: {cleared}"
    );

    let shown = json(&tandem(&sandbox, &["settings", "get"]));
    assert_eq!(
        shown["activeProject"],
        Value::Null,
        "clearing restores no focus: {shown}"
    );
}

#[test]
fn setting_an_undeclared_project_is_refused_naming_the_declared_projects() {
    let sandbox = Sandbox::new("unknown");
    declare(&sandbox, "Tandem");
    declare(&sandbox, "Port");

    let output = tandem(&sandbox, &["settings", "set", "--project", "Nope"]);

    assert!(
        !output.status.success(),
        "an undeclared project is refused, not stored"
    );
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("Nope"),
        "the refusal names the project: {message}"
    );
    assert!(
        message.contains("Tandem") && message.contains("Port"),
        "the refusal names the declared projects: {message}"
    );

    let shown = json(&tandem(&sandbox, &["settings", "get"]));
    assert_eq!(
        shown["activeProject"],
        Value::Null,
        "the refused write left the store alone: {shown}"
    );
}

/// The focus meant a lane before UI-33 and means a project now: a lane name
/// is refused like any other name no project carries, and the refusal names
/// the projects, not the lanes.
#[test]
fn a_lane_name_is_refused_as_a_project_focus() {
    let sandbox = Sandbox::new("lane-not-project");
    declare(&sandbox, "Tandem");
    let lane = tandem(
        &sandbox,
        &["lane", "add", "--name", "ui", "--project", "Tandem"],
    );
    assert!(
        lane.status.success(),
        "the lane is declared inside the project: {}",
        String::from_utf8_lossy(&lane.stderr)
    );

    let refused = tandem(&sandbox, &["settings", "set", "--project", "ui"]);
    assert!(
        !refused.status.success(),
        "a lane name is not a project focus, so it is refused"
    );
    let message = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        message.contains("ui") && message.contains("Tandem"),
        "the refusal names the name and the projects: {message}"
    );

    let shown = json(&tandem(&sandbox, &["settings", "get"]));
    assert_eq!(
        shown["activeProject"],
        Value::Null,
        "the refused write left the store alone: {shown}"
    );
}

#[test]
fn ticket_list_defaults_are_untouched_by_a_focus() {
    let sandbox = Sandbox::new("list-defaults");
    declare(&sandbox, "Tandem");
    declare(&sandbox, "Port");
    let added = json(&tandem(
        &sandbox,
        &["ticket", "add", "--title", "Elsewhere", "--lane", "Port"],
    ));
    assert_eq!(added["lane"], Value::String("Port".to_string()));
    json(&tandem(
        &sandbox,
        &["settings", "set", "--project", "Tandem"],
    ));

    // `ticket list` with no flags still lists every lane: agents opt in
    // through the settings read, the list does not narrow for them.
    let listed = json(&tandem(&sandbox, &["ticket", "list"]));
    assert_eq!(
        listed["tickets"]
            .as_array()
            .expect("the list answers tickets")
            .len(),
        1,
        "the focus hides nothing from the default list: {listed}"
    );
}

#[test]
fn a_focus_only_write_leaves_the_roster_alone() {
    let sandbox = Sandbox::new("focus-keeps-roster");
    declare(&sandbox, "Tandem");

    let stored = json(&tandem(
        &sandbox,
        &["settings", "set", "--roster", "human", "--roster", "review"],
    ));
    assert_eq!(stored["roster"], serde_json::json!(["human", "review"]));

    let focused = json(&tandem(
        &sandbox,
        &["settings", "set", "--project", "Tandem"],
    ));
    assert_eq!(
        focused["activeProject"],
        Value::String("Tandem".to_string())
    );
    assert_eq!(
        focused["roster"],
        serde_json::json!(["human", "review"]),
        "focusing the faces must not rewrite the roster: {focused}"
    );
}

#[test]
fn a_bare_roster_flag_stores_the_empty_list_and_leaves_the_focus() {
    let sandbox = Sandbox::new("empty-roster");
    declare(&sandbox, "Tandem");
    json(&tandem(
        &sandbox,
        &[
            "settings",
            "set",
            "--roster",
            "human",
            "--project",
            "Tandem",
        ],
    ));

    let emptied = json(&tandem(&sandbox, &["settings", "set", "--roster"]));
    assert_eq!(
        emptied["roster"],
        serde_json::json!([]),
        "a bare --roster stores the empty list: {emptied}"
    );
    assert_eq!(
        emptied["activeProject"],
        Value::String("Tandem".to_string()),
        "emptying the roster must not clear the focus: {emptied}"
    );
}
