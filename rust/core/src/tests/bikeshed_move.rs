//! Ported from `BikeshedMoveTests`: the same machinery as `rename`, in one
//! locked transaction. A folder moved into its own subtree and a name already
//! taken in the destination are refused with nothing written; moving an item
//! into the folder it is already in is a no-op.

use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

#[test]
fn move_takes_a_note_into_a_folder_and_rewrites_the_links_to_it() {
    let vault = Vault::new("move-note");
    vault.stage("a/x.md", "the note");
    vault.stage("b/keep.md", "k");
    vault.stage("links.md", "see [[x]]");
    let bikeshed = vault.bikeshed();

    let result = bikeshed.move_into("a/x.md", "b").unwrap().value;
    assert_eq!(result.path, "b/x.md");
    assert_eq!(result.old_path, "a/x.md");
    assert!(!result.is_folder);
    assert_eq!(
        result
            .moved_notes
            .iter()
            .map(|note| note.new_path.as_str())
            .collect::<Vec<_>>(),
        vec!["b/x.md"]
    );
    assert!(!vault.exists("a/x.md"));
    assert_eq!(vault.read("b/x.md"), "the note");
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn move_rewrites_every_wikilink_form() {
    let vault = Vault::new("move-forms");
    vault.stage("a/x.md", "the note");
    vault.stage("b/keep.md", "k");
    let original = "1 [[x]]\n2 [[x|An Alias]]\n3 [[x#Heading]]\n4 [[x#Heading|Alias]]\n5 [[a/x]]\n";
    vault.stage("links.md", original);
    let bikeshed = vault.bikeshed();

    bikeshed.move_into("a/x.md", "b").unwrap();

    let expected = "1 [[x]]\n2 [[x|An Alias]]\n3 [[x#Heading]]\n4 [[x#Heading|Alias]]\n5 [[x]]\n";
    assert_eq!(vault.read("links.md"), expected);
}

#[test]
fn move_to_the_bikeshed_root() {
    let vault = Vault::new("move-root");
    vault.stage("a/x.md", "the note");
    vault.stage("links.md", "see [[x]]");
    let result = vault.bikeshed().move_into("a/x.md", "").unwrap().value;
    assert_eq!(result.path, "x.md");
    assert!(vault.exists("x.md"));
    assert!(!vault.exists("a/x.md"));
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn move_rewrites_the_moved_notes_own_self_links() {
    let vault = Vault::new("move-self");
    vault.stage("a/x.md", "note to [[x]] and [[x|self]]");
    vault.stage("b/keep.md", "k");
    vault.bikeshed().move_into("a/x.md", "b").unwrap();
    assert_eq!(vault.read("b/x.md"), "note to [[x]] and [[x|self]]");
}

#[test]
fn move_into_a_folder_that_leaves_the_basename_ambiguous_rewrites_qualified() {
    let vault = Vault::new("move-ambiguous");
    vault.stage("a/x.md", "the other note");
    vault.stage("z/x.md", "the note");
    vault.stage("b/keep.md", "k");
    vault.stage("links.md", "see [[z/x]]");
    vault.bikeshed().move_into("z/x.md", "b").unwrap();
    assert_eq!(vault.read("links.md"), "see [[b/x]]");
    assert_eq!(vault.read("b/x.md"), "the note");
}

#[test]
fn move_leaves_markdown_links_embeds_and_code_byte_identical() {
    let vault = Vault::new("move-wikilinks-only");
    vault.stage("a/x.md", "the note");
    vault.stage("b/keep.md", "k");
    let original = "[text](a/x.md) and [[x]]\n![[x]]\n```\n[[x]]\n```\n";
    vault.stage("links.md", original);
    vault.bikeshed().move_into("a/x.md", "b").unwrap();
    assert_eq!(vault.read("links.md"), original);
}

#[test]
fn moving_a_note_into_the_folder_it_is_already_in_changes_nothing() {
    let vault = Vault::new("move-noop");
    vault.stage("a/x.md", "the note");
    vault.stage("links.md", "see [[x]]");
    let result = vault.bikeshed().move_into("a/x.md", "a").unwrap().value;
    assert_eq!(result.path, "a/x.md");
    assert!(result.moved_notes.is_empty());
    assert_eq!(vault.read("a/x.md"), "the note");
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn move_takes_a_folder_and_its_notes_into_another_folder() {
    let vault = Vault::new("move-folder");
    vault.stage("a/x.md", "the note");
    vault.stage("a/deep/y.md", "the deep note");
    vault.stage("b/keep.md", "k");
    vault.stage("links.md", "see [[x]] and [[y]]");
    let bikeshed = vault.bikeshed();

    let result = bikeshed.move_into("a", "b").unwrap().value;
    assert_eq!(result.path, "b/a");
    assert!(result.is_folder);
    let mut moved: Vec<String> = result
        .moved_notes
        .iter()
        .map(|note| note.new_path.clone())
        .collect();
    moved.sort();
    assert_eq!(
        moved,
        vec!["b/a/deep/y.md".to_string(), "b/a/x.md".to_string()]
    );
    assert!(!vault.exists("a"));
    assert_eq!(vault.read("b/a/x.md"), "the note");
    assert_eq!(vault.read("b/a/deep/y.md"), "the deep note");
    assert_eq!(vault.read("links.md"), "see [[x]] and [[y]]");
}

#[test]
fn a_folder_move_rewrites_qualified_links_to_the_notes_inside_it() {
    let vault = Vault::new("move-folder-qualified");
    vault.stage("a/x.md", "the note");
    vault.stage("b/keep.md", "k");
    vault.stage("links.md", "see [[a/x]]");
    vault.bikeshed().move_into("a", "b").unwrap();
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn a_folder_move_rewrites_the_links_between_the_notes_it_carries() {
    let vault = Vault::new("move-folder-inner");
    vault.stage("a/x.md", "see [[y]]");
    vault.stage("a/y.md", "the other note");
    vault.stage("b/keep.md", "k");
    vault.bikeshed().move_into("a", "b").unwrap();
    assert_eq!(vault.read("b/a/x.md"), "see [[y]]");
}

#[test]
fn a_folder_move_into_a_folder_holding_the_same_basename_rewrites_qualified() {
    let vault = Vault::new("move-folder-ambiguous");
    vault.stage("a/x.md", "the note");
    vault.stage("b/x.md", "the other note");
    vault.stage("b/keep.md", "k");
    vault.stage("links.md", "see [[a/x]]");
    vault.bikeshed().move_into("a", "b").unwrap();
    assert_eq!(vault.read("links.md"), "see [[b/a/x]]");
}

#[test]
fn a_folder_move_takes_its_attachments_with_it() {
    let vault = Vault::new("move-folder-attachments");
    vault.stage("a/x.md", "the note");
    vault.stage("a/pic.png", "not really a png");
    vault.stage("b/keep.md", "k");
    vault.bikeshed().move_into("a", "b").unwrap();
    assert!(vault.exists("b/a/pic.png"));
    assert!(!vault.exists("a"));
}

#[test]
fn the_tree_reflects_a_folder_move() {
    let vault = Vault::new("move-folder-tree");
    vault.stage("a/x.md", "the note");
    vault.stage("b/keep.md", "k");
    let bikeshed = vault.bikeshed();
    bikeshed.move_into("a", "b").unwrap();

    let tree = bikeshed.list().unwrap().value;
    assert_eq!(
        tree.iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
    assert_eq!(
        tree[0]
            .children
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "keep.md"]
    );
}

#[test]
fn moving_a_folder_into_its_own_descendant_is_refused_as_a_cycle() {
    let vault = Vault::new("move-cycle");
    vault.stage("a/x.md", "the note");
    vault.stage("a/deep/y.md", "the deep note");
    vault.stage("links.md", "see [[x]]");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.move_into("a", "a/deep").unwrap_err(),
        DomainError::MoveCycle {
            path: "a".to_string(),
            into: "a/deep".to_string(),
        }
    );
    assert!(vault.exists("a/x.md"));
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn moving_a_folder_into_itself_is_refused_as_a_cycle() {
    let vault = Vault::new("move-into-itself");
    vault.stage("a/x.md", "the note");
    assert_eq!(
        vault.bikeshed().move_into("a", "a").unwrap_err(),
        DomainError::MoveCycle {
            path: "a".to_string(),
            into: "a".to_string(),
        }
    );
    assert!(vault.exists("a/x.md"));
}

#[test]
fn moving_a_note_onto_a_taken_name_is_refused_and_nothing_is_written() {
    let vault = Vault::new("move-collision");
    vault.stage("a/x.md", "the note");
    vault.stage("b/x.md", "the other note");
    vault.stage("links.md", "see [[a/x]]");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.move_into("a/x.md", "b").unwrap_err(),
        DomainError::NoteAlreadyExists("b/x.md".to_string())
    );
    assert!(vault.exists("a/x.md"));
    assert_eq!(vault.read("b/x.md"), "the other note");
    assert_eq!(vault.read("links.md"), "see [[a/x]]");
}

#[test]
fn moving_a_folder_onto_a_taken_name_is_refused_and_nothing_is_written() {
    let vault = Vault::new("move-folder-collision");
    vault.stage("a/x.md", "the note");
    vault.stage("b/a/y.md", "the other note");
    vault.stage("links.md", "see [[x]]");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.move_into("a", "b").unwrap_err(),
        DomainError::NoteAlreadyExists("b/a".to_string())
    );
    assert!(vault.exists("a/x.md"));
    assert!(vault.exists("b/a/y.md"));
    assert_eq!(vault.read("links.md"), "see [[x]]");
}

#[test]
fn moving_a_missing_note_is_unknown_note() {
    let vault = Vault::new("move-missing-source");
    vault.stage("b/keep.md", "k");
    assert_eq!(
        vault.bikeshed().move_into("Nope.md", "b").unwrap_err(),
        DomainError::UnknownNote("Nope.md".to_string())
    );
}

#[test]
fn moving_into_a_folder_that_does_not_exist_is_unknown_note() {
    let vault = Vault::new("move-missing-destination");
    vault.stage("a/x.md", "the note");
    assert_eq!(
        vault.bikeshed().move_into("a/x.md", "nope").unwrap_err(),
        DomainError::UnknownNote("nope".to_string())
    );
    assert!(vault.exists("a/x.md"));
}

#[test]
fn a_bikeshed_escape_in_the_destination_is_refused() {
    let vault = Vault::new("move-escape");
    vault.stage("a/x.md", "the note");
    assert_eq!(
        vault
            .bikeshed()
            .move_into("a/x.md", "../escape")
            .unwrap_err(),
        DomainError::InvalidName("../escape".to_string())
    );
    assert!(vault.exists("a/x.md"));
}
