//! The committed fixture vault, read by the port (`Fixtures/port/Home/bikeshed`).
//!
//! The fixture home is content, not scratch: it is authored and committed, and
//! no run writes into it. Every suite above builds its own temp vault; this one
//! proves the derived readers answer over the fixture both implementations share,
//! from a **copy** of it.

use std::path::Path;

use crate::bikeshed::Bikeshed;
use crate::bikeshed::front::ScaffoldRole;
use crate::bikeshed::wikilink::LinkResolution;

use super::bikeshed_support::Vault;

fn fixture() -> Vault {
    let vault = Vault::new("fixture-home");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Fixtures/port/Home/bikeshed");
    vault.copy_from(&source);
    vault
}

#[test]
fn the_fixture_vault_reads_as_one_project_with_its_overview() {
    let vault = fixture();
    let rows = vault
        .bikeshed()
        .front_page()
        .expect("the front page reads")
        .value;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Fixture");
    assert_eq!(rows[0].folder, "projects/fixture");
    assert_eq!(
        rows[0]
            .pages
            .iter()
            .map(|page| page.role)
            .collect::<Vec<_>>(),
        vec![ScaffoldRole::Overview]
    );
    assert_eq!(rows[0].pages[0].path, "projects/fixture/fixture.md");
}

#[test]
fn the_fixture_vaults_health_is_the_one_link_and_the_one_orphan() {
    let vault = fixture();
    let health = vault
        .bikeshed()
        .health()
        .expect("the health read runs")
        .value;
    assert!(health.broken.is_empty());
    assert_eq!(
        health.counts.notes, 3,
        "Index, the overview and the daily note"
    );
    assert_eq!(health.counts.links, 1, "[[fixture]] in the Index");
    assert_eq!(health.orphans, vec!["daily/2026-10-30.md".to_string()]);
}

#[test]
fn the_fixture_vaults_backlink_resolves_to_the_overview() {
    let vault = fixture();
    let bikeshed = vault.bikeshed();
    assert_eq!(
        bikeshed.resolve("fixture"),
        LinkResolution::Resolved("projects/fixture/fixture.md".to_string())
    );
    let rows = bikeshed
        .backlinks("projects/fixture/fixture.md")
        .expect("the backlinks read")
        .value;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "Index.md");
}

#[test]
fn the_fixture_vaults_search_and_stats_answer() {
    let vault = fixture();
    let bikeshed = vault.bikeshed();
    let hits = bikeshed.search("fixture").expect("the search runs").value;
    assert_eq!(
        hits.iter().map(|hit| hit.path.as_str()).collect::<Vec<_>>(),
        vec![
            "daily/2026-10-30.md",
            "Index.md",
            "projects/fixture/fixture.md"
        ]
    );

    let stats = bikeshed.stats(None).expect("the stats read").value;
    assert_eq!(stats.bikeshed, vault.dir().to_string_lossy());
    assert_eq!(stats.projects.len(), 1);
    let row = &stats.projects[0];
    assert_eq!(row.project, "fixture");
    assert_eq!(row.curated_notes, 1);
    assert_eq!(row.raw_pending, 0);
    assert_eq!(row.raw_archived, 0);
    assert!(row.last_consolidated.is_some());
}

#[test]
fn the_fixture_vaults_daily_note_is_pending_until_the_marker_moves() {
    let vault = fixture();
    let bikeshed = vault.bikeshed();
    assert_eq!(
        bikeshed
            .pending_daily_notes()
            .expect("the daily scan runs")
            .value,
        vec!["daily/2026-10-30.md".to_string()]
    );

    // A copy is the suite's own: advancing the marker writes nothing to the
    // committed fixture.
    let index = vault.read("Index.md");
    vault.stage(
        "Index.md",
        &format!("{index}\nHuman notes folded through 2026-10-30\n"),
    );
    assert!(
        bikeshed
            .pending_daily_notes()
            .expect("the daily scan runs")
            .value
            .is_empty()
    );
}

#[test]
fn the_fixture_copy_is_not_the_committed_tree() {
    let vault = fixture();
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Fixtures/port/Home/bikeshed");
    assert_ne!(vault.dir(), committed.as_path());
    assert!(Bikeshed::new(vault.dir()).read("Index.md").is_ok());
}
