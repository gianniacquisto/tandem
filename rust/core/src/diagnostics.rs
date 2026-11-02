//! The setup report (ADR 0014), ported from `SetupDiagnostics.swift`.
//!
//! The gathering is impure, it stats the filesystem and probes the lock, but it
//! produces a [`Facts`] value and [`report`] is a pure function of that value.
//! So the report's shape, wording and pass/fail rule are covered with injected
//! facts, and the CLI only wires the real world to it.
//!
//! The eleven rows, in the pinned order: `home`, `store`, `bikeshed`, `comments`,
//! `sessions`, `settings`, `todos-json`, `lock`, `app`, `skills`, `agent-face`.
//! The first four and the last can fail; `app`, `skills` and the three leftover
//! rows are informational. `ok` is the AND of the rows that can fail, so a held
//! lock, an unreadable store or a bikeshed path that is a file makes the CLI
//! exit 1.
//!
//! The leftovers are the JSON-era files the store retired: `sessions.json` and
//! `settings.json` each with its `.pre-sqlite-*` copy, and the `todos.json.*`
//! cohort. Each row names the files it found and the command that removes them
//! (TAN-243).

use std::path::{Path, PathBuf};

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::setup::{self, Environment, State};
use crate::store::{self, BackupHealth, Health, Home, HomeSource, Store};

pub use crate::store::CommentsState;
use crate::wire::{DoctorCheck, DoctorReport, slashed_path};

/// The check order, pinned: the CLI documents and tests it.
pub const CHECK_NAMES: [&str; 11] = [
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
    "agent-face",
];

/// An agent host whose skills directory was checked. Informational: a host that
/// is not installed is not a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillHost {
    pub name: String,
    pub path: String,
    pub found: bool,
}

/// Raw observations from the machine, injected as a value so [`report`] stays
/// pure and deterministic under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub home_path: PathBuf,
    pub home_source: HomeSource,
    pub store_path: PathBuf,
    /// The applied schema-migration version, `None` when there is no database.
    pub store_version: Option<i64>,
    /// A JSON store file sitting beside the database: a fork the port never
    /// reads, named rather than silently ignored.
    pub store_json_fork: Option<PathBuf>,
    /// The JSON store file that is still the store. The port has no JSON
    /// read-fallback, so gathering never sets it; the shape stays because the
    /// store row's wording is a ported contract.
    pub store_json_source: Option<PathBuf>,
    /// A database is present but its ticket import has not committed.
    pub store_database_unimported: bool,
    pub store_health: Health,
    pub store_backup: BackupHealth,
    pub store_lock_held: bool,
    pub bikeshed_path: PathBuf,
    pub bikeshed_note_count: usize,
    /// The bikeshed path exists but is a regular file, not a folder.
    pub bikeshed_is_file: bool,
    pub comments_path: PathBuf,
    pub comments_state: CommentsState,
    pub comment_rows: i64,
    pub comments_imported: bool,
    /// The JSON-era leftovers the store retired, one list per row (TAN-243):
    /// `sessions.json` with its `.pre-sqlite-*` copy, `settings.json` with its
    /// own, and the `todos.json.*` cohort. Named in every home, because a file
    /// nothing reads should not be found by `ls`; empty is the clean home. Only
    /// `sessions.json` and `settings.json` are import evidence, so their rows
    /// print a removal command only beside a database (ADR 0031: the store's
    /// schema is the marker that the domain moved). The cohort is ADR 0030's
    /// retirement and needs no marker.
    pub sessions_json: Vec<PathBuf>,
    pub settings_json: Vec<PathBuf>,
    pub todos_json: Vec<PathBuf>,
    pub app_present: bool,
    pub app_version: Option<String>,
    pub app_quarantined: bool,
    pub skill_hosts: Vec<SkillHost>,
    pub opencode_has_skills: bool,
    /// The agent face's state (ADR 0034), from [`setup::state`]. `None` in a
    /// test that injects no state, which reads as no warnings.
    pub agent_face: Option<State>,
}

