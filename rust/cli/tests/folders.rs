//! The local `project add|rename` and `label add|rename` verbs keep the notes
//! folders in step with the board, as the MCP tools do (TAN-921).

use std::fs;
use std::path::PathBuf;
use std::process::Output;
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
            "tandem-folders-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn projects(&self) -> PathBuf {
        self.root.join("Home").join("notes").join("projects")
    }

    fn run(&self, args: &[&str]) -> Output {
        std::process::Command::new(BINARY)
            .arg("--home")
            .arg(self.root.join("Home"))
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env_remove("TANDEM_HOME")
            .env_remove("TANDEM_SERVER")
            .env_remove("OPENCODE_SESSION_ID")
            .output()
            .expect("the binary runs")
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
fn project_and_label_add_create_the_folders_and_links_report_none_missing() {
    let sandbox = Sandbox::new("add");
    let added = sandbox.json(&["project", "add", "--name", "Proj"]);
    assert!(added.get("revision").is_none(), "no revision is printed");
    assert!(sandbox.projects().join("Proj").is_dir());

    let labels = sandbox.json(&["label", "add", "--name", "L", "--project", "Proj"]);
    assert!(labels.get("revision").is_none(), "no revision is printed");
    assert!(sandbox.projects().join("Proj").join("L").is_dir());

    let links = sandbox.json(&["note", "links"]);
    let missing = &links["missingFolders"];
    assert!(
        missing.is_null() || missing.as_array().is_some_and(Vec::is_empty),
        "missingFolders: {missing}"
    );
}

#[test]
fn project_and_label_rename_move_the_folders_and_the_notes_inside() {
    let sandbox = Sandbox::new("rename");
    sandbox.json(&["project", "add", "--name", "Proj"]);
    sandbox.json(&["label", "add", "--name", "ui", "--project", "Proj"]);
    let wrote = sandbox.run(&[
        "note",
        "write",
        "--path",
        "projects/Proj/ui/n.md",
        "--text",
        "hi",
    ]);
    assert_eq!(wrote.status.code(), Some(0));

    sandbox.json(&["label", "rename", "--from", "ui", "--to", "web"]);
    assert!(sandbox.projects().join("Proj/web/n.md").is_file());
    assert!(!sandbox.projects().join("Proj/ui").exists());

    sandbox.json(&["project", "rename", "--from", "Proj", "--to", "Next"]);
    assert!(sandbox.projects().join("Next/web/n.md").is_file());
    assert!(!sandbox.projects().join("Proj").exists());
}

#[test]
fn a_refused_project_add_prints_the_message_and_exits_one() {
    let sandbox = Sandbox::new("refuse");
    sandbox.json(&["project", "add", "--name", "Proj"]);
    let output = sandbox.run(&["project", "add", "--name", "Proj"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
