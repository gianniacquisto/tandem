//! The Settings surface's one read and its one write, over the wire.
//!
//! `docs/port.md` (Remaining surfaces): Settings is the surface the setup
//! wizard and the doctor's window fold into, and the repair action it carries
//! is `setup install` run again, never a writer of the app's own. These tests
//! hold the contract that makes that true:
//!
//! - `setup.status` answers the roots a Settings surface shows beside the agent
//!   face, so the renderer computes no path of its own;
//! - `setup.install` answers the report the CLI prints, so the repair's result
//!   is the files it wrote and not a verdict;
//! - `doctor.report` answers ADR 0014's eight rows by name and state;
//! - a broken host is red, and the repair turns it green in place;
//! - every method the rendered Settings face calls is a method the wire speaks,
//!   so a control cannot reach a writer the CLI does not have.
//!
//! The environment is injected (`handle_in`), so `HOME`, `XDG_CONFIG_HOME` and
//! `TANDEM_AGENT_BUNDLE` name a throwaway tree and no run reads the human's own.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use tandem_core::setup::{self, Environment};
use tandem_core::store::{Actor, Home, Store};
use tandem_core::wire::rpc::METHODS;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway machine: one home, one bundle, one injected environment.
struct Fixture {
    root: PathBuf,
    home: PathBuf,
    tandem_home: PathBuf,
    environment: Environment,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-settings-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("user");
        let tandem_home = root.join("tandem");
        let bundle = root.join("bundle");
        fs::create_dir_all(&home).expect("the user home is creatable");
        write_bundle(&bundle);

        let mut environment = Environment::new();
        environment.insert("HOME".to_string(), setup::text(&home));
        environment.insert(
            "XDG_CONFIG_HOME".to_string(),
            setup::text(&home.join(".config")),
        );
        environment.insert("TANDEM_AGENT_BUNDLE".to_string(), setup::text(&bundle));

        Self {
            root,
            home,
            tandem_home,
            environment,
        }
    }

    fn store(&self) -> Store {
        Store::new(Home::resolve(Some(&self.tandem_home), None, &self.home))
    }

    /// The skill link the install writes: a directory symlink into the face.
    fn opencode_skill(&self) -> PathBuf {
        self.home.join(".config/opencode/skills/tandem-setup")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_bundle(bundle: &Path) {
    let skill = bundle.join("skills/tandem-setup");
    fs::create_dir_all(&skill).expect("the bundle skill");
    fs::write(skill.join("SKILL.md"), "setup\n").expect("the skill");
    fs::create_dir_all(bundle.join("agents")).expect("the bundle agents");
    fs::write(bundle.join("agents/builder.md"), "builder\n").expect("the prompt");
    fs::write(bundle.join("AGENTS.md"), "the agent face\n").expect("AGENTS.md");
    fs::write(bundle.join("VERSION"), "1.0\n").expect("VERSION");
}

fn call(fixture: &Fixture, id: u64, method: &str, params: Value) -> Value {
    let frame = json!({"id": id, "method": method, "params": params}).to_string();
    let answer = tandem_core::rpc::handle_in(
        &frame,
        &fixture.store(),
        &Actor::Human,
        &fixture.environment,
    );
    serde_json::from_str(&answer)
        .unwrap_or_else(|error| panic!("one JSON frame: {error}: {answer}"))
}

/// A method's reply, or the test's failure when the frame was refused.
fn reply(fixture: &Fixture, method: &str, params: Value) -> Value {
    let answer = call(fixture, 1, method, params);
    match answer.get("error") {
        None => answer["result"].clone(),
        Some(error) => panic!("{method} answered an error: {error}"),
    }
}

fn status(fixture: &Fixture) -> Value {
    reply(fixture, "setup.status", json!({}))
}

fn doctor(fixture: &Fixture) -> Value {
    reply(fixture, "doctor.report", json!({}))["report"].clone()
}

fn install(fixture: &Fixture) -> Value {
    reply(
        fixture,
        "setup.install",
        json!({"host": null, "force": false}),
    )
}

fn check<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .unwrap_or_else(|| panic!("the report carries checks: {report}"))
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("the report carries the `{name}` check: {report}"))
}

#[test]
fn setup_status_names_the_roots_a_settings_surface_shows() {
    let fixture = Fixture::new("roots");

    let answer = status(&fixture);

    assert_eq!(answer["home"], setup::text(&fixture.tandem_home));
    assert_eq!(
        answer["bikeshed"],
        setup::text(&fixture.tandem_home.join("bikeshed"))
    );
    let hosts: Vec<(String, String)> = answer["hosts"]
        .as_array()
        .unwrap_or_else(|| panic!("the reply names each host: {answer}"))
        .iter()
        .map(|host| {
            (
                host["host"].as_str().expect("a host name").to_string(),
                host["path"].as_str().expect("a root path").to_string(),
            )
        })
        .collect();
    assert_eq!(
        hosts,
        vec![
            (
                "opencode".to_string(),
                setup::text(&fixture.home.join(".config/opencode"))
            ),
            (
                "claude".to_string(),
                setup::text(&fixture.home.join(".claude"))
            ),
            (
                "agents".to_string(),
                setup::text(&fixture.home.join(".agents"))
            ),
        ],
        "each host root is resolved from the injected environment"
    );
}

