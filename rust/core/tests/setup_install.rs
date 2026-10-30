//! The agent face's install, uninstall and state (ADR 0034), ported from
//! `SetupInstallTests.swift`.
//!
//! One bundled copy in the Tandem home, host links into it, a manifest that
//! records every path, and the warning model that `setup status` and `doctor`
//! both read.
//!
//! Every test drives a fixture bundle directory instead of a real `.app`, so no
//! build and no `/Applications` is involved: `TANDEM_AGENT_BUNDLE` is the seam
//! the product already has (`TANDEM_HOME`'s shape), and the environment is
//! injected so `~/.claude` and `~/.agents` are throwaway.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tandem_core::setup::{self, Code, Environment, Host, PathAction, Report, Severity, State};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    tandem_home: PathBuf,
    bundle: PathBuf,
    skills: Vec<String>,
    prompts: Vec<String>,
    environment: Environment,
}

impl Fixture {
    fn face(&self) -> PathBuf {
        self.tandem_home.join("agent")
    }

    fn manifest(&self) -> PathBuf {
        self.tandem_home.join("agent-install.json")
    }

    fn opencode_config(&self) -> PathBuf {
        self.home.join(".config/opencode/opencode.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        make_writable(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn make_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.is_dir() {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        make_writable(&entry.path());
                    }
                }
            } else if !metadata.file_type().is_symlink() {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o644));
            }
        }
    }
}

fn fixture(skills: &[&str], prompts: &[&str], version: &str) -> Fixture {
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "tandem-setup-tests-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    let tandem_home = home.join(".tandem");
    let bundle = root.join("bundle");
    fs::create_dir_all(&home).expect("the sandbox home is creatable");
    write_bundle(&bundle, skills, prompts, version);
    let mut environment = HashMap::new();
    environment.insert("HOME".into(), setup::text(&home));
    environment.insert("XDG_CONFIG_HOME".into(), setup::text(&home.join(".config")));
    environment.insert("TANDEM_HOME".into(), setup::text(&tandem_home));
    Fixture {
        root,
        home,
        tandem_home,
        bundle,
        skills: skills.iter().map(|skill| skill.to_string()).collect(),
        prompts: prompts.iter().map(|prompt| prompt.to_string()).collect(),
        environment,
    }
}

fn write_bundle(bundle: &Path, skills: &[&str], prompts: &[&str], version: &str) {
    fs::create_dir_all(bundle.join("skills")).expect("the bundle skills");
    fs::create_dir_all(bundle.join("agents")).expect("the bundle agents");
    for skill in skills {
        let directory = bundle.join("skills").join(skill);
        fs::create_dir_all(&directory).expect("the skill directory");
        fs::write(directory.join("SKILL.md"), format!("{skill}\n")).expect("the skill");
    }
    for prompt in prompts {
        fs::write(
            bundle.join("agents").join(format!("{prompt}.md")),
            format!("{prompt}\n"),
        )
        .expect("the prompt");
    }
    fs::write(bundle.join("AGENTS.md"), "the agent face\n").expect("AGENTS.md");
    fs::write(bundle.join("VERSION"), version).expect("VERSION");
}

fn install(fixture: &Fixture, hosts: &[Host], force: bool) -> setup::Result<Report> {
    setup::install(
        &fixture.tandem_home,
        &fixture.environment,
        Some(&fixture.bundle),
        force,
        hosts,
    )
}

fn state(fixture: &Fixture, bundle: Option<&Path>) -> State {
    setup::state(
        &fixture.tandem_home,
        &fixture.environment,
        bundle.or(Some(&fixture.bundle)),
        Path::new("/tmp/tandem-cli"),
        true,
    )
}

fn action<'a>(report: &'a Report, path: &str) -> Option<&'a PathAction> {
    report.paths.iter().find(|action| action.path == path)
}

fn link(path: &Path, target: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the link's directory");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, path).expect("the link");
    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, path).expect("the directory link");
        } else {
            std::os::windows::fs::symlink_file(target, path).expect("the file link");
        }
    }
}

fn target(path: &Path) -> Option<PathBuf> {
    fs::read_link(path).ok()
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

/// Every non-directory entry under `root`, as a sorted map of
/// `path -> contents-or-target`. Empty directories are not state.
fn tree(root: &Path) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    walk(root, root, &mut entries);
    entries
}

