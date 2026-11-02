//! The setup report (ADR 0014), ported from `SetupDiagnosticsTests.swift`: the
//! pure assembly, and the store's health reads that feed it.
//!
//! The impure gathering is deliberately not tested here: it only reads the
//! machine. Everything with a decision in it is `report`, which these tests
//! drive with injected facts.

use std::path::PathBuf;

use tandem_core::diagnostics::{
    self, CHECK_NAMES, CommentsState, Facts, SkillHost, age_text, report, shell_quoted,
};
use tandem_core::domain::{AddTicket, Board};
use tandem_core::harness::HarnessAuth;
use tandem_core::setup::{self, Code, HostState, Severity, State, Warning};
use tandem_core::store::{BackupHealth, Health, Home, HomeSource, Store};

fn facts() -> Facts {
    Facts {
        home_path: PathBuf::from("/Users/tester/.tandem-dev"),
        home_source: HomeSource::Environment,
        store_path: PathBuf::from("/Users/tester/.tandem-dev/tandem.db"),
        store_version: Some(1),
        store_json_fork: None,
        store_json_source: None,
        store_database_unimported: false,
        store_health: Health::Counts { open: 3, done: 2 },
        store_backup: BackupHealth::Present {
            age: std::time::Duration::from_secs(300),
        },
        store_lock_held: false,
        bikeshed_path: PathBuf::from("/Users/tester/.tandem-dev/bikeshed"),
        bikeshed_note_count: 4,
        bikeshed_is_file: false,
        comments_path: PathBuf::from("/Users/tester/.tandem-dev/comments"),
        comments_state: CommentsState::Absent,
        comment_rows: 0,
        comments_imported: false,
        app_present: true,
        app_version: Some("0.9.0".to_string()),
        app_quarantined: false,
        skill_hosts: vec![
            SkillHost {
                name: "agents".to_string(),
                path: "/Users/tester/.agents/skills/tandem".to_string(),
                found: true,
            },
            SkillHost {
                name: "claude".to_string(),
                path: "/Users/tester/.claude/skills/tandem".to_string(),
                found: false,
            },
        ],
        opencode_has_skills: true,
        agent_face: None,
        harness_auth: HarnessAuth::NotStored,
        sessions_json: Vec::new(),
        settings_json: Vec::new(),
        todos_json: Vec::new(),
    }
}

/// The agent face's state, as `setup::state` would return it.
fn agent_face(opencode: &str, warnings: Vec<Warning>) -> State {
    State {
        bundled_version: Some("1.0".to_string()),
        installed_version: Some("1.0".to_string()),
        hosts: vec![
            HostState {
                host: "opencode".to_string(),
                state: opencode.to_string(),
                links: 2,
                expected_links: 2,
            },
            HostState {
                host: "claude".to_string(),
                state: "absent".to_string(),
                links: 0,
                expected_links: 0,
            },
            HostState {
                host: "agents".to_string(),
                state: "absent".to_string(),
                links: 0,
                expected_links: 0,
            },
        ],
        warnings,
    }
}

fn check<'a>(
    report: &'a tandem_core::wire::DoctorReport,
    name: &str,
) -> &'a tandem_core::wire::DoctorCheck {
    report
        .checks
        .iter()
        .find(|check| check.name == name)
        .unwrap_or_else(|| panic!("the report carries a `{name}` row"))
}

#[test]
fn a_healthy_machine_passes_with_the_checks_in_the_pinned_order() {
    let report = report(&facts());
    assert!(report.ok);
    assert_eq!(
        report
            .checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        CHECK_NAMES
    );
    assert!(report.checks.iter().all(|check| check.ok));
    assert!(report.checks[0].detail.contains("TANDEM_HOME override"));
    assert!(report.checks[1].detail.contains("3 open, 2 done"));
    assert!(report.checks[1].detail.contains("backup 5m old"));
    assert!(report.checks[2].detail.contains("4 note(s)"));
    assert!(
        report.checks[3]
            .detail
            .contains("the first comment writes a row")
    );
    assert!(check(&report, "app").detail.contains("version 0.9.0"));
}

#[test]
fn an_absent_store_is_a_valid_empty_setup() {
    let mut facts = facts();
    facts.store_health = Health::Absent;
    facts.store_version = None;
    let report = report(&facts);
    assert!(report.ok);
    assert!(check(&report, "store").detail.contains("absent"));
}