/// Assemble the report from observed `facts`. Pure.
pub fn report(facts: &Facts) -> DoctorReport {
    let mut checks: Vec<DoctorCheck> = Vec::new();

    checks.push(DoctorCheck {
        name: "home".to_string(),
        ok: true,
        detail: format!(
            "{} ({})",
            slashed_path(&facts.home_path),
            home_why(facts.home_source)
        ),
    });

    checks.push(store_check(facts));
    checks.push(bikeshed_check(facts));
    checks.push(comments_check(facts));
    // The two imported sidecars print their removal only beside a database: a
    // home that never opened the store has imported nothing, and the row names
    // what it sees instead of claiming the file is absent.
    let store_present = !matches!(facts.store_health, Health::Absent);
    checks.push(sidecar_check(
        "sessions",
        "sessions.json",
        SIDECAR_RETIRED,
        &facts.sessions_json,
        store_present,
    ));
    checks.push(sidecar_check(
        "settings",
        "settings.json",
        SIDECAR_RETIRED,
        &facts.settings_json,
        store_present,
    ));
    // The `todos.json.*` cohort is ADR 0030's retirement, not import evidence,
    // so it is removable in any home, database or not.
    checks.push(sidecar_check(
        "todos-json",
        "todos.json.* files",
        TODOS_RETIRED,
        &facts.todos_json,
        true,
    ));
    checks.push(DoctorCheck {
        name: "lock".to_string(),
        ok: !facts.store_lock_held,
        detail: if facts.store_lock_held {
            "held by another process \u{2014} retry once it finishes".to_string()
        } else {
            "free".to_string()
        },
    });
    checks.push(app_check(facts));
    checks.push(DoctorCheck {
        name: "skills".to_string(),
        ok: true,
        detail: skill_detail(facts),
    });
    checks.push(agent_face_check(facts));

    let ok = checks.iter().all(|check| check.ok);
    DoctorReport { ok, checks }
}

/// Why the home resolved where it did, in the words `doctor` prints. The port
/// has one home (`~/.tandem`), so the Swift's shipped-app/dev-build split is
/// replaced by the flag and the default.
fn home_why(source: HomeSource) -> &'static str {
    match source {
        HomeSource::Flag => "--home",
        HomeSource::Environment => "TANDEM_HOME override",
        HomeSource::Default => "default",
    }
}

fn store_check(facts: &Facts) -> DoctorCheck {
    let backup = backup_detail(facts.store_backup);
    let version = facts
        .store_version
        .map(|version| format!("v{version}; "))
        .unwrap_or_default();
    let fork = facts
        .store_json_fork
        .as_ref()
        .map(|path| {
            format!(
                "; a JSON store file is also present at {} \u{2014} a fork, ignored since the \
                 database took over; remove it with: rm {}",
                slashed_path(path),
                shell_quoted(path)
            )
        })
        .unwrap_or_default();
    let path = slashed_path(&facts.store_path);

    match facts.store_health {
        Health::Absent => DoctorCheck {
            name: "store".to_string(),
            ok: true,
            detail: format!("{path} \u{2014} absent (a valid empty store); {backup}"),
        },
        Health::Counts { open, done } => {
            if let Some(source) = &facts.store_json_source {
                let pending = if facts.store_database_unimported {
                    format!("; the database at {path} has not imported it yet")
                } else {
                    String::new()
                };
                DoctorCheck {
                    name: "store".to_string(),
                    ok: true,
                    detail: format!(
                        "{} \u{2014} {open} open, {done} done{pending}; {backup}",
                        slashed_path(source)
                    ),
                }
            } else {
                DoctorCheck {
                    name: "store".to_string(),
                    ok: true,
                    detail: format!(
                        "{path} \u{2014} {version}{open} open, {done} done; {backup}{fork}"
                    ),
                }
            }
        }
        Health::Legacy { version } => DoctorCheck {
            name: "store".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} schema v{version}, which this build does not read; import it \
                 once with Scripts/import-legacy.py (PRT-18); {backup}"
            ),
        },
        Health::Unreadable => {
            if let Some(source) = &facts.store_json_source {
                DoctorCheck {
                    name: "store".to_string(),
                    ok: false,
                    detail: format!(
                        "{} \u{2014} present but unreadable or corrupt; fix the file \u{2014} the \
                         store is not rewritten; {backup}",
                        slashed_path(source)
                    ),
                }
            } else {
                DoctorCheck {
                    name: "store".to_string(),
                    ok: false,
                    detail: format!(
                        "{path} \u{2014} present but unreadable or corrupt; fix the file, or \
                         restore it from tandem.db.bak (remove tandem.db-wal and tandem.db-shm \
                         first); {backup}"
                    ),
                }
            }
        }
    }
}

fn bikeshed_check(facts: &Facts) -> DoctorCheck {
    let path = slashed_path(&facts.bikeshed_path);
    if facts.bikeshed_is_file {
        DoctorCheck {
            name: "bikeshed".to_string(),
            ok: false,
            detail: format!("{path} \u{2014} a file, not a folder; pick a folder"),
        }
    } else {
        DoctorCheck {
            name: "bikeshed".to_string(),
            ok: true,
            detail: format!("{path} \u{2014} {} note(s)", facts.bikeshed_note_count),
        }
    }
}