fn walk(root: &Path, directory: &Path, entries: &mut BTreeMap<String, String>) {
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if relative
            .split('/')
            .any(|component| component.starts_with("agent.new-"))
        {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            walk(root, &path, entries);
        } else if metadata.file_type().is_symlink() {
            entries.insert(
                relative,
                format!("-> {}", setup::text(&target(&path).unwrap_or_default())),
            );
        } else {
            entries.insert(
                relative,
                fs::read_to_string(&path).unwrap_or_else(|_| "<data>".to_string()),
            );
        }
    }
}

fn warnings(fixture: &Fixture, bundle: Option<&Path>) -> Vec<setup::Warning> {
    state(fixture, bundle).warnings
}

fn has(warnings: &[setup::Warning], code: Code) -> bool {
    warnings.iter().any(|warning| warning.code == code)
}

// MARK: - Install

#[test]
fn install_writes_the_bundled_face_and_links_every_skill_and_prompt() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let report = install(&f, &[Host::Opencode], false).expect("the install runs");

    assert!(f.face().join("AGENTS.md").exists());
    assert_eq!(fs::read_to_string(f.face().join("VERSION")).unwrap(), "1.0");
    for skill in &f.skills {
        let path = f.home.join(".config/opencode/skills").join(skill);
        assert!(is_symlink(&path), "{skill} is linked");
        assert_eq!(
            target(&path),
            Some(f.face().join("skills").join(skill)),
            "{skill} points into the face"
        );
    }
    for prompt in &f.prompts {
        let path = f
            .home
            .join(".config/opencode/agents")
            .join(format!("{prompt}.md"));
        assert!(is_symlink(&path), "{prompt} is linked");
        assert_eq!(
            target(&path),
            Some(f.face().join("agents").join(format!("{prompt}.md")))
        );
    }
    let config = fs::read_to_string(f.opencode_config()).unwrap();
    assert!(config.contains(&setup::text(&f.face().join("AGENTS.md"))));
    assert_eq!(report.verb, "install");
    assert_eq!(report.manifest, "used");
    assert!(
        !f.home.join(".config/opencode/AGENTS.md").exists(),
        "install never writes the global AGENTS.md"
    );
    assert!(
        !f.home.join(".claude/CLAUDE.md").exists(),
        "opencode is the default and claude is not touched"
    );
}

#[test]
fn install_is_idempotent() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let first = install(&f, &[Host::Opencode], false).expect("the first install");
    let before = tree(&f.root);
    let manifest_before = fs::read(f.manifest()).unwrap();

    let second = install(&f, &[Host::Opencode], false).expect("the second install");
    assert_eq!(tree(&f.root), before);
    assert_eq!(fs::read(f.manifest()).unwrap(), manifest_before);
    assert_eq!(second.state, first.state);
    assert!(
        !second
            .paths
            .iter()
            .any(|action| action.action == "linked" || action.action == "replaced"),
        "a second install writes nothing"
    );
}

#[test]
fn install_never_overwrites_a_real_file() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let path = f.home.join(".config/opencode/skills/alpha");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "the human's own\n").unwrap();

    let report = install(&f, &[Host::Opencode], true).expect("the install runs");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "the human's own\n",
        "even --force leaves a real file alone"
    );
    assert_eq!(action(&report, &setup::text(&path)).unwrap().action, "skip");
    assert!(
        action(&report, &setup::text(&path))
            .unwrap()
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("a real file is there"))
    );
}

#[test]
fn install_leaves_a_checkout_link_alone_and_force_replaces_it() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let checkout = f.root.join("checkout/skills/alpha");
    fs::create_dir_all(&checkout).unwrap();
    let path = f.home.join(".config/opencode/skills/alpha");
    link(&path, &checkout);

    let report = install(&f, &[Host::Opencode], false).expect("the install runs");
    assert_eq!(
        target(&path),
        Some(checkout.clone()),
        "the checkout route survives"
    );
    assert_eq!(action(&report, &setup::text(&path)).unwrap().action, "skip");
    assert!(
        action(&report, &setup::text(&path))
            .unwrap()
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("points elsewhere"))
    );

    let forced = install(&f, &[Host::Opencode], true).expect("the forced install");
    assert_eq!(target(&path), Some(f.face().join("skills/alpha")));
    assert_eq!(
        action(&forced, &setup::text(&path)).unwrap().action,
        "replaced"
    );
}

#[test]
fn install_replaces_a_dead_link() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let path = f.home.join(".config/opencode/skills/alpha");
    link(&path, &f.root.join("gone/skills/alpha"));

    let report = install(&f, &[Host::Opencode], false).expect("the install runs");
    assert_eq!(target(&path), Some(f.face().join("skills/alpha")));
    assert_eq!(
        action(&report, &setup::text(&path)).unwrap().action,
        "replaced"
    );
}

