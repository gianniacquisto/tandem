//! The home plumbing, end to end: the binary resolves `--home`, then
//! `TANDEM_HOME`, then the user's `~/.tandem`, and a run against the fixture
//! home never creates or touches the user's own `~/.tandem`.
//!
//! Every test gives the child a sandbox `HOME`, so "the user's `~/.tandem`"
//! here is that sandbox's `.tandem` and the assertion is exact: the directory
//! does not exist after a run that was pointed somewhere else. `doctor` is
//! read-only (ADR 0014), so no run creates the home it reports.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

/// Sandbox names are readable but unique: a counter keeps two tests that share
/// a name (a copy-paste) from silently sharing a directory.
static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

/// The committed fixture home (`Fixtures/port/Home`), built lexically from the
/// manifest directory so the comparison is against the same spelling the child
/// reports: no `..` and no symlink resolution, both of which differ by platform.
fn fixture_home() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // <repo>/rust/cli
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("{} sits two levels below the repo root", manifest.display()));
    let path = root.join("Fixtures").join("port").join("Home");
    assert!(
        path.join("bikeshed").is_dir(),
        "the fixture home is committed at Fixtures/port/Home ({})",
        path.display()
    );
    path
}

/// A throwaway directory that is this test's `HOME` and nothing else.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-home-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox root is creatable");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    /// The user home the child sees: the sandbox, so `~/.tandem` is a path no
    /// run may create unless it resolved the default.
    fn user_home(&self) -> PathBuf {
        self.root.join("user")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Run the binary with a sandbox `HOME` and, when given, a `TANDEM_HOME`.
fn tandem(sandbox: &Sandbox, environment: Option<&Path>, args: &[&str]) -> Output {
    let user_home = sandbox.user_home();
    fs::create_dir_all(&user_home).expect("the sandbox user home is creatable");
    let mut command = Command::new(BINARY);
    command
        .args(args)
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .env_remove("TANDEM_HOME");
    if let Some(home) = environment {
        command.env("TANDEM_HOME", home);
    }
    command.output().expect("the binary runs")
}

/// The report is ADR 0014's shape: exactly `ok` and `checks`.
fn report(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "the run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is one JSON object");
    assert_eq!(
        value.as_object().map(|object| object.len()),
        Some(2),
        "the report has exactly `ok` and `checks`: {value}"
    );
    assert_eq!(
        value["ok"],
        Value::Bool(true),
        "a fresh home passes every row that can fail, so the report is ok: {value}"
    );
    value
}

/// The `home` row, shaped as ADR 0014 pins it, inside the eight-row report.
fn home_row(value: &Value) -> &Value {
    let rows = value["checks"]
        .as_array()
        .expect("`checks` is an array of rows");
    assert_eq!(
        rows.len(),
        8,
        "the report carries the eight checks: {value}"
    );
    let row = rows
        .iter()
        .find(|row| row["name"] == "home")
        .expect("the report carries the `home` row");
    assert_eq!(
        row.as_object().map(|object| object.len()),
        Some(3),
        "a check is exactly `name`, `ok` and `detail`: {row}"
    );
    assert_eq!(row["name"], "home");
    assert_eq!(
        row["ok"],
        Value::Bool(true),
        "`home` is informational: {row}"
    );
    row
}

/// The `home` row's detail is `<directory> (<why>)`, the directory ADR 0014's
/// row reports and the branch of the rule that resolved it.
fn home_detail(value: &Value) -> &str {
    home_row(value)["detail"]
        .as_str()
        .expect("the row names the home")
}

fn reported_home(value: &Value) -> PathBuf {
    let detail = home_detail(value);
    let (home, _) = detail
        .rsplit_once(" (")
        .unwrap_or_else(|| panic!("the detail names the home and its source: {detail}"));
    PathBuf::from(home)
}

fn reported_source(value: &Value) -> &str {
    let detail = home_detail(value);
    let (_, source) = detail
        .rsplit_once(" (")
        .unwrap_or_else(|| panic!("the detail names the home and its source: {detail}"));
    source
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("the detail closes its source: {detail}"))
}