#[test]
fn an_unreadable_store_fails_the_report() {
    let mut facts = facts();
    facts.store_health = Health::Unreadable;
    let report = report(&facts);
    assert!(!report.ok);
    assert!(!check(&report, "store").ok);
}

#[test]
fn a_v5_file_is_named_as_the_import_not_as_corruption() {
    let mut facts = facts();
    facts.store_health = Health::Legacy { version: 5 };
    facts.store_version = None;
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(!store.ok);
    assert!(store.detail.contains("schema v5"));
    assert!(store.detail.contains("Scripts/import-legacy.py"));
    assert!(!store.detail.contains("corrupt"));
}

#[test]
fn the_store_check_reports_the_rolling_backup_or_its_absence() {
    let with_backup = report(&facts());
    assert!(with_backup.checks[1].detail.contains("backup 5m old"));
    assert!(with_backup.ok);

    let mut facts = facts();
    facts.store_backup = BackupHealth::Absent;
    let without = report(&facts);
    assert!(without.checks[1].detail.contains("no backup yet"));
    assert!(
        without.ok,
        "a store too young to have a backup is not a failure"
    );
}

#[test]
fn an_old_backup_on_a_corrupt_store_is_still_reported_beside_the_failure() {
    let mut facts = facts();
    facts.store_health = Health::Unreadable;
    facts.store_backup = BackupHealth::Present {
        age: std::time::Duration::from_secs(7_200),
    };
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(!store.ok);
    assert!(store.detail.contains("backup 2h old"));
}

#[test]
fn the_store_row_names_the_database_version_and_how_to_clear_the_fork() {
    let mut facts = facts();
    facts.store_json_fork = Some(PathBuf::from("/Users/tester/.tandem-dev/tickets.json"));
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(store.detail.contains("/Users/tester/.tandem-dev/tandem.db"));
    assert!(store.detail.contains("v1;"));
    assert!(store.detail.contains("a fork"));
    assert!(store.detail.contains("tickets.json"));
    assert!(
        store
            .detail
            .contains("rm '/Users/tester/.tandem-dev/tickets.json'"),
        "a fork nothing deletes names the command that clears it"
    );
}

/// The store row and the comments row share one quoting rule, so a path the
/// shell would split stays one word in both commands.
#[test]
fn the_store_fork_quotes_a_path_the_shell_would_split() {
    let mut facts = facts();
    facts.store_json_fork = Some(PathBuf::from("/Users/one two/tickets.json"));
    let report = report(&facts);
    assert!(
        check(&report, "store")
            .detail
            .contains("rm '/Users/one two/tickets.json'")
    );
}

#[test]
fn a_pre_import_home_names_the_json_file_not_the_database() {
    let mut facts = facts();
    facts.store_version = None;
    facts.store_json_source = Some(PathBuf::from("/Users/tester/.tandem-dev/tickets.json"));
    facts.store_database_unimported = true;
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(
        store
            .detail
            .contains("/Users/tester/.tandem-dev/tickets.json")
    );
    assert!(store.detail.contains("3 open, 2 done"));
    assert!(
        !store.detail.contains("v1"),
        "the schema is not the store's yet"
    );
    assert!(store.detail.contains("has not imported it yet"));
}

#[test]
fn a_json_only_home_names_the_file_and_has_no_database_to_mention() {
    let mut facts = facts();
    facts.store_version = None;
    facts.store_json_source = Some(PathBuf::from("/Users/tester/.tandem-dev/tickets.json"));
    facts.store_health = Health::Counts { open: 1, done: 0 };
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(
        store
            .detail
            .contains("/Users/tester/.tandem-dev/tickets.json")
    );
    assert!(
        !store.detail.contains("tandem.db"),
        "there is no database to name"
    );
    assert!(!store.detail.contains("has not imported"));
}

#[test]
fn an_unreadable_json_file_is_named_with_no_backup_recipe_for_a_database() {
    let mut facts = facts();
    facts.store_version = None;
    facts.store_json_source = Some(PathBuf::from("/Users/tester/.tandem-dev/tickets.json"));
    facts.store_health = Health::Unreadable;
    let report = report(&facts);
    let store = check(&report, "store");
    assert!(!store.ok);
    assert!(
        store
            .detail
            .contains("/Users/tester/.tandem-dev/tickets.json")
    );
    assert!(!store.detail.contains("tandem.db"));
    assert!(store.detail.contains("store is not rewritten"));
}