#[test]
fn install_honours_the_environment() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let elsewhere = f.root.join("xdg");
    let mut environment = f.environment.clone();
    environment.insert("XDG_CONFIG_HOME".into(), setup::text(&elsewhere));

    setup::install(
        &f.tandem_home,
        &environment,
        Some(&f.bundle),
        false,
        &[Host::Opencode],
    )
    .expect("the install runs");

    assert!(is_symlink(&elsewhere.join("opencode/skills/alpha")));
    assert!(!f.home.join(".config/opencode/skills/alpha").exists());
}

#[test]
fn a_new_bundled_version_replaces_the_installed_copy() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the first install");

    write_bundle(&f.bundle, &["alpha", "beta"], &["builder"], "2.0");
    let report = install(&f, &[Host::Opencode], false).expect("the second install");

    assert_eq!(fs::read_to_string(f.face().join("VERSION")).unwrap(), "2.0");
    assert_eq!(report.state.installed_version.as_deref(), Some("2.0"));
    for skill in &f.skills {
        let path = f.home.join(".config/opencode/skills").join(skill);
        assert!(path.exists(), "{skill} still resolves");
    }
    assert_eq!(setup::manifest(&f.tandem_home).unwrap().version, "2.0");
}

#[test]
fn a_failed_replace_leaves_the_previous_copy_and_every_link_intact() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the first install");
    let before = tree(&f.tandem_home);

    write_bundle(&f.bundle, &["alpha", "beta"], &["builder"], "2.0");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.bundle.join("skills/alpha/SKILL.md"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
    }

    assert!(
        install(&f, &[Host::Opencode], false).is_err(),
        "the unreadable bundle fails the copy"
    );
    assert_eq!(
        tree(&f.tandem_home),
        before,
        "nothing half-written, every link intact"
    );
    let names: Vec<String> = fs::read_dir(&f.tandem_home)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|name| name.starts_with("agent.new-")),
        "the temporary tree is removed"
    );
}

#[test]
fn install_refuses_an_unwritable_home_and_writes_nothing() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(&f.tandem_home).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&f.tandem_home, fs::Permissions::from_mode(0o555)).unwrap();
    }

    let report = install(&f, &[Host::Opencode], false).expect("the install reports");
    assert!(!f.face().exists());
    assert!(!f.manifest().exists());
    assert!(
        report
            .state
            .warnings
            .iter()
            .any(|warning| warning.code == Code::Unwritable && warning.severity == Severity::Error)
    );
}

#[test]
fn claude_and_agents_are_written_only_when_named() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the default install");
    assert!(!f.home.join(".claude").exists());
    assert!(!f.home.join(".agents").exists());

    let all = install(&f, &[Host::Opencode, Host::Claude, Host::Agents], false)
        .expect("the all-host install");
    assert!(is_symlink(&f.home.join(".claude/skills/alpha")));
    assert!(is_symlink(&f.home.join(".agents/skills/alpha")));
    assert!(f.home.join(".claude/CLAUDE.md").exists());
    assert_eq!(all.manifest, "used");
    let manifest = setup::manifest(&f.tandem_home).unwrap();
    let hosts: std::collections::HashSet<&str> =
        manifest.hosts.iter().map(String::as_str).collect();
    assert_eq!(
        hosts,
        ["opencode", "claude", "agents"].into_iter().collect()
    );
}

/// The file list ADR 0034 names, exactly: the face, the manifest, the host
/// links and the two instruction edits. Nothing else, and nothing outside the
/// two roots.
#[test]
fn install_writes_exactly_the_adr_file_list() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode, Host::Claude, Host::Agents], false).expect("the install runs");

    let found: Vec<String> = tree(&f.home).into_keys().collect();
    let face = setup::text(&f.face());
    let expected = vec![
        ".agents/skills/alpha".to_string(),
        ".agents/skills/beta".to_string(),
        ".claude/CLAUDE.md".to_string(),
        ".claude/skills/alpha".to_string(),
        ".claude/skills/beta".to_string(),
        ".config/opencode/agents/builder.md".to_string(),
        ".config/opencode/opencode.json".to_string(),
        ".config/opencode/skills/alpha".to_string(),
        ".config/opencode/skills/beta".to_string(),
        ".tandem/agent-install.json".to_string(),
        ".tandem/agent/AGENTS.md".to_string(),
        ".tandem/agent/VERSION".to_string(),
        ".tandem/agent/agents/builder.md".to_string(),
        ".tandem/agent/skills/alpha/SKILL.md".to_string(),
        ".tandem/agent/skills/beta/SKILL.md".to_string(),
    ];
    assert_eq!(found, expected);

    let entries = tree(&f.home);
    for (path, value) in &entries {
        if value.starts_with("-> ") {
            assert!(
                value.contains(&face),
                "{path} points into the installed face: {value}"
            );
        }
    }
}