/// A path as one POSIX shell word: single-quoted, with an embedded `'` written
/// `'\''`. Both commands the report prints remove a path that comes from the
/// environment, so the quoting is what keeps the delete inside the path it
/// names.
pub fn shell_quoted(path: &Path) -> String {
    format!("'{}'", slashed_path(path).replace('\'', "'\\''"))
}

/// The comments row: the live log is the database, and `<home>/comments/` is
/// the import's evidence. The port has no importer: a directory beside a port
/// store is evidence of an import that already happened, never something the
/// port reads. So a present directory is named with the command that removes
/// it, and an absent one is the clean state after the human runs that command.
fn comments_check(facts: &Facts) -> DoctorCheck {
    let path = slashed_path(&facts.comments_path);
    let store = slashed_path(&facts.store_path);
    if facts.comments_imported {
        let live = format!("{store} \u{2014} {} comment(s)", facts.comment_rows);
        let removal = format!(
            "; remove them with: rm -rf {}",
            shell_quoted(&facts.comments_path)
        );
        let detail = match facts.comments_state {
            CommentsState::Absent => format!("{live}; no legacy log directory"),
            CommentsState::Readable => {
                format!("{live}; legacy logs at {path} already imported{removal}")
            }
            CommentsState::Unreadable => format!(
                "{live}; legacy logs at {path} present but unreadable \u{2014} already \
                 imported, so not read{removal}"
            ),
        };
        return DoctorCheck {
            name: "comments".to_string(),
            ok: true,
            detail,
        };
    }
    match facts.comments_state {
        CommentsState::Absent => DoctorCheck {
            name: "comments".to_string(),
            ok: true,
            detail: format!("{store} \u{2014} none yet; the first comment writes a row"),
        },
        CommentsState::Readable => DoctorCheck {
            name: "comments".to_string(),
            ok: true,
            detail: format!(
                "{path} \u{2014} legacy logs, still the store; the first comment imports them \
                 into {store}"
            ),
        },
        CommentsState::Unreadable => DoctorCheck {
            name: "comments".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} present but unreadable; the legacy logs are the store until \
                 the first comment imports them"
            ),
        },
    }
}

/// What `sessions.json` and `settings.json` are now: imported once, never read.
const SIDECAR_RETIRED: &str = "imported into the store, never read again";

/// What the `todos.json.*` cohort is: the retired board, and not a fork.
const TODOS_RETIRED: &str = "the retired JSON board, never read again";

/// One row per retired JSON file group (TAN-243): the files it found, in the
/// order the gathering sorted them, and, when the files may be removed, one
/// command that removes them all.
///
/// Informational in both states, like `comments`: a leftover is disk the human
/// may clear, and a clean home is the same row saying so. `removable` is false
/// for an imported sidecar in a home with no database: nothing there has
/// imported it, so the row names the file and prints no `rm` rather than
/// claiming the file is absent. The command is built by [`shell_quoted`], so a
/// home the shell would split stays one path.
fn sidecar_check(
    name: &str,
    file: &str,
    retired: &str,
    files: &[PathBuf],
    removable: bool,
) -> DoctorCheck {
    if files.is_empty() {
        return DoctorCheck {
            name: name.to_string(),
            ok: true,
            detail: format!("no {file} in this home"),
        };
    }
    let named = files
        .iter()
        .map(|path| slashed_path(path))
        .collect::<Vec<_>>()
        .join(", ");
    let pronoun = if files.len() == 1 { "it" } else { "them" };
    let detail = if removable {
        let commands = files
            .iter()
            .map(|path| shell_quoted(path))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{named} \u{2014} {retired}; remove {pronoun} with: rm -f {commands}")
    } else {
        format!("{named} \u{2014} present; no store in this home has imported {pronoun} yet")
    };
    DoctorCheck {
        name: name.to_string(),
        ok: true,
        detail,
    }
}

/// The rolling backup's clause for the store check: presence and age.
fn backup_detail(health: BackupHealth) -> String {
    match health {
        BackupHealth::Absent => "no backup yet".to_string(),
        BackupHealth::Present { age } => format!("backup {} old", age_text(age.as_secs_f64())),
    }
}

/// An age in the shortest unit that still reads as one: seconds, minutes,
/// hours, days. Coarse on purpose.
pub fn age_text(age: f64) -> String {
    let seconds = age.round().max(0.0) as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3_600 {
        return format!("{}m", seconds / 60);
    }
    if seconds < 86_400 {
        return format!("{}h", seconds / 3_600);
    }
    format!("{}d", seconds / 86_400)
}

