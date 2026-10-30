//! Ported from `BikeshedBacklinkTests`: one row per linking note with the line
//! of its first resolving wikilink; self-links included, broken links, code and
//! embeds contribute nothing; a missing target is `UnknownNote`.

use crate::bikeshed::{Backlink, Bikeshed};
use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

fn rows(bikeshed: &Bikeshed, path: &str) -> Vec<Backlink> {
    bikeshed.backlinks(path).expect("the backlinks read").value
}

#[test]
fn one_row_per_linking_note_with_the_line_of_the_link() {
    let vault = Vault::new("backlink-basic");
    vault.stage("A.md", "see [[B]] below");
    vault.stage("B.md", "bee");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        rows(&bikeshed, "B.md"),
        vec![Backlink {
            path: "A.md".to_string(),
            line: "see [[B]] below".to_string(),
        }]
    );
    assert!(rows(&bikeshed, "A.md").is_empty());
}

#[test]
fn multiple_links_in_one_note_yield_one_row() {
    let vault = Vault::new("backlink-one-row");
    vault.stage("A.md", "first [[B]]\nsecond [[B]] again");
    vault.stage("B.md", "bee");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        rows(&bikeshed, "B.md"),
        vec![Backlink {
            path: "A.md".to_string(),
            line: "first [[B]]".to_string(),
        }]
    );
}

#[test]
fn every_form_that_resolves_produces_a_row() {
    let vault = Vault::new("backlink-forms");
    vault.stage("B.md", "bee");
    vault.stage("a/A.md", "bare [[B]]");
    vault.stage("a/qualified.md", "qualified [[B]] via [[a/A]]");
    vault.stage("a/aliased.md", "aliased [[B|the bee]]");
    vault.stage("a/fraged.md", "fraged [[B#Top]]");
    vault.stage("a/cased.md", "cased [[b]]");
    let bikeshed = vault.bikeshed();

    let found = rows(&bikeshed, "B.md");
    assert_eq!(
        found
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "a/A.md",
            "a/aliased.md",
            "a/cased.md",
            "a/fraged.md",
            "a/qualified.md",
        ]
    );
    assert!(found.contains(&Backlink {
        path: "a/qualified.md".to_string(),
        line: "qualified [[B]] via [[a/A]]".to_string(),
    }));
    assert!(found.contains(&Backlink {
        path: "a/aliased.md".to_string(),
        line: "aliased [[B|the bee]]".to_string(),
    }));
}

#[test]
fn self_links_are_included() {
    let vault = Vault::new("backlink-self");
    vault.stage("B.md", "self [[B]] reference");
    assert_eq!(
        rows(&vault.bikeshed(), "B.md"),
        vec![Backlink {
            path: "B.md".to_string(),
            line: "self [[B]] reference".to_string(),
        }]
    );
}

#[test]
fn broken_links_contribute_nothing() {
    let vault = Vault::new("backlink-broken");
    vault.stage("B.md", "bee");
    vault.stage("onlyBroken.md", "[[Missing]] and [[ghost]]");
    vault.stage("mixed.md", "[[Missing]] then [[B]]");
    assert_eq!(
        rows(&vault.bikeshed(), "B.md"),
        vec![Backlink {
            path: "mixed.md".to_string(),
            line: "[[Missing]] then [[B]]".to_string(),
        }]
    );
}

#[test]
fn code_and_embeds_are_not_links() {
    let vault = Vault::new("backlink-code");
    vault.stage("B.md", "bee");
    vault.stage("coded.md", "```\n[[B]]\n```");
    vault.stage("embedded.md", "![[B]]");
    assert!(rows(&vault.bikeshed(), "B.md").is_empty());
}

#[test]
fn rows_are_sorted_by_the_linking_notes_path() {
    let vault = Vault::new("backlink-sorted");
    vault.stage("B.md", "bee");
    vault.stage("z/z.md", "[[B]]");
    vault.stage("a/a.md", "[[B]]");
    vault.stage("m.md", "[[B]]");
    assert_eq!(
        rows(&vault.bikeshed(), "B.md")
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a/a.md", "m.md", "z/z.md"]
    );
}

#[test]
fn a_missing_target_is_unknown_note() {
    let vault = Vault::new("backlink-missing");
    vault.stage("A.md", "[[B]]");
    assert_eq!(
        vault.bikeshed().backlinks("B.md").unwrap_err(),
        DomainError::UnknownNote("B.md".to_string())
    );
}

#[test]
fn a_folder_target_is_unknown_note() {
    let vault = Vault::new("backlink-folder");
    vault.stage("d/n.md", "x");
    assert_eq!(
        vault.bikeshed().backlinks("d").unwrap_err(),
        DomainError::UnknownNote("d".to_string())
    );
}

#[test]
fn backlinks_are_recomputed_after_note_creation() {
    let vault = Vault::new("backlink-create");
    vault.stage("B.md", "bee");
    let bikeshed = vault.bikeshed();
    assert!(rows(&bikeshed, "B.md").is_empty());

    bikeshed.write("C.md", "new link to [[B]]").unwrap();
    assert_eq!(
        rows(&bikeshed, "B.md"),
        vec![Backlink {
            path: "C.md".to_string(),
            line: "new link to [[B]]".to_string(),
        }]
    );
}

#[test]
fn backlinks_are_recomputed_after_note_deletion() {
    let vault = Vault::new("backlink-delete");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");
    let bikeshed = vault.bikeshed();
    assert_eq!(rows(&bikeshed, "B.md").len(), 1);

    bikeshed.delete("A.md").unwrap();
    assert!(rows(&bikeshed, "B.md").is_empty());
}

#[test]
fn backlinks_are_recomputed_after_rename() {
    let vault = Vault::new("backlink-rename");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");
    let bikeshed = vault.bikeshed();

    bikeshed.rename("B.md", "C.md").unwrap();

    assert_eq!(
        rows(&bikeshed, "C.md"),
        vec![Backlink {
            path: "A.md".to_string(),
            line: "see [[C]]".to_string(),
        }]
    );
    assert_eq!(
        bikeshed.backlinks("B.md").unwrap_err(),
        DomainError::UnknownNote("B.md".to_string())
    );
}