#[test]
fn backup_ages_read_as_seconds_minutes_hours_then_days() {
    assert_eq!(age_text(0.0), "0s");
    assert_eq!(age_text(59.0), "59s");
    assert_eq!(age_text(60.0), "1m");
    assert_eq!(age_text(3_599.0), "59m");
    assert_eq!(age_text(3_600.0), "1h");
    assert_eq!(age_text(86_399.0), "23h");
    assert_eq!(age_text(86_400.0), "1d");
}

#[test]
fn a_held_lock_fails_the_report_with_a_retry_hint() {
    let mut facts = facts();
    facts.store_lock_held = true;
    let report = report(&facts);
    assert!(!report.ok);
    let lock = check(&report, "lock");
    assert!(!lock.ok);
    assert!(lock.detail.contains("retry"));
}

#[test]
fn a_bikeshed_path_that_is_a_file_fails_the_report() {
    let mut facts = facts();
    facts.bikeshed_is_file = true;
    let report = report(&facts);
    assert!(!report.ok);
    assert!(!check(&report, "bikeshed").ok);
}

#[test]
fn the_comments_row_reads_the_live_store_not_the_legacy_directory() {
    let mut facts = facts();
    facts.comment_rows = 12;
    facts.comments_imported = true;
    let report = report(&facts);
    let comments = check(&report, "comments");
    assert!(report.ok);
    assert!(comments.ok);
    assert!(
        comments
            .detail
            .contains("/Users/tester/.tandem-dev/tandem.db")
    );
    assert!(comments.detail.contains("12 comment(s)"));
    assert!(!comments.detail.contains("created on first comment"));
    assert_eq!(
        comments.detail,
        "/Users/tester/.tandem-dev/tandem.db \u{2014} 12 comment(s); no legacy log directory"
    );
}

/// The evidence the import left behind is the human's to remove, so the row
/// prints the command, byte for byte, with the quoted directory.
#[test]
fn an_imported_directory_names_its_removal() {
    let mut facts = facts();
    facts.comment_rows = 12;
    facts.comments_imported = true;
    facts.comments_state = CommentsState::Readable;
    let report = report(&facts);
    let comments = check(&report, "comments");
    assert!(report.ok);
    assert!(comments.ok);
    assert_eq!(
        comments.detail,
        "/Users/tester/.tandem-dev/tandem.db \u{2014} 12 comment(s); legacy logs at \
         /Users/tester/.tandem-dev/comments already imported; remove them with: rm -rf \
         '/Users/tester/.tandem-dev/comments'"
    );
}

#[test]
fn legacy_logs_awaiting_the_import_are_named_as_still_to_import() {
    let mut facts = facts();
    facts.comments_state = CommentsState::Readable;
    let report = report(&facts);
    let comments = check(&report, "comments");
    assert!(report.ok);
    assert!(comments.ok);
    assert!(
        comments
            .detail
            .contains("/Users/tester/.tandem-dev/comments")
    );
    assert!(comments.detail.contains("still the store"));
    assert!(!comments.detail.contains("created on first comment"));
}

#[test]
fn an_unreadable_legacy_directory_fails_while_it_is_still_the_store() {
    let mut facts = facts();
    facts.comments_state = CommentsState::Unreadable;
    let report = report(&facts);
    let comments = check(&report, "comments");
    assert!(!report.ok);
    assert!(!comments.ok);
    assert!(
        comments
            .detail
            .contains("/Users/tester/.tandem-dev/comments")
    );
    assert!(comments.detail.contains("unreadable"));
}

#[test]
fn an_unreadable_legacy_directory_is_harmless_once_the_import_landed() {
    let mut facts = facts();
    facts.comments_state = CommentsState::Unreadable;
    facts.comment_rows = 5;
    facts.comments_imported = true;
    let report = report(&facts);
    let comments = check(&report, "comments");
    assert!(report.ok);
    assert!(comments.ok);
    assert!(comments.detail.contains("5 comment(s)"));
    assert!(comments.detail.contains("already imported"));
    assert_eq!(
        comments.detail,
        "/Users/tester/.tandem-dev/tandem.db \u{2014} 5 comment(s); legacy logs at \
         /Users/tester/.tandem-dev/comments present but unreadable \u{2014} already imported, so \
         not read; remove them with: rm -rf '/Users/tester/.tandem-dev/comments'"
    );
}