// MARK: - The resolver

#[test]
fn the_bundle_resolves_from_the_environment_from_an_app_and_to_none_for_a_dev_build() {
    let f = fixture(&["alpha"], &["builder"], "1.0");
    let mut environment = HashMap::new();
    environment.insert("TANDEM_AGENT_BUNDLE".into(), setup::text(&f.bundle));
    assert_eq!(
        setup::bundle(&environment, Path::new("/tmp/tandem-cli"), true)
            .map(|path| setup::resolved(&path)),
        Some(setup::resolved(&f.bundle))
    );

    let app = f.root.join("Tandem.app");
    let face = app.join("Contents/Resources/agent");
    fs::create_dir_all(&face).unwrap();
    let executable = app.join("Contents/MacOS/tandem-cli");
    assert_eq!(
        setup::bundle(&HashMap::new(), &executable, false).map(|path| setup::resolved(&path)),
        Some(setup::resolved(&face))
    );
    assert_eq!(
        setup::bundle(&HashMap::new(), &executable, true),
        None,
        "a dev build cannot invent a bundle"
    );
    assert_eq!(
        setup::bundle(&HashMap::new(), Path::new("/tmp/build/tandem-cli"), false),
        None
    );
}

// MARK: - Status and its states

#[test]
fn status_reports_each_state() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");

    let empty = state(&f, None);
    assert!(empty.hosts.iter().all(|host| host.state == "absent"));

    install(&f, &[Host::Opencode, Host::Claude, Host::Agents], false).expect("the install runs");
    let checkout = f.root.join("checkout/skills/alpha");
    fs::create_dir_all(&checkout).unwrap();
    let claude = f.home.join(".claude/skills/alpha");
    fs::remove_file(&claude).unwrap();
    link(&claude, &checkout);
    let agents = f.home.join(".agents/skills/beta");
    fs::remove_file(&agents).unwrap();
    fs::write(&agents, "the human's own\n").unwrap();

    let wired = state(&f, None);
    let by_host = |host: &str| {
        wired
            .hosts
            .iter()
            .find(|state| state.host == host)
            .unwrap()
            .state
            .clone()
    };
    assert_eq!(by_host("opencode"), "installed");
    assert_eq!(by_host("claude"), "checkout");
    assert_eq!(by_host("agents"), "real-file");
    assert_eq!(wired.installed_version.as_deref(), Some("1.0"));
    assert_eq!(wired.bundled_version.as_deref(), Some("1.0"));
}

// MARK: - The manifest

#[test]
fn the_manifest_records_every_link_and_edit_and_survives_the_replace() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");

    let manifest = setup::manifest(&f.tandem_home).expect("the manifest");
    assert_eq!(manifest.version, "1.0");
    assert_eq!(manifest.hosts, ["opencode"]);
    assert_eq!(manifest.links.len(), f.skills.len() + f.prompts.len());
    assert!(manifest.links.iter().all(|link| link.kind == "symlink"));
    assert!(
        manifest.links.iter().any(|link| {
            link.path == setup::text(&f.home.join(".config/opencode/skills/alpha"))
        })
    );
    assert!(
        manifest
            .edits
            .iter()
            .any(|edit| edit.kind == "instructions-entry")
    );

    write_bundle(&f.bundle, &["alpha", "beta"], &["builder"], "2.0");
    install(&f, &[Host::Opencode], false).expect("the second install");
    let after = setup::manifest(&f.tandem_home).expect("the manifest");
    assert_eq!(after.version, "2.0");
    assert_eq!(
        after.links, manifest.links,
        "a replace does not touch the links"
    );
    assert_eq!(after.edits, manifest.edits);
}

