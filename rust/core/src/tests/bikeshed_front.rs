//! Ported from `BikeshedFrontTests`: one row per project folder, the Overview
//! matched by normalized name, the scaffold page set in fixed order, the
//! "where this left off" state line, the freshness stamp, group sub-folders
//! excluded, a folder without an Overview skipped, and the hidden saddlebag
//! never read.

use std::time::{Duration, SystemTime};

use crate::bikeshed::front::ScaffoldRole;

use super::bikeshed_support::Vault;

fn rows(vault: &Vault) -> Vec<crate::bikeshed::ProjectFront> {
    vault
        .bikeshed()
        .front_page()
        .expect("the front page reads")
        .value
}

#[test]
fn one_row_per_project_sorted_by_name_and_topics_is_not_a_project() {
    let vault = Vault::new("front-rows");
    vault.stage(
        "projects/tandem/tandem.md",
        "# Tandem\n\n## Where left off\n\n2026-10-25: state.\n",
    );
    vault.stage("projects/code-rank/Code Rank.md", "# Code Rank\n");
    vault.stage("topics/Something.md", "# Something\n");

    let found = rows(&vault);
    assert_eq!(
        found
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Code Rank", "Tandem"]
    );
    assert_eq!(
        found
            .iter()
            .map(|row| row.folder.as_str())
            .collect::<Vec<_>>(),
        vec!["projects/code-rank", "projects/tandem"]
    );
}

#[test]
fn the_overview_is_matched_by_normalized_name() {
    let vault = Vault::new("front-overview");
    vault.stage("projects/code-rank/Code Rank.md", "# Code Rank\n");
    vault.stage("projects/code-rank/Decoy.md", "# Decoy\n");
    let found = rows(&vault);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0]
            .pages
            .iter()
            .map(|page| page.path.as_str())
            .collect::<Vec<_>>(),
        vec!["projects/code-rank/Code Rank.md"]
    );
}

#[test]
fn the_project_name_is_the_overview_title() {
    let vault = Vault::new("front-name");
    vault.stage("projects/tandem/tandem.md", "##   Tandem  \nbody\n");
    vault.stage("projects/quiet/quiet.md", "\nbody\n");
    assert_eq!(
        rows(&vault)
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["quiet", "Tandem"]
    );
}

#[test]
fn the_scaffold_page_set_is_in_fixed_order() {
    let vault = Vault::new("front-pages");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    vault.stage(
        "projects/tandem/tandem Decisions.md",
        "# Tandem Decisions\n",
    );
    vault.stage(
        "projects/tandem/tandem Processes.md",
        "# Tandem Processes\n",
    );
    vault.stage("projects/tandem/tandem Risks.md", "# Tandem Risks\n");
    let row = &rows(&vault)[0];
    assert_eq!(
        row.pages.iter().map(|page| page.role).collect::<Vec<_>>(),
        vec![
            ScaffoldRole::Overview,
            ScaffoldRole::Decisions,
            ScaffoldRole::Risks,
            ScaffoldRole::Processes,
        ]
    );
    assert_eq!(
        row.pages
            .iter()
            .map(|page| page.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "projects/tandem/tandem.md",
            "projects/tandem/tandem Decisions.md",
            "projects/tandem/tandem Risks.md",
            "projects/tandem/tandem Processes.md",
        ]
    );
}

#[test]
fn missing_scaffold_pages_are_omitted() {
    let vault = Vault::new("front-missing-pages");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    vault.stage(
        "projects/tandem/tandem Decisions.md",
        "# Tandem Decisions\n",
    );
    assert_eq!(
        rows(&vault)[0]
            .pages
            .iter()
            .map(|page| page.role)
            .collect::<Vec<_>>(),
        vec![ScaffoldRole::Overview, ScaffoldRole::Decisions]
    );
}

#[test]
fn group_subfolders_are_neither_rows_nor_pages() {
    let vault = Vault::new("front-subfolders");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    vault.stage("projects/tandem/App/App.md", "# App\n");
    vault.stage("projects/tandem/App/tandem Decisions.md", "# not a page\n");
    let found = rows(&vault);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0]
            .pages
            .iter()
            .map(|page| page.role)
            .collect::<Vec<_>>(),
        vec![ScaffoldRole::Overview]
    );
}

#[test]
fn a_project_folder_without_an_overview_is_skipped() {
    let vault = Vault::new("front-skip");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    vault.stage("projects/misc/Loose.md", "# Loose\n");
    assert_eq!(
        rows(&vault)
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Tandem"]
    );
}

#[test]
fn the_state_line_is_the_first_non_empty_line_after_the_heading() {
    let vault = Vault::new("front-state");
    vault.stage(
        "projects/tandem/tandem.md",
        "# Tandem\n\n## Where left off\n\n2026-10-25: the scaffold is live.\n\n## Threads\n\n- more\n",
    );
    assert_eq!(
        rows(&vault)[0].state_line.as_deref(),
        Some("2026-10-25: the scaffold is live.")
    );
}

#[test]
fn no_where_left_off_heading_means_no_state_line() {
    let vault = Vault::new("front-no-state");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n\nplain body text\n");
    assert_eq!(rows(&vault)[0].state_line, None);
}

#[test]
fn the_freshness_stamp_is_the_overviews_mtime() {
    let vault = Vault::new("front-freshness");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
    vault.set_modified("projects/tandem/tandem.md", stamp);
    assert_eq!(rows(&vault)[0].freshness, Some(stamp));
}

#[test]
fn no_projects_folder_is_an_empty_front_page() {
    let vault = Vault::new("front-no-projects");
    vault.stage("notes/Loose.md", "# Loose\n");
    assert!(rows(&vault).is_empty());
}

#[test]
fn the_hidden_saddlebag_is_never_read() {
    let vault = Vault::new("front-hidden");
    vault.stage("projects/tandem/tandem.md", "# Tandem\n");
    vault.stage(
        "projects/tandem/.raw/ses/2026-10-25-decision-x.md",
        "## Where left off\n\nraw state\n",
    );
    let found = rows(&vault);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0]
            .pages
            .iter()
            .map(|page| page.role)
            .collect::<Vec<_>>(),
        vec![ScaffoldRole::Overview]
    );
    assert_eq!(found[0].state_line, None);
}

#[test]
fn the_role_labels_are_the_chip_text() {
    assert_eq!(ScaffoldRole::Overview.label(), "Overview");
    assert_eq!(ScaffoldRole::Decisions.label(), "Decisions");
    assert_eq!(ScaffoldRole::Risks.label(), "Risks");
    assert_eq!(ScaffoldRole::Processes.label(), "Processes");
}