/// The JSON-era leftovers retire with the store, so each gets one row that names
/// the files it found and the command that removes them (TAN-243). A row is
/// green either way: a leftover is disk the human may clear, not a broken setup.
#[test]
fn the_sessions_row_names_the_imported_sidecar_and_its_removal() {
    let mut facts = facts();
    facts.sessions_json = vec![PathBuf::from("/Users/tester/.tandem-dev/sessions.json")];
    let report = report(&facts);
    assert!(report.ok, "a leftover does not fail the report");
    assert_eq!(
        check(&report, "sessions").detail,
        "/Users/tester/.tandem-dev/sessions.json \u{2014} imported into the store, never read \
         again; remove it with: rm -f '/Users/tester/.tandem-dev/sessions.json'"
    );
}

#[test]
fn the_settings_row_names_its_pre_import_copy_and_removes_both() {
    let mut facts = facts();
    facts.settings_json = vec![
        PathBuf::from("/Users/tester/.tandem-dev/settings.json"),
        PathBuf::from("/Users/tester/.tandem-dev/settings.json.pre-sqlite-1793309342"),
    ];
    let report = report(&facts);
    assert!(report.ok);
    assert_eq!(
        check(&report, "settings").detail,
        "/Users/tester/.tandem-dev/settings.json, \
         /Users/tester/.tandem-dev/settings.json.pre-sqlite-1793309342 \u{2014} imported into the \
         store, never read again; remove them with: rm -f \
         '/Users/tester/.tandem-dev/settings.json' \
         '/Users/tester/.tandem-dev/settings.json.pre-sqlite-1793309342'"
    );
}

/// The cohort is one row, not eight: the retired board's files are one fact
/// about the home, and one command takes all of them.
#[test]
fn the_todos_row_names_the_whole_cohort_with_one_removal() {
    let mut facts = facts();
    facts.todos_json = vec![
        PathBuf::from("/Users/tester/.tandem-dev/todos.json.bak"),
        PathBuf::from("/Users/tester/.tandem-dev/todos.json.bak-1792449127"),
        PathBuf::from("/Users/tester/.tandem-dev/todos.json.corrupt-1792239875"),
    ];
    let report = report(&facts);
    assert!(report.ok);
    assert_eq!(
        check(&report, "todos-json").detail,
        "/Users/tester/.tandem-dev/todos.json.bak, \
         /Users/tester/.tandem-dev/todos.json.bak-1792449127, \
         /Users/tester/.tandem-dev/todos.json.corrupt-1792239875 \u{2014} the retired JSON board, \
         never read again; remove them with: rm -f \
         '/Users/tester/.tandem-dev/todos.json.bak' \
         '/Users/tester/.tandem-dev/todos.json.bak-1792449127' \
         '/Users/tester/.tandem-dev/todos.json.corrupt-1792239875'"
    );
}

#[test]
fn a_home_with_no_leftovers_prints_one_clean_sentence_per_row() {
    let report = report(&facts());
    assert!(report.ok);
    for (name, detail) in [
        ("sessions", "no sessions.json in this home"),
        ("settings", "no settings.json in this home"),
        ("todos-json", "no todos.json.* files in this home"),
    ] {
        let row = check(&report, name);
        assert!(row.ok, "{name} is informational");
        assert_eq!(row.detail, detail);
    }
}

/// No database, no import: the row still names the file it sees, and prints no
/// `rm` for a leftover the store never carried over (TAN-243).
#[test]
fn a_home_without_a_store_names_its_sidecar_and_prints_no_removal() {
    let mut facts = facts();
    facts.store_health = Health::Absent;
    facts.store_version = None;
    facts.sessions_json = vec![PathBuf::from("/Users/tester/.tandem-dev/sessions.json")];
    let report = report(&facts);
    assert!(report.ok);
    assert_eq!(
        check(&report, "sessions").detail,
        "/Users/tester/.tandem-dev/sessions.json \u{2014} present; no store in this home has \
         imported it yet"
    );
    assert!(
        !check(&report, "sessions").detail.contains("rm -f"),
        "nothing imported it, so the row prints no removal command"
    );
}

