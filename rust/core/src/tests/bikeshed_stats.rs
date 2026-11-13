//! Ported from `BikeshedStatsTests`: the curated-note count, the `.raw`
//! folder's pending/archived depth and oldest-pending age, the newest curated
//! note's mtime, the totals, and the JSON's null keys.
//!
//! The capture verb left the core (CUT-7), so the `.raw` files a human or an
//! agent wrote by hand are planted directly: the stats read counts files, not
//! writes.

use std::fs;
use std::time::{Duration, SystemTime};

use crate::bikeshed::{BikeshedProjectStats, BikeshedStats, BikeshedStatsTotals};

use super::bikeshed_support::{INSTANT, Vault};

const INSTANT_ISO: &str = "2026-10-18T14:02:11Z";

/// One pending `.raw` file: `projects/<project>/.raw/<session>/<stamp>-<name>`.
fn plant(vault: &Vault, project: &str, session: &str, stamp: &str, name: &str) {
    let folder = vault
        .dir()
        .join("projects")
        .join(project)
        .join(".raw")
        .join(session);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(format!("{stamp}-{name}.md")), "# planted").unwrap();
}

/// Move one pending file into the archive folder, as a consolidation pass does.
fn archive(
    vault: &Vault,
    project: &str,
    session: &str,
    done_session: &str,
    stamp: &str,
    name: &str,
) {
    let done = vault
        .dir()
        .join("projects")
        .join(project)
        .join(".raw")
        .join(".done")
        .join(done_session);
    fs::create_dir_all(&done).unwrap();
    let source = vault
        .dir()
        .join("projects")
        .join(project)
        .join(".raw")
        .join(session)
        .join(format!("{stamp}-{name}.md"));
    fs::rename(&source, done.join(source.file_name().unwrap())).unwrap();
}

fn stats(vault: &Vault, project: Option<&str>) -> BikeshedStats {
    vault
        .bikeshed()
        .stats(project)
        .expect("the stats read")
        .value
}

#[test]
fn an_empty_bikeshed_reports_no_projects_and_zero_totals() {
    let vault = Vault::new("stats-empty");
    let found = stats(&vault, None);
    assert_eq!(found.bikeshed, crate::wire::slashed_path(vault.dir()));
    assert!(found.projects.is_empty());
    assert_eq!(found.totals.curated_notes, 0);
    assert_eq!(found.totals.raw_pending, 0);
    assert_eq!(found.totals.raw_archived, 0);
}

#[test]
fn a_project_reports_its_queue_its_archive_and_its_oldest_pending() {
    let vault = Vault::new("stats-pending");
    plant(&vault, "tandem", "ses", "20261018T140211", "gotcha-older");
    plant(&vault, "tandem", "ses", "20261018T140311", "fact-newer");
    plant(&vault, "tandem", "ses", "20261018T140411", "fact-consumed");
    archive(
        &vault,
        "tandem",
        "ses",
        "ses_done",
        "20261018T140411",
        "fact-consumed",
    );

    let bikeshed = vault.bikeshed();
    bikeshed.write("projects/tandem/Note.md", "# Note").unwrap();
    let consolidated = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_400_000);
    vault.set_modified("projects/tandem/Note.md", consolidated);

    let row = &stats(&vault, None).projects[0];
    assert_eq!(row.project, "tandem");
    assert_eq!(row.curated_notes, 1);
    assert_eq!(row.raw_pending, 2);
    assert_eq!(row.raw_archived, 1);
    assert_eq!(row.oldest_pending.as_deref(), Some(INSTANT_ISO));
    assert_eq!(
        row.last_consolidated.as_deref(),
        Some("2026-10-19T08:53:20Z")
    );
}

#[test]
fn a_project_with_pending_files_but_no_curated_notes() {
    let vault = Vault::new("stats-no-curated");
    plant(&vault, "fresh", "ses", "20261018T140211", "decision-first");
    let row = &stats(&vault, None).projects[0];
    assert_eq!(row.project, "fresh");
    assert_eq!(row.curated_notes, 0);
    assert_eq!(row.raw_pending, 1);
    assert_eq!(row.raw_archived, 0);
    assert_eq!(row.last_consolidated, None);
    assert_eq!(row.oldest_pending.as_deref(), Some(INSTANT_ISO));
}

#[test]
fn curated_notes_count_recursively_and_totals_sum_the_rows() {
    let vault = Vault::new("stats-counting");
    let bikeshed = vault.bikeshed();
    bikeshed.write("projects/alpha/Top.md", "# Top").unwrap();
    bikeshed
        .write("projects/alpha/deep/Nested.md", "# Nested")
        .unwrap();
    bikeshed.write("projects/beta/Only.md", "# Only").unwrap();
    plant(&vault, "alpha", "ses", "20261018T140211", "fact-hidden");

    let found = stats(&vault, None);
    assert_eq!(
        found
            .projects
            .iter()
            .map(|row| row.project.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(
        found
            .projects
            .iter()
            .find(|row| row.project == "alpha")
            .unwrap()
            .curated_notes,
        2
    );
    assert_eq!(found.totals.curated_notes, 3);
    assert_eq!(found.totals.raw_pending, 1);
    assert_eq!(found.totals.raw_archived, 0);
}

#[test]
fn the_oldest_pending_falls_back_to_the_files_own_mtime() {
    let vault = Vault::new("stats-mtime");
    let folder = vault.dir().join("projects/tandem/.raw/ses");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("untitled.md"), "# unstamped").unwrap();
    let row = &stats(&vault, None).projects[0];
    assert_eq!(row.raw_pending, 1);
    assert!(
        row.oldest_pending.is_some(),
        "a file with no leading stamp still names an age"
    );
}

#[test]
fn project_narrows_the_report_and_an_unknown_name_is_empty() {
    let vault = Vault::new("stats-narrow");
    let bikeshed = vault.bikeshed();
    bikeshed.write("projects/alpha/A.md", "# A").unwrap();
    bikeshed.write("projects/beta/B.md", "# B").unwrap();

    let narrowed = stats(&vault, Some("beta"));
    assert_eq!(
        narrowed
            .projects
            .iter()
            .map(|row| row.project.as_str())
            .collect::<Vec<_>>(),
        vec!["beta"]
    );
    assert_eq!(narrowed.totals.curated_notes, 1);

    let none = stats(&vault, Some("missing"));
    assert!(none.projects.is_empty());
    assert_eq!(none.totals.curated_notes, 0);
}

#[test]
fn the_json_keeps_both_queue_age_keys_present_as_null() {
    let stats = BikeshedStats {
        bikeshed: "/tmp/bikeshed".to_string(),
        projects: vec![BikeshedProjectStats {
            project: "tandem".to_string(),
            curated_notes: 0,
            raw_pending: 0,
            raw_archived: 0,
            oldest_pending: None,
            last_consolidated: None,
        }],
        totals: BikeshedStatsTotals {
            curated_notes: 0,
            raw_pending: 0,
            raw_archived: 0,
        },
    };
    let json = serde_json::to_string(&stats).expect("the stats serialize");
    assert!(json.contains("\"oldestPending\":null"));
    assert!(json.contains("\"lastConsolidated\":null"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["bikeshed"], "/tmp/bikeshed");
    assert_eq!(value["projects"].as_array().unwrap().len(), 1);
}

#[test]
fn the_planted_stamp_matches_the_suites_instant() {
    // The planted filenames above are only the suite's own words if the stamp
    // spells the instant the fixture names.
    assert_eq!(crate::store::encode(INSTANT), INSTANT_ISO);
}