#[test]
fn the_manifest_merges_across_two_runs() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the opencode install");
    install(&f, &[Host::Claude], false).expect("the claude install");

    let manifest = setup::manifest(&f.tandem_home).expect("the manifest");
    assert_eq!(manifest.hosts, ["opencode", "claude"]);
    assert!(
        manifest.links.iter().any(|link| {
            link.path == setup::text(&f.home.join(".config/opencode/skills/alpha"))
        })
    );
    assert!(
        manifest
            .links
            .iter()
            .any(|link| link.path == setup::text(&f.home.join(".claude/skills/alpha")))
    );
    assert!(
        manifest
            .edits
            .iter()
            .any(|edit| edit.kind == "instructions-entry")
    );
    assert!(
        manifest
            .edits
            .iter()
            .any(|edit| edit.kind == "claude-block")
    );
}

// MARK: - Uninstall

#[test]
fn uninstall_removes_exactly_what_install_wrote() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let before = tree(&f.root);
    install(&f, &[Host::Opencode, Host::Claude, Host::Agents], false).expect("the install runs");

    let report =
        setup::uninstall(&f.tandem_home, &f.environment, None).expect("the uninstall runs");

    assert_eq!(report.verb, "uninstall");
    assert_eq!(report.manifest, "used");
    assert_eq!(
        tree(&f.root),
        before,
        "the home returns to its pre-install state"
    );
    assert!(!f.face().exists());
    assert!(!f.manifest().exists());
}

#[test]
fn uninstall_leaves_a_repointed_link_and_says_so() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = f.home.join(".config/opencode/skills/alpha");
    let checkout = f.root.join("checkout/skills/alpha");
    fs::create_dir_all(&checkout).unwrap();
    fs::remove_file(&path).unwrap();
    link(&path, &checkout);

    let report =
        setup::uninstall(&f.tandem_home, &f.environment, None).expect("the uninstall runs");

    assert!(path.exists());
    assert_eq!(action(&report, &setup::text(&path)).unwrap().action, "left");
    assert!(
        action(&report, &setup::text(&path))
            .unwrap()
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("points elsewhere"))
    );
}

#[test]
fn uninstall_without_a_manifest_removes_the_face_and_reports_unattributed() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let link_path = f.home.join(".config/opencode/skills/alpha");
    fs::remove_file(f.manifest()).unwrap();

    let report =
        setup::uninstall(&f.tandem_home, &f.environment, None).expect("the uninstall runs");

    assert_eq!(report.manifest, "absent");
    assert_eq!(report.unattributed, Some(true));
    assert!(!f.face().exists());
    assert!(is_symlink(&link_path), "an unattributed link is left");
}

#[test]
fn install_uninstall_install_returns_to_the_same_state() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode, Host::Claude], false).expect("the first install");
    let installed = tree(&f.root);
    let manifest = fs::read(f.manifest()).unwrap();

    setup::uninstall(&f.tandem_home, &f.environment, None).expect("the uninstall");
    install(&f, &[Host::Opencode, Host::Claude], false).expect("the second install");

    assert_eq!(tree(&f.root), installed);
    assert_eq!(fs::read(f.manifest()).unwrap(), manifest);
}

#[test]
fn a_second_uninstall_is_a_clean_no_op() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    setup::uninstall(&f.tandem_home, &f.environment, None).expect("the first uninstall");

    let again =
        setup::uninstall(&f.tandem_home, &f.environment, None).expect("the second uninstall");
    assert_eq!(again.manifest, "absent");
    assert!(
        again.paths.iter().all(|action| action.action == "absent"),
        "nothing left to remove"
    );
}

// MARK: - The OpenCode config edit

#[test]
fn the_config_merge_is_idempotent_and_preserves_the_human_config() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(f.opencode_config().parent().unwrap()).unwrap();
    let original = "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \
                    \"instructions\": [\"/somewhere/AGENTS.md\"],\n  \"mcp\": {}\n}\n\n";
    fs::write(f.opencode_config(), original).unwrap();

    install(&f, &[Host::Opencode], false).expect("the install runs");
    let merged = fs::read_to_string(f.opencode_config()).unwrap();
    assert!(
        merged.contains("/somewhere/AGENTS.md"),
        "the human's entry survives"
    );
    assert!(merged.contains(&setup::text(&f.face().join("AGENTS.md"))));
    assert!(merged.contains("$schema"));
    assert!(merged.contains("\"mcp\""));

    install(&f, &[Host::Opencode], false).expect("the second install");
    assert_eq!(
        fs::read_to_string(f.opencode_config()).unwrap(),
        merged,
        "a second install does not append a second entry"
    );
}

