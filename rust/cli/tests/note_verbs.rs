//! The port-only note verbs the app's writes name: `note write` and `note
//! move`. The app's `note.write` and `note.move` methods must be the CLI's own
//! acts (no UI-only writes), and `note --help` stays the Swift
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
        fs::create_dir_all(root.join("Home").join("notes")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("Home")
    }

    fn vault(&self) -> PathBuf {
        self.home().join("notes")
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

// MARK: - The one-time move of an older `bikeshed/` folder (ADR 0055)

impl Sandbox {
    /// A home that an older release left: `bikeshed/` with notes, no `notes/`.
    fn older(name: &str) -> Self {
        let sandbox = Self::new(name);
        fs::remove_dir_all(sandbox.vault()).unwrap();
        let older = sandbox.home().join("bikeshed");
        fs::create_dir_all(older.join("projects")).unwrap();
        fs::write(older.join("Index.md"), "# Index\n").unwrap();
        fs::write(older.join("projects/p.md"), "# p\n").unwrap();
        sandbox
    }
}

fn names(root: &std::path::Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(root)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    found.sort();
    found
}

#[test]
fn a_note_verb_moves_an_older_bikeshed_folder_to_notes_once() {
    let sandbox = Sandbox::older("migrate");

    let first = sandbox.run(&["note", "stats"], None);
    printed(&first);

    assert_eq!(names(&sandbox.home()), ["notes", "notes.lock"]);
    assert_eq!(
        fs::read_to_string(sandbox.vault().join("Index.md")).unwrap(),
        "# Index\n",
        "every note still reads"
    );
    assert_eq!(
        fs::read_to_string(sandbox.vault().join("projects/p.md")).unwrap(),
        "# p\n"
    );

    let second = sandbox.run(&["note", "stats"], None);
    printed(&second);
    assert_eq!(
        names(&sandbox.home()),
        ["notes", "notes.lock"],
        "a second run changes nothing"
    );
}

#[test]
fn an_empty_notes_folder_beside_bikeshed_still_migrates() {
    let sandbox = Sandbox::older("migrate-empty");
    fs::create_dir_all(sandbox.vault()).unwrap();

    printed(&sandbox.run(&["note", "stats"], None));

    assert!(!sandbox.home().join("bikeshed").exists());
    assert!(sandbox.vault().join("Index.md").is_file());
}

#[test]
fn both_folders_with_notes_refuse_with_the_error_and_leave_both_trees_alone() {
    let sandbox = Sandbox::older("migrate-clash");
    fs::create_dir_all(sandbox.vault()).unwrap();
    fs::write(sandbox.vault().join("Mine.md"), "mine\n").unwrap();

    let output = sandbox.run(&["note", "stats"], None);

    assert_ne!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Merge the notes"), "{stderr}");
    assert!(
        stderr.contains("bikeshed") && stderr.contains("notes"),
        "{stderr}"
    );
    assert_eq!(names(&sandbox.home()), ["bikeshed", "notes"]);
    assert_eq!(names(&sandbox.vault()), ["Mine.md"]);
    assert_eq!(
        names(&sandbox.home().join("bikeshed")),
        ["Index.md", "projects"]
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_bikeshed_is_refused_and_not_followed() {
    let sandbox = Sandbox::new("migrate-link");
    let elsewhere = sandbox.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::remove_dir_all(sandbox.vault()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, sandbox.home().join("bikeshed")).unwrap();

    let output = sandbox.run(&["note", "stats"], None);

    assert_ne!(output.status.code(), Some(0));
    assert!(sandbox.home().join("bikeshed").is_symlink());
    assert!(!sandbox.vault().exists());
    assert!(names(&elsewhere).is_empty());
}

#[test]
fn note_stats_carries_the_deprecated_bikeshed_field_and_warns_on_stderr() {
    let sandbox = Sandbox::new("stats-alias");

    let output = sandbox.run(&["note", "stats"], None);
    let stats = printed(&output);

    assert_eq!(stats["bikeshed"], stats["notes"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("deprecated"), "{stderr}");
}