/// One quoting rule for every printed delete: a home with a space or a quote
/// stays one word in the new rows too.
#[test]
fn a_leftover_path_the_shell_would_split_stays_one_word_in_its_command() {
    let mut facts = facts();
    facts.settings_json = vec![PathBuf::from("/Users/one two/o'brien/settings.json")];
    let report = report(&facts);
    assert!(
        check(&report, "settings")
            .detail
            .contains("rm -f '/Users/one two/o'\\''brien/settings.json'")
    );
}

/// The command is one shell word: a home with a space or a quote prints a
/// delete that still names only that directory.
#[test]
fn shell_quoting_survives_a_space_and_a_quote() {
    assert_eq!(
        shell_quoted(std::path::Path::new("/Users/tester/plain/comments")),
        "'/Users/tester/plain/comments'"
    );
    assert_eq!(
        shell_quoted(std::path::Path::new("/Users/one two/comments")),
        "'/Users/one two/comments'"
    );
    assert_eq!(
        shell_quoted(std::path::Path::new("/Users/o'brien/comments")),
        "'/Users/o'\\''brien/comments'"
    );
    assert_eq!(
        shell_quoted(std::path::Path::new("/tmp/x' ; rm -rf ~ #")),
        "'/tmp/x'\\'' ; rm -rf ~ #'"
    );
}

#[test]
fn a_missing_app_is_informational() {
    let mut facts = facts();
    facts.app_present = false;
    facts.app_version = None;
    let report = report(&facts);
    let app = check(&report, "app");
    assert!(app.ok);
    assert!(app.detail.contains("not installed"));
    assert!(report.ok);
}

#[test]
fn a_quarantined_app_names_the_quarantine_strip() {
    let mut facts = facts();
    facts.app_quarantined = true;
    let report = report(&facts);
    assert!(check(&report, "app").detail.contains("xattr -dr"));
}

#[test]
fn the_skills_check_reports_each_host_and_the_opencode_config() {
    let report = report(&facts());
    let skills = &check(&report, "skills").detail;
    assert!(skills.contains("agents found"));
    assert!(skills.contains("claude not found"));
    assert!(skills.contains("opencode configured"));
}

#[test]
fn the_skills_check_names_the_installed_and_checkout_routes_both_ok() {
    let mut installed = facts();
    installed.agent_face = Some(agent_face("installed", Vec::new()));
    let installed_report = report(&installed);
    let skills = check(&installed_report, "skills");
    assert!(skills.ok);
    assert!(skills.detail.contains("opencode configured (installed)"));
    assert!(skills.detail.contains("tandem-cli setup install"));

    let mut checkout = facts();
    checkout.agent_face = Some(agent_face("checkout", Vec::new()));
    let checkout_report = report(&checkout);
    let skills = check(&checkout_report, "skills");
    assert!(
        skills.ok,
        "a checkout route is the normal dev-machine state"
    );
    assert!(skills.detail.contains("opencode configured (checkout)"));
    assert!(skills.detail.contains("tandem-cli setup install"));
}

#[test]
fn the_agent_face_row_is_green_clean_and_red_with_the_count_on_a_dead_link() {
    let mut clean = facts();
    clean.agent_face = Some(agent_face("installed", Vec::new()));
    let clean_report = report(&clean);
    let row = check(&clean_report, "agent-face");
    assert!(row.ok);
    assert_eq!(row.detail, "no warnings");
    assert!(clean_report.ok);

    let info = Warning::new(
        Code::ForeignLink,
        Severity::Info,
        Some("/Users/tester/.config/opencode/skills/tandem".to_string()),
        "a checkout route",
        "none: a checkout route is expected on a dev machine",
    );
    let mut warned = facts();
    warned.agent_face = Some(agent_face("installed", vec![info]));
    assert!(
        check(&report(&warned), "agent-face").ok,
        "an informational warning does not turn doctor red"
    );

    let dead = Warning::new(
        Code::DeadLink,
        Severity::Warn,
        Some("/Users/tester/.config/opencode/skills/alpha".to_string()),
        "gone",
        "tandem-cli setup install",
    );
    let mut red = facts();
    red.agent_face = Some(agent_face("installed", vec![dead]));
    let red_report = report(&red);
    let row = check(&red_report, "agent-face");
    assert!(!row.ok);
    assert!(row.detail.contains("1 warning: dead-link"));
    assert!(row.detail.contains("tandem-cli setup install"));
    assert!(!red_report.ok, "a warn stands, so the report fails");
}