#[test]
fn an_unparseable_config_is_a_skip_and_the_skills_are_still_linked() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(f.opencode_config().parent().unwrap()).unwrap();
    fs::write(f.opencode_config(), "not json at all {").unwrap();

    let report = install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = setup::text(&f.opencode_config());
    assert_eq!(action(&report, &path).unwrap().action, "skip");
    assert!(
        action(&report, &path)
            .unwrap()
            .reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert!(is_symlink(&f.home.join(".config/opencode/skills/alpha")));
}

#[test]
fn a_jsonc_config_is_read_and_removal_is_byte_exact() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(f.opencode_config().parent().unwrap()).unwrap();
    let original = "{\n  // the provider list\n  \"providers\": {\n    \
                    \"gianni-desktop\": { \"models\": [\"medium\",], },\n  },\n}\n\n";
    fs::write(f.opencode_config(), original).unwrap();

    install(&f, &[Host::Opencode], false).expect("the install runs");
    let merged = fs::read_to_string(f.opencode_config()).unwrap();
    assert!(merged.contains(&setup::text(&f.face().join("AGENTS.md"))));
    assert!(
        merged.contains("// the provider list"),
        "the comment survives"
    );

    setup::uninstall(&f.tandem_home, &f.environment, None).expect("the uninstall");
    assert_eq!(
        fs::read_to_string(f.opencode_config()).unwrap(),
        original,
        "removal is byte-exact"
    );
}

#[test]
fn an_existing_entry_is_ok_and_the_file_is_not_rewritten() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let entry = setup::text(&f.face().join("AGENTS.md"));
    fs::create_dir_all(f.opencode_config().parent().unwrap()).unwrap();
    let original = format!("{{\n  \"instructions\": [\"{entry}\"]\n}}\n");
    fs::write(f.opencode_config(), &original).unwrap();

    let report = install(&f, &[Host::Opencode], false).expect("the install runs");
    assert_eq!(
        action(&report, &setup::text(&f.opencode_config()))
            .unwrap()
            .action,
        "ok"
    );
    assert_eq!(fs::read_to_string(f.opencode_config()).unwrap(), original);
}

// MARK: - The CLAUDE.md block

#[test]
fn the_claude_block_is_created_appended_and_replaced_in_place() {
    let absent = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&absent, &[Host::Claude], false).expect("the claude install");
    let claude_md = absent.home.join(".claude/CLAUDE.md");
    let created = fs::read_to_string(&claude_md).unwrap();
    assert!(created.contains("<!-- tandem:begin -->"));
    assert!(created.contains(&setup::text(&absent.face().join("AGENTS.md"))));
    assert!(created.contains("<!-- tandem:end -->"));

    let plain = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let plain_md = plain.home.join(".claude/CLAUDE.md");
    fs::create_dir_all(plain_md.parent().unwrap()).unwrap();
    fs::write(&plain_md, "the human's own rules\n").unwrap();
    install(&plain, &[Host::Claude], false).expect("the claude install");
    let appended = fs::read_to_string(&plain_md).unwrap();
    assert!(appended.starts_with("the human's own rules\n"));
    assert!(appended.contains("<!-- tandem:begin -->"));

    install(&plain, &[Host::Claude], false).expect("the second claude install");
    assert_eq!(fs::read_to_string(&plain_md).unwrap(), appended);
    assert_eq!(appended.matches("<!-- tandem:begin -->").count(), 1);

    setup::uninstall(&plain.tandem_home, &plain.environment, None).expect("the uninstall");
    assert_eq!(
        fs::read_to_string(&plain_md).unwrap(),
        "the human's own rules\n",
        "uninstall leaves the rest of the file byte-identical"
    );
}

// MARK: - The warning model

#[test]
fn no_bundle_is_info_on_a_dev_build_and_warn_inside_an_app() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");

    let dev = setup::state(
        &f.tandem_home,
        &f.environment,
        None,
        Path::new("/tmp/build/tandem-cli"),
        true,
    );
    assert!(
        dev.warnings
            .iter()
            .any(|warning| warning.code == Code::NoBundle && warning.severity == Severity::Info)
    );

    let app = setup::state(
        &f.tandem_home,
        &f.environment,
        None,
        Path::new("/Applications/Tandem.app/Contents/MacOS/tandem-cli"),
        false,
    );
    assert!(
        app.warnings
            .iter()
            .any(|warning| warning.code == Code::NoBundle && warning.severity == Severity::Warn)
    );
}

#[test]
fn an_agent_directory_with_no_manifest_warns_no_manifest() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(f.face()).unwrap();
    fs::write(f.face().join("VERSION"), "1.0").unwrap();
    let list = setup::state(
        &f.tandem_home,
        &f.environment,
        None,
        Path::new("/tmp/x"),
        true,
    )
    .warnings;
    assert!(has(&list, Code::NoManifest));
}

