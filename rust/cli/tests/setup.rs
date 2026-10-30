//! The `setup` group through the built binary, ported from
//! `SetupVerbTests.swift`: one JSON report on stdout, the exit codes the install
//! report earns, and the failure that is a message on stderr with no JSON at all
//! (ADR 0034).

use std::fs;
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
            "tandem-setup-verb-tests-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home")).expect("the sandbox home");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn user(&self) -> PathBuf {
        self.root.join("user")
    }

    fn bundle(&self) -> PathBuf {
        self.root.join("bundle")
    }

    fn tandem_home(&self) -> PathBuf {
        self.home().join(".tandem")
    }

    fn write_bundle(&self, version: &str) {
        let bundle = self.bundle();
        fs::create_dir_all(bundle.join("skills/alpha")).unwrap();
        fs::create_dir_all(bundle.join("agents")).unwrap();
        fs::write(bundle.join("skills/alpha/SKILL.md"), "alpha\n").unwrap();
        fs::write(bundle.join("agents/builder.md"), "builder\n").unwrap();
        fs::write(bundle.join("AGENTS.md"), "the agent face\n").unwrap();
        fs::write(bundle.join("VERSION"), version).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(self.tandem_home(), fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run(sandbox: &Sandbox, args: &[&str], bundle: Option<&Path>) -> Output {
    let mut command = Command::new(BINARY);
    command
        .args(args)
        .current_dir(sandbox.home())
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", sandbox.user())
        .env("TANDEM_HOME", sandbox.tandem_home())
        .stdin(Stdio::null());
    if let Some(bundle) = bundle {
        command.env("TANDEM_AGENT_BUNDLE", bundle);
    }
    command.output().expect("the binary runs")
}

fn object(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("the answer is one JSON object")
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn setup_install_prints_one_json_report_and_exits_zero() {
    let sandbox = Sandbox::new("install");
    sandbox.write_bundle("1.0");

    let output = run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stderr(&output), "");
    let value = object(&output);
    assert_eq!(value["verb"], "install");
    assert_eq!(value["manifest"], "used");
    assert_eq!(value["state"]["installedVersion"], "1.0");
    assert_eq!(value["state"]["bundledVersion"], "1.0");
    let paths = value["paths"].as_array().expect("the report carries paths");
    assert!(paths.iter().any(|action| action["action"] == "linked"));
}

#[test]
fn setup_status_exits_zero_and_names_each_host() {
    let sandbox = Sandbox::new("status");
    sandbox.write_bundle("1.0");

    let output = run(&sandbox, &["setup", "status"], Some(&sandbox.bundle()));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stderr(&output), "");
    let value = object(&output);
    assert_eq!(value["verb"], "status");
    assert!(
        value.get("bundled").is_none(),
        "status carries no bundled path key"
    );
    let hosts = value["state"]["hosts"].as_array().expect("hosts");
    let names: std::collections::HashSet<&str> = hosts
        .iter()
        .filter_map(|host| host["host"].as_str())
        .collect();
    assert_eq!(
        names,
        ["opencode", "claude", "agents"].into_iter().collect()
    );
}

#[test]
fn setup_uninstall_prints_one_json_report_and_exits_zero() {
    let sandbox = Sandbox::new("uninstall");
    sandbox.write_bundle("1.0");
    run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));

    let output = run(&sandbox, &["setup", "uninstall"], Some(&sandbox.bundle()));
    assert_eq!(output.status.code(), Some(0));
    let value = object(&output);
    assert_eq!(value["verb"], "uninstall");
    assert_eq!(value["manifest"], "used");
}

#[test]
fn setup_uninstall_with_nothing_installed_reports_absent() {
    let sandbox = Sandbox::new("uninstall-empty");
    sandbox.write_bundle("1.0");

    let output = run(&sandbox, &["setup", "uninstall"], Some(&sandbox.bundle()));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(object(&output)["manifest"], "absent");
}

#[test]
fn setup_install_with_no_bundle_exits_one_with_no_json() {
    let sandbox = Sandbox::new("no-bundle");
    sandbox.write_bundle("1.0");

    let output = run(&sandbox, &["setup", "install"], None);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output), "", "no JSON on stdout");
    assert!(stderr(&output).contains("no bundled agent face"));
}