#[test]
fn the_harness_auth_row_reports_the_sign_in_state_without_failing() {
    let unsigned = report(&facts());
    let row = check(&unsigned, "harness-auth");
    assert!(
        row.ok,
        "an unsigned harness waits for a sign-in; nothing failed"
    );
    assert_eq!(row.detail, "not signed in; sign in from Settings");

    let mut stored = facts();
    stored.harness_auth = HarnessAuth::Stored {
        account: "Personal".to_string(),
        integrations: vec!["opencode".to_string(), "openrouter".to_string()],
    };
    let stored_report = report(&stored);
    let row = check(&stored_report, "harness-auth");
    assert!(row.ok);
    assert_eq!(
        row.detail,
        "signed in as Personal (opencode, openrouter stored)"
    );

    let mut unreadable = facts();
    unreadable.harness_auth = HarnessAuth::Unreadable {
        detail: "the harness store is not readable: file is not a database".to_string(),
    };
    let unreadable_report = report(&unreadable);
    let row = check(&unreadable_report, "harness-auth");
    assert!(
        row.ok,
        "an unreadable harness store names itself; it does not fail the home"
    );
    assert!(row.detail.contains("not readable"));
}

#[test]
fn the_home_check_names_which_branch_of_the_rule_this_process_takes() {
    let mut flag = facts();
    flag.home_source = HomeSource::Flag;
    assert!(report(&flag).checks[0].detail.contains("--home"));

    let mut default = facts();
    default.home_source = HomeSource::Default;
    assert!(report(&default).checks[0].detail.contains("default"));
}

// MARK: - Store.health / lock probe (the impure read's decisions)

