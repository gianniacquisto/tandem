//! The port-only note verbs the app's writes name: `note write` and `note
//! move`. The app's `note.write` and `note.move` methods must be the CLI's own
//! acts (`docs/port.md`, no UI-only writes), and `note --help` stays the Swift
//! capture's text, so the two verbs are documented at their own help instead.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tandem-note-verbs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Home").join("bikeshed")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("Home")
    }

    fn vault(&self) -> PathBuf {
        self.home().join("bikeshed")
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(BINARY)
            .arg("--home")
            .arg(self.home())
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env_remove("TANDEM_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        if let Some(body) = stdin {
            child
                .stdin
                .as_mut()
                .expect("a piped stdin")
                .write_all(body.as_bytes())
                .expect("the body is writable");
        }
        child.wait_with_output().expect("the run finishes")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn printed(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "the verb succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("the verb prints one JSON object")
}

#[test]
fn note_write_takes_its_text_on_the_flag_or_stdin() {
    let sandbox = Sandbox::new("write");

    let flag = sandbox.run(
        &[
            "note",
            "write",
            "--path",
            "alpha.md",
            "--text",
            "hello [[beta]]",
        ],
        None,
    );
    let wrote = printed(&flag);
    assert_eq!(wrote["path"], "alpha.md");
    assert!(wrote["hash"].as_str().is_some_and(|hash| hash.len() == 64));
    assert_eq!(
        fs::read_to_string(sandbox.vault().join("alpha.md")).expect("the note is on disk"),
        "hello [[beta]]"
    );

    let piped = sandbox.run(&["note", "write", "--path", "beta.md"], Some("bee\n"));
    printed(&piped);
    assert_eq!(
        fs::read_to_string(sandbox.vault().join("beta.md")).expect("the note is on disk"),
        "bee\n",
        "an absent flag reads stdin"
    );
}

#[test]
fn note_move_rewrites_the_links_through_the_same_core_call() {
    let sandbox = Sandbox::new("move");
    fs::create_dir_all(sandbox.vault().join("projects")).expect("the folder is creatable");
    sandbox.run(
        &["note", "write", "--path", "beta.md", "--text", "bee"],
        None,
    );
    sandbox.run(
        &[
            "note",
            "write",
            "--path",
            "alpha.md",
            "--text",
            "see [[beta]]",
        ],
        None,
    );

    let moved = printed(&sandbox.run(
        &["note", "move", "--path", "beta.md", "--into", "projects"],
        None,
    ));
    assert_eq!(moved["path"], "projects/beta.md");
    assert_eq!(moved["oldPath"], "beta.md");
    assert_eq!(moved["isFolder"], false);
    assert_eq!(moved["movedNotes"][0]["newPath"], "projects/beta.md");
    assert!(sandbox.vault().join("projects").join("beta.md").is_file());
    assert_eq!(
        fs::read_to_string(sandbox.vault().join("alpha.md")).expect("the note is on disk"),
        "see [[beta]]",
        "the bare target stays the canonical one while it is unique"
    );
}

#[test]
fn the_group_help_stays_the_swift_capture_and_the_new_verbs_name_themselves() {
    let sandbox = Sandbox::new("help");

    let group = sandbox.run(&["note", "--help"], None);
    let text = String::from_utf8_lossy(&group.stdout);
    assert!(text.contains("tandem-cli note links"));
    assert!(
        !text.contains("note write"),
        "the group text is the golden captured from the Swift binary: {text}"
    );

    let write = sandbox.run(&["note", "write", "--help"], None);
    assert_eq!(write.status.code(), Some(0));
    let text = String::from_utf8_lossy(&write.stdout);
    assert!(text.contains("tandem-cli note write"), "{text}");
    assert!(text.contains("--path <path>"), "{text}");

    let moved = sandbox.run(&["note", "move", "--help"], None);
    assert!(String::from_utf8_lossy(&moved.stdout).contains("tandem-cli note move"));
}

#[test]
fn ticket_file_files_the_selection_and_links_the_note() {
    let sandbox = Sandbox::new("ticket-file");
    sandbox.run(
        &[
            "note",
            "write",
            "--path",
            "Index.md",
            "--text",
            "the fixture index",
        ],
        None,
    );

    let filed = printed(&sandbox.run(
        &[
            "ticket",
            "file",
            "--from-note",
            "Index.md",
            "--text",
            "- [ ] File this line",
        ],
        None,
    ));
    assert_eq!(filed["title"], "File this line");
    assert_eq!(filed["link"], "Index.md");
    assert_eq!(filed["description"], "- [ ] File this line");
    assert_eq!(filed["status"], "backlog");

    let empty = sandbox.run(
        &["ticket", "file", "--from-note", "Index.md", "--text", ""],
        None,
    );
    assert_eq!(
        empty.status.code(),
        Some(1),
        "an empty selection is a refusal"
    );
    assert!(
        String::from_utf8_lossy(&empty.stderr).contains("Select text"),
        "{}",
        String::from_utf8_lossy(&empty.stderr)
    );
}
