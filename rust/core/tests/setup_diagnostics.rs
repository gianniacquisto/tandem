//! The `doctor` report (ADR 0014), cut to the three checks the board and the
//! notes store need (ADR 0045): the home, the store and the bikeshed.
//!
//! The impure gathering is deliberately not tested here: it only reads the
//! machine. Everything with a decision in it is `report`, which these tests
//! drive with injected facts.

use std::path::PathBuf;
use std::time::Duration;

use tandem_core::diagnostics::{self, CHECK_NAMES, Facts, age_text, report};
use tandem_core::domain::{AddTicket, Board};
use tandem_core::store::{BackupHealth, Health, Home, HomeSource, Store};

fn facts() -> Facts {
    Facts {
        home_path: PathBuf::from("/Users/tester/.tandem-dev"),
        home_source: HomeSource::Environment,
        store_path: PathBuf::from("/Users/tester/.tandem-dev/tandem.db"),
        store_version: Some(1),
        store_health: Health::Counts { open: 3, done: 2 },
        store_backup: BackupHealth::Present {
            age: Duration::from_secs(300),
        },
        bikeshed_path: PathBuf::from("/Users/tester/.tandem-dev/bikeshed"),
        bikeshed_note_count: 4,
        bikeshed_is_file: false,
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

/// A throwaway store rooted at a fresh temporary directory.
fn temp_store(name: &str) -> (Store, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("tandem-doctor-tests-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = Home::resolve(Some(&root), None, &root.join("user"));
    (Store::new(home), root)
}

#[test]
fn a_healthy_machine_passes_with_the_three_checks_in_the_pinned_order() {
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
    assert_eq!(CHECK_NAMES, ["home", "store", "bikeshed"]);
    assert!(report.checks.iter().all(|check| check.ok));
    assert_eq!(
        check(&report, "home").detail,
        "/Users/tester/.tandem-dev (TANDEM_HOME override)"
    );
    assert_eq!(
        check(&report, "store").detail,
        "/Users/tester/.tandem-dev/tandem.db \u{2014} v1; 3 open, 2 done; backup 5m old"
    );
    assert_eq!(
        check(&report, "bikeshed").detail,
        "/Users/tester/.tandem-dev/bikeshed \u{2014} 4 note(s)"
    );
}

#[test]
fn an_absent_store_is_a_valid_empty_setup() {
    let mut facts = facts();
    facts.store_health = Health::Absent;
    facts.store_version = None;
    let report = report(&facts);
    assert!(report.ok);
    assert_eq!(
        check(&report, "store").detail,
        "/Users/tester/.tandem-dev/tandem.db \u{2014} absent (a valid empty store); backup 5m old"
    );
}

#[test]
fn a_schema_this_build_does_not_read_is_red() {
    let mut facts = facts();
    facts.store_health = Health::Legacy { version: 5 };
    let report = report(&facts);
    assert!(!report.ok);
    let store = check(&report, "store");
    assert!(!store.ok);
    assert!(store.detail.contains("schema v5"), "{}", store.detail);
}

#[test]
fn an_unreadable_store_is_red_and_names_the_recovery() {
    let mut facts = facts();
    facts.store_health = Health::Unreadable;
    facts.store_backup = BackupHealth::Absent;
    let report = report(&facts);
    assert!(!report.ok);
    let store = check(&report, "store");
    assert!(!store.ok);
    assert!(
        store.detail.contains("restore it from tandem.db.bak"),
        "{}",
        store.detail
    );
    assert!(store.detail.ends_with("no backup yet"), "{}", store.detail);
}

#[test]
fn a_bikeshed_path_that_is_a_file_is_red() {
    let mut facts = facts();
    facts.bikeshed_is_file = true;
    let report = report(&facts);
    assert!(!report.ok);
    let row = check(&report, "bikeshed");
    assert!(!row.ok);
    assert_eq!(
        row.detail,
        "/Users/tester/.tandem-dev/bikeshed \u{2014} a file, not a folder; pick a folder"
    );
}

#[test]
fn the_home_row_names_the_flag_override() {
    let mut facts = facts();
    facts.home_source = HomeSource::Flag;
    assert_eq!(
        check(&report(&facts), "home").detail,
        "/Users/tester/.tandem-dev (--home)"
    );
    facts.home_source = HomeSource::Default;
    assert_eq!(
        check(&report(&facts), "home").detail,
        "/Users/tester/.tandem-dev (default)"
    );
}

#[test]
fn the_backup_age_reads_in_the_shortest_unit() {
    assert_eq!(age_text(1.2), "1s");
    assert_eq!(age_text(90.0), "1m");
    assert_eq!(age_text(3_700.0), "1h");
    assert_eq!(age_text(172_800.0), "2d");
}

#[test]
fn gathering_reads_a_fresh_home_without_creating_it() {
    let (store, root) = temp_store("gather");
    let home = store.home().clone();
    let facts = diagnostics::gather(&home);
    assert_eq!(facts.store_health, Health::Absent);
    assert_eq!(facts.bikeshed_note_count, 0);
    assert_eq!(facts.home_path, home.directory());
    assert!(!root.exists(), "doctor writes nothing, not even the home");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn gathering_counts_the_stores_rows_and_the_bikeshed_notes() {
    let (store, root) = temp_store("counts");
    let home = store.home().clone();
    Board::new(store)
        .add("Doctor probe", AddTicket::default())
        .expect("the ticket is written");
    std::fs::create_dir_all(home.directory().join("bikeshed")).unwrap();
    std::fs::write(home.directory().join("bikeshed/note.md"), "a note\n").unwrap();

    let report = report(&diagnostics::gather(&home));

    assert!(report.ok);
    assert!(check(&report, "store").detail.contains("1 open, 0 done"));
    assert!(check(&report, "bikeshed").detail.contains("1 note(s)"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn gathering_names_a_bikeshed_path_that_is_a_file() {
    let (store, root) = temp_store("bikeshed-file");
    let home = store.home().clone();
    std::fs::create_dir_all(home.directory()).unwrap();
    std::fs::write(home.directory().join("bikeshed"), "not a folder\n").unwrap();

    let report = report(&diagnostics::gather(&home));

    assert!(!report.ok);
    assert!(!check(&report, "bikeshed").ok);
    let _ = std::fs::remove_dir_all(&root);
}