fn temp_store(name: &str) -> (Store, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("tandem-doctor-tests-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = Home::resolve(Some(&root), None, &root.join("user"));
    (Store::new(home), root)
}

#[test]
fn health_is_absent_then_counts_then_unreadable() {
    let (store, root) = temp_store("health");
    assert_eq!(store.health(), Health::Absent);

    Board::new(store.clone())
        .add("Doctor probe", AddTicket::default())
        .expect("the ticket is written");
    assert_eq!(store.health(), Health::Counts { open: 1, done: 0 });

    // A file that is not a database where the store's database belongs.
    std::fs::write(store.database(), b"not a database").unwrap();
    assert_eq!(store.health(), Health::Unreadable);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_v5_file_health_is_legacy_not_unreadable() {
    let (store, root) = temp_store("legacy-health");
    std::fs::create_dir_all(&root).unwrap();
    let connection = rusqlite::Connection::open(store.database()).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER NOT NULL, applied_at TEXT);
             INSERT INTO schema_migrations VALUES (5, '2026-10-01T00:00:00Z');",
        )
        .unwrap();
    drop(connection);
    assert_eq!(store.health(), Health::Legacy { version: 5 });
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn comments_health_is_no_rows_then_the_first_comment() {
    let (store, root) = temp_store("comments");
    assert_eq!(
        store.comments_health(),
        tandem_core::store::CommentsHealth {
            rows: 0,
            imported: false
        }
    );
    let board = Board::new(store.clone());
    let ticket = board.add("Comment probe", AddTicket::default()).unwrap();
    board
        .append_comment(&ticket.id.to_string(), "builder", "one")
        .expect("the comment is written");
    assert_eq!(
        store.comments_health(),
        tandem_core::store::CommentsHealth {
            rows: 1,
            imported: true
        }
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_lock_probe_reports_free_when_nothing_holds_the_lock() {
    let (store, root) = temp_store("lock");
    assert!(!store.lock_is_held());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn backup_health_is_absent_then_ages() {
    let (store, root) = temp_store("backup");
    assert_eq!(store.backup_health(), BackupHealth::Absent);
    let board = Board::new(store.clone());
    board.add("First probe", AddTicket::default()).unwrap();
    assert_eq!(
        store.backup_health(),
        BackupHealth::Absent,
        "a first write has no file to roll"
    );
    board.add("Second probe", AddTicket::default()).unwrap();
    match store.backup_health() {
        BackupHealth::Present { age } => assert!(age.as_secs() < 60),
        BackupHealth::Absent => panic!("two writes should leave a backup"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The impure gathering names the JSON leftovers beside a database: each domain's
/// file with its `.pre-sqlite-*` copy, and the `todos.json.*` cohort. The bare
/// `todos.json` is not in the cohort, because the store row already names it.
#[test]
fn gathering_names_the_json_leftovers_beside_a_database() {
    let (store, root) = temp_store("sidecars");
    let home = store.home().clone();
    Board::new(store.clone())
        .add("Sidecar probe", AddTicket::default())
        .expect("the store is written");
    let directory = home.directory();
    std::fs::write(directory.join("sessions.json"), "{}").unwrap();
    std::fs::write(directory.join("sessions.json.pre-sqlite-1793055385"), "{}").unwrap();
    std::fs::write(directory.join("settings.json"), "{}").unwrap();
    std::fs::write(directory.join("todos.json"), "{}").unwrap();
    std::fs::write(directory.join("todos.json.pre-lanes"), "{}").unwrap();

    let facts = diagnostics::gather(&home, &tandem_core::setup::Environment::new());
    assert_eq!(
        facts.sessions_json,
        vec![
            directory.join("sessions.json"),
            directory.join("sessions.json.pre-sqlite-1793055385"),
        ]
    );
    assert_eq!(facts.settings_json, vec![directory.join("settings.json")]);
    assert_eq!(
        facts.todos_json,
        vec![directory.join("todos.json.pre-lanes")],
        "the bare todos.json belongs to the store row's fork note"
    );

    let report = report(&facts);
    assert!(report.ok);
    assert!(
        check(&report, "sessions")
            .detail
            .contains("sessions.json.pre-sqlite-1793055385")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// No database, no import: the leftovers are still named, so `ls` is not the
/// only surface, but the two imported sidecars print no removal command. The
/// `todos.json.*` cohort is ADR 0030's retirement and carries no import marker,
/// so its row names the removal in any home.
#[test]
fn no_database_names_the_leftovers_but_removes_only_the_retired_board() {
    let (store, root) = temp_store("sidecars-fresh");
    let home = store.home().clone();
    std::fs::create_dir_all(home.directory()).unwrap();
    std::fs::write(home.directory().join("sessions.json"), "{}").unwrap();
    std::fs::write(
        home.directory().join("settings.json.pre-sqlite-1793055385"),
        "{}",
    )
    .unwrap();
    std::fs::write(home.directory().join("todos.json.corrupt"), "{}").unwrap();

    let facts = diagnostics::gather(&home, &tandem_core::setup::Environment::new());
    let directory = home.directory();
    assert_eq!(facts.sessions_json, vec![directory.join("sessions.json")]);
    assert_eq!(
        facts.settings_json,
        vec![directory.join("settings.json.pre-sqlite-1793055385")]
    );
    assert_eq!(facts.todos_json, vec![directory.join("todos.json.corrupt")]);

    let report = report(&facts);
    assert!(report.ok);
    let sessions = &check(&report, "sessions").detail;
    assert!(sessions.contains("sessions.json"), "{sessions}");
    assert!(
        !sessions.contains("rm -f"),
        "nothing imported it, so the row prints no removal command: {sessions}"
    );
    let todos = &check(&report, "todos-json").detail;
    assert!(
        todos.contains("rm -f"),
        "the cohort needs no store to be removable: {todos}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The impure gathering answers this machine without writing: a fresh home
/// reads as an absent store and an empty bikeshed, and the home it reports is
/// the one it was given.
#[test]
fn gathering_reads_a_fresh_home_without_creating_it() {
    let (store, root) = temp_store("gather");
    let home = store.home().clone();
    let environment = tandem_core::setup::Environment::new();
    let facts = diagnostics::gather(&home, &environment);
    assert_eq!(facts.store_health, Health::Absent);
    assert_eq!(facts.bikeshed_note_count, 0);
    assert!(!facts.store_lock_held);
    assert_eq!(facts.home_path, home.directory());
    assert!(!root.exists(), "doctor writes nothing, not even the home");
    let _ = std::fs::remove_dir_all(&root);
}

/// `setup` and `doctor` read one state: the row and the verb cannot disagree.
#[test]
fn the_agent_face_row_reads_the_same_state_as_setup() {
    let (store, root) = temp_store("agent-face");
    let home = store.home().clone();
    let environment = tandem_core::setup::Environment::new();
    let facts = diagnostics::gather(&home, &environment);
    let state = setup::state(
        home.directory(),
        &environment,
        None,
        &std::env::current_exe().unwrap_or_default(),
        cfg!(debug_assertions),
    );
    assert_eq!(facts.agent_face, Some(state));
    let _ = std::fs::remove_dir_all(&root);
}
