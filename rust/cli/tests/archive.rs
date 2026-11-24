//! `tandem export` and `tandem import` through the built binary (ADR 0050).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-archive-cli-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn command(&self, home: &str) -> Command {
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.home(home))
            .env("HOME", self.root.join("user"))
            .env_remove("TANDEM_AGENT")
            .env_remove("TANDEM_SERVER")
            .env_remove("TANDEM_TOKEN")
            .env_remove("TANDEM_HOME")
            .stdin(Stdio::null());
        command
    }

    fn run(&self, home: &str, args: &[&str]) -> Output {
        self.command(home)
            .args(args)
            .output()
            .expect("the CLI runs")
    }

    /// A home with two tickets and a note.
    fn seed(&self, home: &str) {
        let output = self.run(home, &["project", "add", "--name", "Main"]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        for title in ["first", "second"] {
            let output = self.run(
                home,
                &["ticket", "add", "--title", title, "--project", "Main"],
            );
            assert!(output.status.success(), "{}", text(&output.stderr));
        }
        let notes = self.home(home).join("notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("Index.md"), "# Index\n").unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn ticket_count(sandbox: &Sandbox, home: &str) -> usize {
    let output = sandbox.run(home, &["ticket", "list", "--filter", "all"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let value: Value = serde_json::from_slice(&output.stdout).expect("a ticket list");
    value["tickets"].as_array().expect("tickets").len()
}

fn listing(root: &Path) -> Vec<String> {
    fn walk(path: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            out.push(name.clone());
            if entry.path().is_dir() {
                walk(&entry.path(), &name, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

#[test]
fn export_writes_the_archive_and_prints_its_path_and_size() {
    let sandbox = Sandbox::new("writes");
    sandbox.seed("source");
    let out = sandbox.root.join("home.tar.gz");
    let output = sandbox.run("source", &["export", "--out", out.to_str().unwrap()]);
    assert!(output.status.success(), "{}", text(&output.stderr));

    let bytes = fs::read(&out).expect("the archive exists");
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "a gzip stream");
    let stdout = text(&output.stdout);
    assert!(stdout.contains(out.to_str().unwrap()), "{stdout}");
    assert!(
        stdout.contains(&format!("{} bytes", bytes.len())),
        "{stdout}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // Stock tools list it: the store, the notes, the manifest last.
        if let Ok(listing) = Command::new("tar").arg("-tzf").arg(&out).output()
            && listing.status.success()
        {
            let names: Vec<String> = text(&listing.stdout)
                .lines()
                .map(|line| line.trim_end_matches('/').to_string())
                .collect();
            assert_eq!(names, ["tandem.db", "notes/Index.md", "manifest.json"]);
        }
    }
}

#[test]
fn export_refuses_an_existing_file() {
    let sandbox = Sandbox::new("exists");
    sandbox.seed("source");
    let out = sandbox.root.join("taken.tar.gz");
    fs::write(&out, "precious").unwrap();
    let output = sandbox.run("source", &["export", "--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains(out.to_str().unwrap()), "{stderr}");
    assert_eq!(fs::read_to_string(&out).unwrap(), "precious");
}

#[test]
fn export_to_stdout_pipes_into_import_from_stdin() {
    let sandbox = Sandbox::new("pipe");
    sandbox.seed("source");
    let exported = sandbox.run("source", &["export", "--out", "-"]);
    assert!(exported.status.success(), "{}", text(&exported.stderr));
    assert_eq!(&exported.stdout[..2], &[0x1f, 0x8b]);

    let mut child = sandbox
        .command("target")
        .args(["import", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("import starts");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&exported.stdout)
        .expect("the archive pipes in");
    let output = child.wait_with_output().expect("import ends");
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(ticket_count(&sandbox, "target"), 2);
}

#[test]
fn export_runs_with_no_serve_and_writes_one_record_line_to_stderr() {
    let sandbox = Sandbox::new("record");
    sandbox.seed("source");
    let out = sandbox.root.join("a.tar.gz");
    let output = sandbox
        .command("source")
        .env("TANDEM_AGENT", "builder")
        .args(["export", "--out", out.to_str().unwrap()])
        .output()
        .expect("the CLI runs");
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stderr = text(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(
        lines[0].starts_with("export actor=builder door=cli at="),
        "{stderr}"
    );
    assert!(
        !stderr.contains(out.to_str().unwrap()),
        "the record names no path"
    );
}

#[test]
fn import_restores_a_home_and_names_the_next_steps() {
    let sandbox = Sandbox::new("restores");
    sandbox.seed("source");
    let out = sandbox.root.join("a.tar.gz");
    assert!(
        sandbox
            .run("source", &["export", "--out", out.to_str().unwrap()])
            .status
            .success()
    );
    let output = sandbox.run("target", &["import", out.to_str().unwrap()]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert!(stdout.contains("2 tickets"), "{stdout}");
    assert!(stdout.contains("1 note"), "{stdout}");
    assert!(stdout.contains("tandem serve"), "{stdout}");
    assert!(stdout.contains("sign in again"), "{stdout}");
    assert!(stdout.contains("new tokens"), "{stdout}");
    assert_eq!(ticket_count(&sandbox, "target"), 2);
    assert_eq!(
        fs::read_to_string(sandbox.home("target").join("notes/Index.md")).unwrap(),
        "# Index\n"
    );
}

#[test]
fn each_import_refusal_exits_non_zero_names_the_next_act_and_writes_nothing() {
    let sandbox = Sandbox::new("refusals");
    sandbox.seed("source");
    let good = sandbox.root.join("good.tar.gz");
    assert!(
        sandbox
            .run("source", &["export", "--out", good.to_str().unwrap()])
            .status
            .success()
    );
    let bytes = fs::read(&good).unwrap();
    let cut = sandbox.root.join("cut.tar.gz");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let garbage = sandbox.root.join("garbage.tar.gz");
    fs::write(&garbage, "not an archive").unwrap();

    let missing = sandbox.root.join("nope");
    let cases: [(&str, &str, &[&str], &str); 5] = [
        (
            "a non-empty home",
            "source",
            &[good.to_str().unwrap()],
            "new, empty directory",
        ),
        (
            "a cut archive",
            "t1",
            &[cut.to_str().unwrap()],
            "Export it again",
        ),
        (
            "not an archive",
            "t2",
            &[garbage.to_str().unwrap()],
            "Export it again",
        ),
        (
            "a missing file",
            "t3",
            &[missing.to_str().unwrap()],
            "Check the path",
        ),
        (
            "over the ceiling",
            "t4",
            &[good.to_str().unwrap(), "--max-bytes", "10"],
            "--max-bytes",
        ),
    ];
    for (name, home, args, next_act) in cases {
        let before = listing(&sandbox.home(home));
        let mut full = vec!["import"];
        full.extend_from_slice(args);
        let output = sandbox.run(home, &full);
        assert_eq!(output.status.code(), Some(1), "{name}");
        let stderr = text(&output.stderr);
        assert!(stderr.contains(next_act), "{name}: {stderr}");
        assert_eq!(
            listing(&sandbox.home(home)),
            before,
            "{name} wrote something"
        );
    }
}