#[test]
fn an_installed_version_that_differs_from_the_bundle_warns_stale_face() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    write_bundle(&f.bundle, &["alpha", "beta"], &["builder"], "2.0");
    let stale = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::StaleFace)
        .expect("a stale face");
    assert!(stale.detail.contains("1.0"));
    assert!(stale.detail.contains("2.0"));
}

#[test]
fn a_face_without_a_version_marker_warns_missing_version() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(f.face().join("skills")).unwrap();
    let list = warnings(&f, None);
    assert!(has(&list, Code::MissingVersion));
    assert!(
        !has(&list, Code::StaleFace),
        "nothing to compare without a marker"
    );
    assert!(!has(&list, Code::PartialFace));
}

#[test]
fn a_face_with_a_different_skill_set_warns_partial_face() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    fs::remove_dir_all(f.face().join("skills/beta")).unwrap();
    fs::create_dir_all(f.face().join("skills/retired")).unwrap();
    let partial = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::PartialFace)
        .expect("a partial face");
    assert!(partial.detail.contains("missing: skills/beta"));
    assert!(partial.detail.contains("extra: skills/retired"));
}

#[test]
fn a_host_link_whose_target_is_gone_warns_dead_link() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = f.home.join(".config/opencode/skills/alpha");
    fs::remove_file(&path).unwrap();
    link(&path, &f.face().join("skills/ghost"));
    let dead = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::DeadLink)
        .expect("a dead link");
    assert_eq!(dead.path.as_deref(), Some(setup::text(&path).as_str()));
    assert!(dead.detail.contains("(gone)"));
}

#[test]
fn a_link_into_the_face_the_manifest_does_not_record_warns_orphan_link() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    fs::remove_file(f.manifest()).unwrap();
    assert!(has(&warnings(&f, None), Code::OrphanLink));
}

#[test]
fn a_checkout_link_is_info_and_warns_shadowing_beside_a_current_install() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    let checkout = f.root.join("checkout/skills/alpha");
    fs::create_dir_all(&checkout).unwrap();
    let path = f.home.join(".config/opencode/skills/alpha");
    link(&path, &checkout);

    let bare = warnings(&f, None);
    assert!(
        bare.iter()
            .any(|warning| warning.code == Code::ForeignLink && warning.severity == Severity::Info)
    );

    install(&f, &[Host::Opencode], true).expect("the forced install");
    fs::remove_file(&path).unwrap();
    link(&path, &checkout);
    let shadowed = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::CheckoutShadowsInstall)
        .expect("a shadowed install");
    assert_eq!(shadowed.severity, Severity::Warn);
    assert!(shadowed.detail.contains("beside a current install"));
}

#[test]
fn a_real_file_at_a_host_path_warns_blocked_path() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = f.home.join(".config/opencode/agents/builder.md");
    fs::remove_file(&path).unwrap();
    fs::write(&path, "mine\n").unwrap();
    let blocked = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::BlockedPath)
        .expect("a blocked path");
    assert_eq!(blocked.path.as_deref(), Some(setup::text(&path).as_str()));
    assert!(blocked.detail.contains("holds a real file"));
}

#[test]
fn a_global_agents_md_beside_the_instructions_entry_warns_double_load() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let global = f.home.join(".config/opencode/AGENTS.md");
    fs::write(&global, "the old route\n").unwrap();
    let warning = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::DoubleLoad)
        .expect("a double load");
    assert!(warning.detail.contains("both load"));
}

#[test]
fn a_host_disabled_by_an_opencode_variable_warns() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Claude, Host::Agents], false).expect("the install runs");
    assert!(
        !has(&warnings(&f, None), Code::HostDisabled),
        "no variable set, no warning"
    );

    let mut environment = f.environment.clone();
    environment.insert("OPENCODE_DISABLE_CLAUDE_CODE".into(), "true".into());
    environment.insert("OPENCODE_DISABLE_EXTERNAL_SKILLS".into(), "1".into());
    let disabled = setup::state(
        &f.tandem_home,
        &environment,
        Some(&f.bundle),
        Path::new("/tmp/tandem-cli"),
        true,
    )
    .warnings;
    assert!(
        disabled
            .iter()
            .any(|warning| warning.code == Code::HostDisabled && warning.detail.contains("claude"))
    );
    assert!(
        disabled
            .iter()
            .any(|warning| warning.code == Code::HostDisabled && warning.detail.contains("agents"))
    );
}

