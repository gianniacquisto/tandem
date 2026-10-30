//! The home plumbing, end to end: the binary resolves `--home`, then
//! `TANDEM_HOME`, then the user's `~/.tandem`, and a run against the fixture
//! home never creates or touches the user's own `~/.tandem`.
//!
//! Every test gives the child a sandbox `HOME`, so "the user's `~/.tandem`"
//! here is that sandbox's `.tandem` and the assertion is exact: the directory
//! does not exist after a run that was pointed somewhere else.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

/// The committed fixture home (`Fixtures/port/Home`), canonical so the
/// comparison is against the path the child reports, not against `..`.
fn fixture_home() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Fixtures")
        .join("port")
        .join("Home");
    fs::canonicalize(&path).unwrap_or_else(|error| {
        panic!(
            "the fixture home is committed at Fixtures/port/Home ({}): {error}",
            path.display()
        )
    })
}

/// A throwaway directory that is this test's `HOME` and nothing else.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tandem-home-{name}-{}", std::process::id()));
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
        "the report has exactly `home` and `source`: {value}"
    );
    value
}

fn reported_home(value: &Value) -> PathBuf {
    let home = value["home"].as_str().expect("the report names a home");
    PathBuf::from(home)
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

    assert_eq!(value["source"], "flag");
    assert_eq!(reported_home(&value), fixture);
    // The fixture already exists: the run must leave it exactly as committed,
    // and it must not have fallen through to either override.
    assert!(!sandbox.user_home().join(".tandem").exists());
    assert!(!environment_home.exists());
}

#[test]
fn the_environment_is_second_and_the_run_creates_it() {
    let sandbox = Sandbox::new("environment");
    let environment_home = sandbox.path().join("tandem home");

    let output = tandem(&sandbox, Some(&environment_home), &["doctor"]);
    let value = report(&output);

    assert_eq!(value["source"], "environment");
    assert_eq!(reported_home(&value), environment_home);
    assert!(
        environment_home.is_dir(),
        "the run creates the home it uses"
    );
    assert!(!sandbox.user_home().join(".tandem").exists());
}

#[test]
fn the_default_is_dot_tandem_under_the_users_home() {
    let sandbox = Sandbox::new("default");

    let output = tandem(&sandbox, None, &["doctor"]);
    let value = report(&output);

    assert_eq!(value["source"], "default");
    assert_eq!(reported_home(&value), sandbox.user_home().join(".tandem"));
    assert!(sandbox.user_home().join(".tandem").is_dir());
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
fn a_run_against_the_fixture_home_is_the_only_write_there() {
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