#[test]
fn the_flag_wins_and_the_run_writes_nothing_in_the_users_home() {
    let sandbox = Sandbox::new("flag");
    let environment_home = sandbox.path().join("environment-home");
    let fixture = fixture_home();

    let output = tandem(
        &sandbox,
        Some(&environment_home),
        &["--home", fixture.to_str().unwrap(), "doctor"],
    );
    let value = report(&output);

    assert_eq!(reported_source(&value), "--home");
    assert_eq!(reported_home(&value), fixture);
    // The fixture already exists: the run must leave it exactly as committed,
    // and it must not have fallen through to either override.
    assert!(!sandbox.user_home().join(".tandem").exists());
    assert!(!environment_home.exists());
}

#[test]
fn the_environment_is_second_and_the_run_creates_nothing() {
    let sandbox = Sandbox::new("environment");
    let environment_home = sandbox.path().join("tandem home");

    let output = tandem(&sandbox, Some(&environment_home), &["doctor"]);
    let value = report(&output);

    assert_eq!(reported_source(&value), "TANDEM_HOME override");
    assert_eq!(reported_home(&value), environment_home);
    assert!(
        !environment_home.exists(),
        "doctor writes nothing, not even the home it reports"
    );
    assert!(!sandbox.user_home().join(".tandem").exists());
}

#[test]
fn the_default_is_dot_tandem_under_the_users_home_and_is_not_created() {
    let sandbox = Sandbox::new("default");

    let output = tandem(&sandbox, None, &["doctor"]);
    let value = report(&output);

    assert_eq!(reported_source(&value), "default");
    assert_eq!(reported_home(&value), sandbox.user_home().join(".tandem"));
    assert!(
        !sandbox.user_home().join(".tandem").exists(),
        "doctor reports the default home without creating it"
    );
}

#[test]
fn a_relative_flag_is_taken_from_the_working_directory() {
    let sandbox = Sandbox::new("relative");

    let output = tandem(&sandbox, None, &["--home", "Fixtures/port/Home", "doctor"]);
    let value = report(&output);

    let expected = std::env::current_dir()
        .unwrap()
        .join("Fixtures")
        .join("port")
        .join("Home");
    assert_eq!(reported_home(&value), expected);
    assert!(!sandbox.user_home().join(".tandem").exists());
}

#[test]
fn a_run_against_the_fixture_home_writes_nothing_there() {
    let sandbox = Sandbox::new("fixture");
    let fixture = fixture_home();
    let before = fs::read_dir(&fixture).unwrap().count();

    let output = tandem(
        &sandbox,
        None,
        &["--home", fixture.to_str().unwrap(), "doctor"],
    );
    let value = report(&output);

    assert_eq!(reported_home(&value), fixture);
    assert_eq!(fs::read_dir(&fixture).unwrap().count(), before);
    assert_eq!(
        fs::read_dir(fixture.join("bikeshed")).unwrap().count(),
        3,
        "the fixture bikeshed is untouched"
    );
    assert!(!sandbox.user_home().join(".tandem").exists());
}

/// A write to a home that cannot be created says which path it tried, once.
/// The path is the first argument's, and the io error is the cause, not a
/// second copy of the path (the gate found the two swapped).
#[test]
fn a_home_that_cannot_be_created_names_its_path_and_the_cause() {
    let sandbox = Sandbox::new("uncreatable");
    let occupied = sandbox.path().join("not-a-directory");
    fs::write(&occupied, b"occupied").expect("the file is writable");

    let output = tandem(
        &sandbox,
        Some(&occupied),
        &["ticket", "add", "--title", "A ticket"],
    );

    assert_eq!(output.status.code(), Some(1), "a write failure is exit 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let path = occupied.to_string_lossy();
    assert!(
        stderr.starts_with(&format!("Cannot create the Tandem home at {path}: ")),
        "the message names the path and then the cause: {stderr}"
    );
    assert_eq!(
        stderr.matches(path.as_ref()).count(),
        1,
        "the path is printed once: {stderr}"
    );
}