#[test]
fn a_host_with_its_links_but_no_instruction_file_warns_half_installed() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    fs::remove_file(f.opencode_config()).unwrap();

    let warning = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::HalfInstalled)
        .expect("a half install");
    assert!(warning.detail.contains("no instruction file"));
    assert!(warning.hint.contains("setup install --host opencode"));
}

#[test]
fn an_unwritable_home_is_an_error() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    fs::create_dir_all(&f.tandem_home).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&f.tandem_home, fs::Permissions::from_mode(0o555)).unwrap();
    }
    let warning = warnings(&f, None)
        .into_iter()
        .find(|warning| warning.code == Code::Unwritable)
        .expect("an unwritable home");
    assert_eq!(warning.severity, Severity::Error);
    assert!(warning.detail.contains("is not writable"));
}

#[test]
fn the_warning_codes_are_pinned() {
    let codes: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "no-bundle",
            "no-manifest",
            "stale-face",
            "missing-version",
            "partial-face",
            "dead-link",
            "orphan-link",
            "foreign-link",
            "checkout-shadows-install",
            "blocked-path",
            "double-load",
            "host-disabled",
            "half-installed",
            "unwritable",
        ]
    );
    assert_eq!(
        [Severity::Error, Severity::Warn, Severity::Info]
            .iter()
            .map(|severity| severity.as_str())
            .collect::<Vec<_>>(),
        ["error", "warn", "info"]
    );
}

#[test]
fn warnings_sort_by_severity_then_code_then_path_and_one_path_carries_one_code() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let alpha = f.home.join(".config/opencode/skills/alpha");
    let beta = f.home.join(".config/opencode/skills/beta");
    fs::remove_file(&alpha).unwrap();
    link(&alpha, &f.face().join("skills/ghost"));
    fs::remove_file(&beta).unwrap();
    fs::remove_file(f.opencode_config()).unwrap();

    let list = warnings(&f, None);
    let keys: Vec<(Severity, String, String)> = list
        .iter()
        .map(|warning| {
            (
                warning.severity,
                warning.code.as_str().to_string(),
                warning.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);

    let paths: Vec<&String> = list
        .iter()
        .filter_map(|warning| warning.path.as_ref())
        .collect();
    let unique: std::collections::HashSet<&&String> = paths.iter().collect();
    assert_eq!(paths.len(), unique.len(), "one path carries one code");
}

#[test]
fn warnings_sort_by_the_code_string_and_severity_not_the_enum_declaration() {
    let list = setup::ordered(vec![
        setup::Warning::new(Code::NoManifest, Severity::Warn, None, "b", "h"),
        setup::Warning::new(
            Code::BlockedPath,
            Severity::Warn,
            Some("/x".to_string()),
            "a",
            "h",
        ),
        setup::Warning::new(Code::NoBundle, Severity::Info, None, "c", "h"),
        setup::Warning::new(
            Code::Unwritable,
            Severity::Error,
            Some("/y".into()),
            "d",
            "h",
        ),
    ]);
    assert_eq!(
        list.iter().map(|warning| warning.code).collect::<Vec<_>>(),
        [
            Code::Unwritable,
            Code::BlockedPath,
            Code::NoManifest,
            Code::NoBundle,
        ],
        "error first, then the code's raw string, then info"
    );
}

#[test]
fn a_dead_link_to_a_retired_skill_is_one_dead_link_not_also_an_orphan() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = f.home.join(".config/opencode/skills/alpha");
    fs::remove_file(&path).unwrap();
    link(&path, &f.face().join("skills/retired"));
    fs::remove_file(f.manifest()).unwrap();

    let list = warnings(&f, None);
    let codes: Vec<Code> = list
        .iter()
        .filter(|warning| warning.path.as_deref() == Some(setup::text(&path).as_str()))
        .map(|warning| warning.code)
        .collect();
    assert_eq!(codes, [Code::DeadLink]);
}

#[test]
fn status_and_install_print_the_same_warning_list() {
    let f = fixture(&["alpha", "beta"], &["builder"], "1.0");
    install(&f, &[Host::Opencode], false).expect("the install runs");
    let path = f.home.join(".config/opencode/skills/alpha");
    fs::remove_file(&path).unwrap();
    link(&path, &f.root.join("checkout/alpha"));

    let installed = install(&f, &[Host::Opencode], false).expect("the second install");
    assert_eq!(installed.state.warnings, state(&f, None).warnings);
}
