//! Ported from `BikeshedStatsTests`: the curated-note count, the newest curated
//! note's mtime, the totals, and the JSON's null keys.
//!
//! The capture layer left the core (CUT-7), so the `.raw` subtree has no writer
//! and `note stats` no longer reads it. A hidden file planted under `.raw/` is
//! still an ordinary hidden file: it is pruned from the curated count and never
//! named on the wire.

use std::fs;
use std::time::{Duration, SystemTime};

use crate::bikeshed::{BikeshedProjectStats, BikeshedStats, BikeshedStatsTotals};

use super::bikeshed_support::Vault;

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
}

#[test]
fn a_project_reports_its_curated_notes_and_newest_note_mtime() {
    let vault = Vault::new("stats-pending");
    let bikeshed = vault.bikeshed();
    bikeshed.write("projects/tandem/Note.md", "# Note").unwrap();
    let consolidated = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_400_000);
    vault.set_modified("projects/tandem/Note.md", consolidated);

    let row = &stats(&vault, None).projects[0];
    assert_eq!(row.project, "tandem");
    assert_eq!(row.curated_notes, 1);
    assert_eq!(
        row.last_consolidated.as_deref(),
        Some("2026-10-19T08:53:20Z")
    );
}

#[test]
fn a_project_with_no_curated_notes_reports_none() {
    let vault = Vault::new("stats-no-curated");
    let folder = vault.dir().join("projects/fresh/.raw/ses");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("capture.md"), "# planted").unwrap();

    let row = &stats(&vault, None).projects[0];
    assert_eq!(row.project, "fresh");
    assert_eq!(row.curated_notes, 0);
    assert_eq!(row.last_consolidated, None);
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
fn the_json_keeps_the_mtime_key_present_as_null_and_names_no_capture_layer() {
    let stats = BikeshedStats {
        bikeshed: "/tmp/bikeshed".to_string(),
        projects: vec![BikeshedProjectStats {
            project: "tandem".to_string(),
            curated_notes: 0,
            last_consolidated: None,
        }],
        totals: BikeshedStatsTotals { curated_notes: 0 },
    };
    let json = serde_json::to_string(&stats).expect("the stats serialize");
    assert!(json.contains("\"lastConsolidated\":null"));
    assert!(!json.contains("rawPending"));
    assert!(!json.contains("rawArchived"));
    assert!(!json.contains("oldestPending"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["bikeshed"], "/tmp/bikeshed");
    assert_eq!(value["projects"].as_array().unwrap().len(), 1);
}
