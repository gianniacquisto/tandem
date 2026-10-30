//! Ported from `BikeshedHealthTests`: the wikilinks that resolve to nothing, the
//! notes nothing links to, the totals, the hidden saddlebag's exclusion, and the
//! JSON shape `note links` prints.

use crate::bikeshed::health::BikeshedHealthCounts;
use crate::bikeshed::{BikeshedHealth, BrokenLink};

use super::bikeshed_support::Vault;

fn health(vault: &Vault) -> BikeshedHealth {
    vault
        .bikeshed()
        .health()
        .expect("the health read runs")
        .value
}

#[test]
fn every_link_form_resolves_and_none_is_broken() {
    let vault = Vault::new("health-forms");
    vault.stage("Note.md", "# Note");
    vault.stage("projects/tandem/Deep.md", "# Deep");
    vault.stage(
        "Linker.md",
        "A bare link: [[Note]]\nA qualified link: [[projects/tandem/Deep]]\n\
         An aliased link: [[Note|shown as this]]\nA fragment link: [[Note#Heading]]\n\
         An aliased fragment: [[Note#Heading|shown]]\n",
    );
    let found = health(&vault);
    assert!(found.broken.is_empty());
    assert_eq!(found.counts.broken, 0);
    assert_eq!(found.counts.links, 5);
}

#[test]
fn a_link_inside_a_code_span_is_not_a_link() {
    let vault = Vault::new("health-code");
    vault.stage("Note.md", "A literal `[[Missing]]` in a code span.");
    let found = health(&vault);
    assert!(found.broken.is_empty());
    assert_eq!(found.counts.links, 0);
}

#[test]
fn a_broken_link_is_reported_with_note_target_and_line() {
    let vault = Vault::new("health-broken");
    vault.stage(
        "Note.md",
        "# Note\n\nFirst line.\nA link to nowhere: [[Missing]].\n",
    );
    let found = health(&vault);
    assert_eq!(found.broken.len(), 1);
    assert_eq!(
        found.broken[0],
        BrokenLink {
            note: "Note.md".to_string(),
            link: "Missing".to_string(),
            line: 4,
        }
    );
    assert_eq!(found.counts.broken, 1);
}

#[test]
fn the_target_drops_the_fragment_and_the_alias() {
    let vault = Vault::new("health-target");
    vault.stage("Note.md", "[[Missing#Heading|shown as this]]");
    let broken = &health(&vault).broken[0];
    assert_eq!(broken.link, "Missing");
    assert_eq!(broken.line, 1);
}

#[test]
fn an_ambiguous_basename_is_broken() {
    let vault = Vault::new("health-ambiguous");
    vault.stage("one/Note.md", "# One");
    vault.stage("two/Note.md", "# Two");
    vault.stage("Linker.md", "[[Note]]");
    let found = health(&vault);
    assert_eq!(found.broken.len(), 1);
    assert_eq!(found.broken[0].link, "Note");
}

#[test]
fn a_note_nothing_links_to_is_an_orphan() {
    let vault = Vault::new("health-orphan");
    vault.stage("Note.md", "# Note\n\nSee [[Reached]].");
    vault.stage("Reached.md", "See [[Note]].");
    vault.stage("Alone.md", "# Alone");
    let found = health(&vault);
    assert_eq!(found.orphans, vec!["Alone.md".to_string()]);
    assert_eq!(found.counts.orphans, 1);
}

#[test]
fn the_hub_and_a_projects_anchor_note_are_never_orphans() {
    let vault = Vault::new("health-anchors");
    vault.stage("Index.md", "# Index\n\n- [[tandem]]\n");
    vault.stage("projects/tandem/Tandem.md", "# Tandem");
    vault.stage("projects/tandem/Other.md", "# Other");
    assert_eq!(
        health(&vault).orphans,
        vec!["projects/tandem/Other.md".to_string()]
    );
}

#[test]
fn outgoing_links_do_not_rescue_an_orphan() {
    let vault = Vault::new("health-outgoing");
    vault.stage("Note.md", "# Note");
    vault.stage("Lonely.md", "See [[Note]].");
    assert_eq!(health(&vault).orphans, vec!["Lonely.md".to_string()]);
}

#[test]
fn the_hidden_saddlebag_is_out_of_the_graph() {
    let vault = Vault::new("health-hidden");
    vault.stage("Note.md", "# Note\n\nSee [[Other]].");
    vault.stage("Other.md", "See [[Note]].");
    vault.stage(
        "projects/tandem/.raw/ses/20261020T120000Z-gotcha-thing.md",
        "A capture mentioning [[Nowhere]].",
    );
    let found = health(&vault);
    assert!(found.broken.is_empty());
    assert!(found.orphans.is_empty());
    assert_eq!(found.counts.notes, 2);
}

#[test]
fn an_empty_bikeshed_reports_zero_counts() {
    let vault = Vault::new("health-empty");
    let found = health(&vault);
    assert!(found.broken.is_empty());
    assert!(found.orphans.is_empty());
    assert_eq!(
        found.counts,
        BikeshedHealthCounts {
            notes: 0,
            links: 0,
            broken: 0,
            orphans: 0
        }
    );
    assert_eq!(BikeshedHealth::empty(), found);
}

#[test]
fn the_json_keeps_the_fixed_shape() {
    let vault = Vault::new("health-json");
    vault.stage("Note.md", "[[Missing]]\n\nSee [[Other]].");
    vault.stage("Other.md", "See [[Note]].");
    vault.stage("Alone.md", "# Alone");

    let value = serde_json::to_value(health(&vault)).expect("the report serializes");
    let broken = value["broken"].as_array().expect("broken rows");
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0]["note"], "Note.md");
    assert_eq!(broken[0]["link"], "Missing");
    assert_eq!(broken[0]["line"], 1);
    assert_eq!(value["orphans"], serde_json::json!(["Alone.md"]));
    assert_eq!(value["counts"]["notes"], 3);
    assert_eq!(value["counts"]["links"], 3);
    assert_eq!(value["counts"]["broken"], 1);
    assert_eq!(value["counts"]["orphans"], 1);
}