#[test]
fn an_unknown_host_is_a_domain_failure() {
    let sandbox = Sandbox::new("unknown-host");
    sandbox.write_bundle("1.0");

    let output = run(
        &sandbox,
        &["setup", "install", "--host", "banana"],
        Some(&sandbox.bundle()),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output), "");
    assert!(stderr(&output).contains("unknown host"));
}

#[test]
fn setup_status_prints_the_warnings_and_exits_zero() {
    let sandbox = Sandbox::new("status-warnings");
    sandbox.write_bundle("1.0");
    run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));
    let path = sandbox.user().join(".config/opencode/skills/alpha");
    fs::remove_file(&path).unwrap();
    link(&path, &sandbox.tandem_home().join("agent/skills/ghost"));

    let output = run(&sandbox, &["setup", "status"], Some(&sandbox.bundle()));
    assert_eq!(
        output.status.code(),
        Some(0),
        "status exits 0 even with warnings"
    );
    let value = object(&output);
    let warnings = value["state"]["warnings"].as_array().expect("warnings");
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"] == "dead-link")
    );
    assert!(warnings.iter().all(|warning| warning["hint"].is_string()));
}

#[test]
fn setup_install_prints_the_same_warnings_as_status() {
    let sandbox = Sandbox::new("same-warnings");
    sandbox.write_bundle("1.0");
    let installed = run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));
    let path = sandbox.user().join(".config/opencode/skills/alpha");
    fs::remove_file(&path).unwrap();
    link(&path, &sandbox.tandem_home().join("agent/skills/ghost"));

    let again = run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));
    let status = run(&sandbox, &["setup", "status"], Some(&sandbox.bundle()));
    assert_eq!(installed.status.code(), Some(0));
    assert_eq!(
        again.status.code(),
        Some(0),
        "a warning is a skip, not a failure"
    );
    let installed_state = &object(&again)["state"];
    let status_state = &object(&status)["state"];
    assert!(installed_state["warnings"].is_array());
    assert_eq!(installed_state["warnings"], status_state["warnings"]);
}

#[test]
fn setup_install_exits_one_and_still_prints_the_report_on_an_error() {
    let sandbox = Sandbox::new("unwritable");
    sandbox.write_bundle("1.0");
    fs::create_dir_all(sandbox.tandem_home()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(sandbox.tandem_home(), fs::Permissions::from_mode(0o555)).unwrap();
    }

    let output = run(&sandbox, &["setup", "install"], Some(&sandbox.bundle()));
    assert_eq!(output.status.code(), Some(1));
    let value = object(&output);
    let warnings = value["state"]["warnings"].as_array().expect("warnings");
    assert!(
        warnings
            .iter()
            .any(|warning| { warning["code"] == "unwritable" && warning["severity"] == "error" })
    );
}

#[test]
fn setup_and_each_verbs_help_print_their_own_grammar() {
    let sandbox = Sandbox::new("help");
    sandbox.write_bundle("1.0");

    let group = run(&sandbox, &["setup", "--help"], Some(&sandbox.bundle()));
    assert_eq!(group.status.code(), Some(0));
    let group_text = text(&group);
    assert!(group_text.contains("tandem-cli setup install"));
    assert!(group_text.contains("tandem-cli setup uninstall"));
    assert!(group_text.contains("tandem-cli setup status"));

    for verb in ["install", "uninstall", "status"] {
        let output = run(
            &sandbox,
            &["setup", verb, "--help"],
            Some(&sandbox.bundle()),
        );
        assert_eq!(output.status.code(), Some(0), "setup {verb} --help");
        assert_eq!(stderr(&output), "", "setup {verb} --help");
        assert!(
            text(&output).contains(&format!("tandem-cli setup {verb}")),
            "setup {verb} --help names itself"
        );
    }

    let doctor = run(&sandbox, &["doctor", "--help"], Some(&sandbox.bundle()));
    assert_eq!(doctor.status.code(), Some(0));
    assert!(text(&doctor).contains("tandem-cli doctor"));
}

fn link(path: &Path, target: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, path).unwrap();
    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, path).unwrap();
        } else {
            std::os::windows::fs::symlink_file(target, path).unwrap();
        }
    }
}