#[test]
fn a_fresh_home_reports_no_installed_face_and_the_eleven_checks() {
    let fixture = Fixture::new("fresh");

    let answer = status(&fixture);

    assert_eq!(answer["report"]["verb"], "status");
    assert_eq!(answer["report"]["manifest"], "absent");
    assert_eq!(answer["report"]["state"]["bundledVersion"], "1.0");
    assert!(
        answer["report"]["state"]["installedVersion"].is_null(),
        "nothing is installed yet: {answer}"
    );
    for host in answer["report"]["state"]["hosts"]
        .as_array()
        .expect("every host is reported")
    {
        assert_eq!(host["state"], "absent", "{host}");
        assert_eq!(host["links"], 0, "{host}");
    }

    let report = doctor(&fixture);
    let names: Vec<&str> = report["checks"]
        .as_array()
        .expect("the report carries checks")
        .iter()
        .map(|check| check["name"].as_str().expect("a check name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "home",
            "store",
            "bikeshed",
            "comments",
            "sessions",
            "settings",
            "todos-json",
            "lock",
            "app",
            "skills",
            "agent-face"
        ],
        "ADR 0014's pinned order, with TAN-243's three leftover rows after `comments`"
    );
    assert_eq!(
        check(&report, "agent-face")["ok"],
        true,
        "a host with no face and no warnings has nothing to repair yet"
    );
}

#[test]
fn a_broken_face_is_red_and_the_repair_turns_it_green_in_place() {
    let fixture = Fixture::new("repair");

    let installed = install(&fixture);
    assert_eq!(installed["report"]["verb"], "install");
    assert_eq!(installed["report"]["state"]["installedVersion"], "1.0");
    assert_eq!(check(&doctor(&fixture), "agent-face")["ok"], true);

    // A partial face: the installed copy lost one skill, so the host's links
    // dangle. This is the broken host the Settings row shows with a fix.
    fs::remove_dir_all(fixture.tandem_home.join("agent/skills/tandem-setup"))
        .expect("the installed skill is removable");
    let broken = doctor(&fixture);
    assert_eq!(
        check(&broken, "agent-face")["ok"],
        false,
        "a partial face is trouble: {broken}"
    );
    let state = status(&fixture)["report"]["state"].clone();
    let warnings: Vec<&str> = state["warnings"]
        .as_array()
        .expect("the state names its warnings")
        .iter()
        .map(|warning| warning["code"].as_str().expect("a code"))
        .collect();
    assert!(
        warnings.contains(&"partial-face"),
        "the face is partial: {warnings:?}"
    );

    let repaired = install(&fixture);
    assert_eq!(repaired["report"]["state"]["installedVersion"], "1.0");
    assert!(
        fixture
            .tandem_home
            .join("agent/skills/tandem-setup")
            .exists(),
        "the repair wrote the missing skill back"
    );

    assert_eq!(
        check(&doctor(&fixture), "agent-face")["ok"],
        true,
        "the repair turned the row green in place"
    );
    assert_eq!(
        status(&fixture)["report"]["state"]["warnings"],
        json!([]),
        "nothing stands after the repair"
    );
}

#[test]
fn every_repair_lists_the_files_it_wrote() {
    let fixture = Fixture::new("written");

    let installed = install(&fixture);
    let paths: Vec<&str> = installed["report"]["paths"]
        .as_array()
        .expect("the install lists its paths")
        .iter()
        .map(|action| action["path"].as_str().expect("a path"))
        .collect();
    assert!(
        paths.contains(&setup::text(&fixture.opencode_skill()).as_str()),
        "the skill link is named: {installed}"
    );
    assert!(
        paths.contains(&setup::text(&fixture.home.join(".config/opencode/AGENTS.md")).as_str())
            || paths.iter().any(|path| path.ends_with("opencode.json")),
        "the instruction route is named: {installed}"
    );

    let again = install(&fixture);
    for action in again["report"]["paths"]
        .as_array()
        .expect("the second install lists its paths")
    {
        assert!(
            matches!(action["action"].as_str(), Some("ok") | Some("linked")),
            "a second install writes nothing new: {action}"
        );
    }
}

#[test]
fn an_install_with_no_bundle_is_a_refusal_with_a_code() {
    let mut fixture = Fixture::new("no-bundle");
    fixture.environment.remove("TANDEM_AGENT_BUNDLE");

    let answer = call(&fixture, 1, "setup.install", json!({"force": false}));

    let error = answer
        .get("error")
        .unwrap_or_else(|| panic!("the refusal is an error frame: {answer}"));
    assert_eq!(error["code"], "setup");
    assert!(
        error["message"]
            .as_str()
            .expect("a message")
            .contains("no bundled agent face"),
        "{error}"
    );
    assert!(
        !fixture.tandem_home.join("agent").exists(),
        "a refusal writes nothing"
    );
}

#[test]
fn every_method_the_settings_face_calls_is_the_wires() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/renderer/src");
    let mut called: Vec<String> = Vec::new();
    for name in ["faces/SettingsFace.tsx", "shell/setup.ts"] {
        let source = fs::read_to_string(app.join(name))
            .unwrap_or_else(|error| panic!("the renderer source {name} is readable: {error}"));
        for capture in source.split("method: '").skip(1) {
            let tag = capture
                .split('\'')
                .next()
                .unwrap_or_else(|| panic!("a method literal in {name} is one tag"));
            called.push(tag.to_string());
        }
    }

    assert!(!called.is_empty(), "the Settings face calls the wire");
    for tag in &called {
        assert!(
            METHODS.iter().any(|(known, _)| known == tag),
            "the Settings face calls `{tag}`, which the wire does not speak"
        );
    }
}