fn app_check(facts: &Facts) -> DoctorCheck {
    if facts.app_present {
        let version = facts
            .app_version
            .as_ref()
            .map(|version| format!("version {version}"))
            .unwrap_or_else(|| "version unknown".to_string());
        let quarantine = if facts.app_quarantined {
            format!(
                "; quarantined \u{2014} run: xattr -dr com.apple.quarantine {}",
                APP_PATH
            )
        } else {
            String::new()
        };
        DoctorCheck {
            name: "app".to_string(),
            ok: true,
            detail: format!("installed at {APP_PATH} ({version}{quarantine})"),
        }
    } else {
        DoctorCheck {
            name: "app".to_string(),
            ok: true,
            detail: format!("not installed at {APP_PATH}"),
        }
    }
}

/// The `skills` row: the host list, the OpenCode route, and the one install
/// line. The route comes from the same state the `agent-face` row reads, so the
/// two rows cannot disagree.
fn skill_detail(facts: &Facts) -> String {
    let hosts = facts
        .skill_hosts
        .iter()
        .map(|host| {
            format!(
                "{} {}",
                host.name,
                if host.found { "found" } else { "not found" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut opencode = if facts.opencode_has_skills {
        "opencode configured".to_string()
    } else {
        "opencode not configured".to_string()
    };
    if let Some(host) = facts
        .agent_face
        .as_ref()
        .and_then(|state| state.hosts.iter().find(|host| host.host == "opencode"))
        && (host.state == "installed" || host.state == "checkout")
    {
        opencode.push_str(&format!(" ({})", host.state));
    }
    format!("{hosts}; {opencode}; install: tandem-cli setup install")
}

/// The `agent-face` row (ADR 0034): red while any `warn` or `error` stands,
/// naming the count, the first code and its hint. Informational warnings do not
/// turn `doctor` red: a checkout route is the normal state on a dev machine.
fn agent_face_check(facts: &Facts) -> DoctorCheck {
    let trouble: Vec<&setup::Warning> = facts
        .agent_face
        .as_ref()
        .map(|state| {
            state
                .warnings
                .iter()
                .filter(|warning| warning.severity != setup::Severity::Info)
                .collect()
        })
        .unwrap_or_default();
    let Some(first) = trouble.first() else {
        return DoctorCheck {
            name: "agent-face".to_string(),
            ok: true,
            detail: "no warnings".to_string(),
        };
    };
    let count = if trouble.len() == 1 {
        "1 warning".to_string()
    } else {
        format!("{} warnings", trouble.len())
    };
    DoctorCheck {
        name: "agent-face".to_string(),
        ok: false,
        detail: format!("{count}: {}, {}", first.code.as_str(), first.hint),
    }
}

/// The app the cask installs (ADR 0003). The shell's Windows and Linux install
/// paths land with PRT-12; until then the row names this one, which exists on no
/// other host, and reads as "not installed" there.
const APP_PATH: &str = "/Applications/Tandem.app";

// MARK: - Gathering (impure)

/// Gather the real machine's facts, then assemble. The CLI calls this.
pub fn run(home: &Home, environment: &Environment) -> DoctorReport {
    report(&gather(home, environment))
}

/// Gather the machine's facts. The only impure entry point besides [`run`].
pub fn gather(home: &Home, environment: &Environment) -> Facts {
    let store = Store::new(home.clone());
    let bikeshed = Bikeshed::new(home.directory().join("bikeshed"));
    let health = store.health();
    let notes = bikeshed
        .list()
        .map(|operation| operation.value)
        .unwrap_or_default();
    let bikeshed_path = home.directory().join("bikeshed");
    let bikeshed_is_file = bikeshed_path.exists() && !bikeshed_path.is_dir();
    let comments_path = home.directory().join("comments");
    let comments_state = store::comments_state(&comments_path);
    let comments_health = store.comments_health();

    let app_path = Path::new(APP_PATH);
    let app_present = app_path.exists();
    let app_version = app_present.then(|| app_version(app_path)).flatten();
    let app_quarantined = app_present && has_quarantine(app_path);

    let context = setup::Context::new(home.directory(), environment);
    let executable = std::env::current_exe().unwrap_or_default();
    let is_debug_build = cfg!(debug_assertions);
    let agent_face = Some(setup::state(
        home.directory(),
        environment,
        setup::bundle(environment, &executable, is_debug_build).as_deref(),
        &executable,
        is_debug_build,
    ));

    let store_json_fork = if store.database().exists() {
        ["tickets.json", "todos.json"]
            .into_iter()
            .map(|name| home.directory().join(name))
            .find(|path| path.exists())
    } else {
        None
    };

    // The JSON-era leftovers are read in every home, not only beside a
    // database: the `todos.json.*` cohort is ADR 0030's retirement and needs no
    // marker, and a home that has not opened the store still has files a
    // diagnostic should name. The report alone decides what may be removed,
    // because only a database can have imported `sessions.json` and
    // `settings.json` (ADR 0031).
    let sessions_json = sidecar_files(home.directory(), "sessions.json");
    let settings_json = sidecar_files(home.directory(), "settings.json");
    let todos_json = todos_cohort(home.directory());

    Facts {
        home_path: home.directory().to_path_buf(),
        home_source: home.source(),
        store_path: store.database(),
        store_version: store.schema_version().ok().flatten(),
        store_json_fork,
        store_json_source: None,
        store_database_unimported: false,
        store_health: health,
        store_backup: store.backup_health(),
        store_lock_held: store.lock_is_held(),
        bikeshed_path,
        bikeshed_note_count: note_count(&notes),
        bikeshed_is_file,
        comments_path,
        comments_state,
        comment_rows: comments_health.rows,
        comments_imported: comments_health.imported,
        sessions_json,
        settings_json,
        todos_json,
        app_present,
        app_version,
        app_quarantined,
        skill_hosts: skill_hosts(environment, &context),
        opencode_has_skills: opencode_has_skills(&context),
        agent_face,
    }
}

/// The JSON sidecar's files in a home: the file itself and any
/// `.pre-sqlite-*` copy beside it, in name order so the printed command does
/// not move between runs.
fn sidecar_files(directory: &Path, name: &str) -> Vec<PathBuf> {
    let copies = format!("{name}.pre-sqlite-");
    let mut files = Vec::new();
    if directory.join(name).exists() {
        files.push(directory.join(name));
    }
    files.extend(children(directory, &copies));
    files
}

/// The `todos.json.*` cohort: every retired-board file in the home. The bare
/// `todos.json` is not one of them, because the store row already names it as
/// a fork.
fn todos_cohort(directory: &Path) -> Vec<PathBuf> {
    children(directory, "todos.json.")
}

/// A home's children whose name starts with `prefix`, sorted. An unreadable
/// home is no files: a diagnostic reports what it can see.
fn children(directory: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn note_count(nodes: &[BikeshedNode]) -> usize {
    nodes
        .iter()
        .map(|node| usize::from(node.kind == NodeKind::Note) + note_count(&node.children))
        .sum()
}

fn skill_hosts(environment: &Environment, context: &setup::Context) -> Vec<SkillHost> {
    let user_home = environment
        .get("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| context.user_home());
    [
        ("agents", user_home.join(".agents/skills/tandem")),
        ("claude", user_home.join(".claude/skills/tandem")),
    ]
    .into_iter()
    .map(|(name, path)| SkillHost {
        name: name.to_string(),
        path: slashed_path(&path),
        found: path.exists(),
    })
    .collect()
}

/// Whether the agent host has Tandem's skills, by either route: the `skills`
/// array in the host's `opencode.json` listing a path containing `tandem/skills`
/// (this repo's skills directory), or the host's own skills directory holding
/// the linked `tandem` skill.
fn opencode_has_skills(context: &setup::Context) -> bool {
    if let Ok(bytes) = std::fs::read(context.opencode_config())
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(skills) = value.get("skills").and_then(|skills| skills.as_array())
        && skills
            .iter()
            .filter_map(|skill| skill.as_str())
            .any(|skill| skill.contains("tandem/skills"))
    {
        return true;
    }
    context.opencode_skills().join("tandem").exists()
}

/// `CFBundleShortVersionString` from the app's XML `Info.plist`. The bundle
/// script writes the XML form; a binary plist reads as no version, which the
/// row says honestly.
fn app_version(app: &Path) -> Option<String> {
    let text = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    let key = "<key>CFBundleShortVersionString</key>";
    let after = &text[text.find(key)? + key.len()..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")? + start;
    let version = after[start..end].trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// Whether the app carries the `com.apple.quarantine` extended attribute, the
/// flag Homebrew leaves on a cask download until it is stripped.
#[cfg(target_os = "macos")]
fn has_quarantine(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let name = c"com.apple.quarantine";
    // SAFETY: both pointers are valid NUL-terminated strings, and a null value
    // with size 0 asks only whether the attribute exists.
    unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) >= 0 }
}

#[cfg(not(target_os = "macos"))]
fn has_quarantine(_path: &Path) -> bool {
    false
}
