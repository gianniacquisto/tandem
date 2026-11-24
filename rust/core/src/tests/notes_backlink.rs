//! Ported from `NotesBacklinkTests`: one row per linking note with the line
//! of its first resolving wikilink; self-links included, broken links, code and
//! embeds contribute nothing; a missing target is `UnknownNote`.

use crate::domain::error::DomainError;
use crate::notes::{Backlink, Notes};

use super::notes_support::Vault;

fn rows(notes: &Notes, path: &str) -> Vec<Backlink> {
    notes.backlinks(path).expect("the backlinks read").value
}

#[test]
fn one_row_per_linking_note_with_the_line_of_the_link() {
    let vault = Vault::new("backlink-basic");
    vault.stage("A.md", "see [[B]] below");
    vault.stage("B.md", "bee");
    let notes = vault.notes();

    assert_eq!(
        rows(&notes, "B.md"),
        vec![Backlink {
            path: "A.md".to_string(),
            line: "see [[B]] below".to_string(),
        }]
    );
    assert!(rows(&notes, "A.md").is_empty());
}

#[test]
fn multiple_links_in_one_note_yield_one_row() {
    let vault = Vault::new("backlink-one-row");
    vault.stage("A.md", "first [[B]]\nsecond [[B]] again");
    vault.stage("B.md", "bee");
    let notes = vault.notes();

    assert_eq!(
        rows(&notes, "B.md"),
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
    let notes = vault.notes();

    let found = rows(&notes, "B.md");
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
        rows(&vault.notes(), "B.md"),
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
        rows(&vault.notes(), "B.md"),
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
    assert!(rows(&vault.notes(), "B.md").is_empty());
}

#[test]
fn rows_are_sorted_by_the_linking_notes_path() {
    let vault = Vault::new("backlink-sorted");
    vault.stage("B.md", "bee");
    vault.stage("z/z.md", "[[B]]");
    vault.stage("a/a.md", "[[B]]");
    vault.stage("m.md", "[[B]]");
    assert_eq!(
        rows(&vault.notes(), "B.md")
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
        vault.notes().backlinks("B.md").unwrap_err(),
        DomainError::UnknownNote("B.md".to_string())
    );
}

#[test]
fn a_folder_target_is_unknown_note() {
    let vault = Vault::new("backlink-folder");
    vault.stage("d/n.md", "x");
    assert_eq!(
        vault.notes().backlinks("d").unwrap_err(),
        DomainError::UnknownNote("d".to_string())
    );
}

#[test]
fn backlinks_are_recomputed_after_note_creation() {
    let vault = Vault::new("backlink-create");
    vault.stage("B.md", "bee");
    let notes = vault.notes();
    assert!(rows(&notes, "B.md").is_empty());

    notes.write("C.md", "new link to [[B]]", None).unwrap();
    assert_eq!(
        rows(&notes, "B.md"),
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
    let notes = vault.notes();
    assert_eq!(rows(&notes, "B.md").len(), 1);

    notes.delete("A.md").unwrap();
    assert!(rows(&notes, "B.md").is_empty());
}

#[test]
fn backlinks_are_recomputed_after_rename() {
    let vault = Vault::new("backlink-rename");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");
    let notes = vault.notes();

    notes.rename("B.md", "C.md").unwrap();

    assert_eq!(
        rows(&notes, "C.md"),
        vec![Backlink {
            path: "A.md".to_string(),
            line: "see [[C]]".to_string(),
        }]
    );
    assert_eq!(
        notes.backlinks("B.md").unwrap_err(),
        DomainError::UnknownNote("B.md".to_string())
    );
}

fn board() -> crate::notes::BoardNames {
    use crate::notes::{BoardNames, BoardProject};
    BoardNames {
        projects: vec![BoardProject {
            name: "Tandem".to_string(),
            labels: vec!["Tandem".to_string(), "cli".to_string()],
        }],
    }
}

fn typed(notes: &Notes, kind: crate::notes::ConceptKind, name: &str) -> Vec<Backlink> {
    notes
        .typed_backlinks(kind, name, &board())
        .expect("the typed backlinks read")
        .value
}

#[test]
fn typed_backlinks_list_the_notes_and_lines_for_a_label_and_a_project_ignoring_case() {
    use crate::notes::ConceptKind;

    let vault = Vault::new("backlink-typed");
    vault.stage(
        "daily/2026-11-20.md",
        "About: [[project:Tandem]] [[label:cli]]",
    );
    vault.stage(
        "A.md",
        "first\nthe door: [[Label:CLI|the cli]]\nagain [[label:cli]]",
    );
    vault.stage("B.md", "only a project [[project:tandem]]");
    vault.stage("C.md", "broken [[label:nope]] and a plain [[A]]");
    vault.stage("D.md", "```\n[[label:cli]]\n```");
    let notes = vault.notes();

    assert_eq!(
        typed(&notes, ConceptKind::Label, "cli"),
        vec![
            Backlink {
                path: "A.md".to_string(),
                line: "the door: [[Label:CLI|the cli]]".to_string(),
            },
            Backlink {
                path: "daily/2026-11-20.md".to_string(),
                line: "About: [[project:Tandem]] [[label:cli]]".to_string(),
            },
        ]
    );
    assert_eq!(
        typed(&notes, ConceptKind::Label, "CLI").len(),
        2,
        "the asked name matches ignoring case"
    );
    assert_eq!(
        typed(&notes, ConceptKind::Project, "Tandem")
            .into_iter()
            .map(|row| row.path)
            .collect::<Vec<_>>(),
        vec!["B.md".to_string(), "daily/2026-11-20.md".to_string()]
    );
    assert!(typed(&notes, ConceptKind::Label, "nope").is_empty());
}
