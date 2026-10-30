//! Ported from `BikeshedRenameTests`: one locked transaction, every wikilink
//! form rewritten (self-links included, re-canonicalised against the post-rename
//! file set), then one `rename(2)`. Markdown links, frontmatter, embeds, code and
//! non-resolving links stay byte-identical.

use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

#[test]
fn rename_moves_the_note_and_rewrites_bare_links() {
    let vault = Vault::new("rename-bare");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");
    let bikeshed = vault.bikeshed();

    let result = bikeshed.rename("B.md", "C.md").unwrap();
    assert_eq!(result.value, "C.md");
    assert!(!vault.exists("B.md"));
    assert_eq!(vault.read("C.md"), "bee");
    assert_eq!(vault.read("A.md"), "see [[C]]");
}

#[test]
fn rename_returns_the_new_path_and_the_tree_reflects_the_move() {
    let vault = Vault::new("rename-tree");
    vault.stage("projects/B.md", "bee");
    vault.stage("A.md", "x");
    let bikeshed = vault.bikeshed();
    bikeshed.rename("projects/B.md", "C.md").unwrap();

    assert_eq!(vault.read("C.md"), "bee");
    let tree = bikeshed.list().unwrap().value;
    assert_eq!(
        tree.iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        vec!["projects", "A.md", "C.md"]
    );
    assert_eq!(tree[0].kind, crate::bikeshed::NodeKind::Folder);
    assert!(tree[0].children.is_empty());
}

#[test]
fn rename_rewrites_every_form() {
    let vault = Vault::new("rename-forms");
    vault.stage("B.md", "bee");
    let original = "1 [[B]]\n2 [[B|An Alias]]\n3 [[B#Heading]]\n4 [[B#Heading|Alias]]\n5 [[B|Alias#Heading]]\n";
    vault.stage("A.md", original);
    let bikeshed = vault.bikeshed();

    bikeshed.rename("B.md", "C.md").unwrap();

    assert_eq!(
        vault.read("A.md"),
        original
            .replace("1 [[B]]", "1 [[C]]")
            .replace("2 [[B|An Alias]]", "2 [[C|An Alias]]")
            .replace("3 [[B#Heading]]", "3 [[C#Heading]]")
            .replace("4 [[B#Heading|Alias]]", "4 [[C#Heading|Alias]]")
            .replace("5 [[B|Alias#Heading]]", "5 [[C|Alias#Heading]]")
    );
}

#[test]
fn rename_rewrites_qualified_and_dot_md_links() {
    let vault = Vault::new("rename-qualified");
    vault.stage("projects/plan.md", "p");
    vault.stage("A.md", "a [[projects/plan]] b [[projects/plan.md]]");
    vault
        .bikeshed()
        .rename("projects/plan.md", "plan.md")
        .unwrap();
    assert_eq!(vault.read("A.md"), "a [[plan]] b [[plan]]");
}

#[test]
fn rename_is_case_insensitive() {
    let vault = Vault::new("rename-case");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[b]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "see [[C]]");
}

#[test]
fn rename_includes_the_notes_own_self_links() {
    let vault = Vault::new("rename-self");
    vault.stage("B.md", "note to [[B]] and [[B|self]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("C.md"), "note to [[C]] and [[C|self]]");
}

#[test]
fn rename_into_a_taken_basename_rewrites_to_the_qualified_form() {
    let vault = Vault::new("rename-ambiguous");
    vault.stage("x.md", "x");
    vault.stage("y.md", "y");
    vault.stage("A.md", "root [[x]] sub [[y]]");
    vault.bikeshed().rename("x.md", "a/y.md").unwrap();
    assert_eq!(vault.read("A.md"), "root [[a/y]] sub [[y]]");
}

#[test]
fn rename_to_a_unique_basename_rewrites_to_the_bare_form() {
    let vault = Vault::new("rename-unique");
    vault.stage("a/x.md", "a x");
    vault.stage("A.md", "see [[a/x]]");
    vault.bikeshed().rename("a/x.md", "x.md").unwrap();
    assert_eq!(vault.read("A.md"), "see [[x]]");
}

#[test]
fn a_bare_link_broken_by_duplicates_is_not_rewritten() {
    let vault = Vault::new("rename-broken-bare");
    vault.stage("x.md", "root x");
    vault.stage("b/x.md", "b x");
    vault.stage("A.md", "broken [[x]]");
    let bikeshed = vault.bikeshed();
    bikeshed.rename("x.md", "y.md").unwrap();

    assert_eq!(vault.read("A.md"), "broken [[x]]");
    assert_eq!(
        bikeshed.resolve("x"),
        crate::bikeshed::wikilink::LinkResolution::Resolved("b/x.md".to_string())
    );
}

#[test]
fn rename_leaves_markdown_links_byte_identical() {
    let vault = Vault::new("rename-markdown");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "[text](B.md) and [[B]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "[text](B.md) and [[C]]");
}

#[test]
fn rename_leaves_wikilinks_in_leading_frontmatter_byte_identical() {
    let vault = Vault::new("rename-frontmatter");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "---\nlinks: [[B]]\n---\nbody [[B]]\n");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "---\nlinks: [[B]]\n---\nbody [[C]]\n");
}

#[test]
fn an_unterminated_leading_block_is_not_frontmatter() {
    let vault = Vault::new("rename-unterminated");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "---\nlinks: [[B]]\nno closing fence");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "---\nlinks: [[C]]\nno closing fence");
}

#[test]
fn rename_leaves_embeds_byte_identical() {
    let vault = Vault::new("rename-embed");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "![[B]] and [[B]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "![[B]] and [[C]]");
}

#[test]
fn rename_leaves_non_resolving_links_byte_identical() {
    let vault = Vault::new("rename-broken");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "[[Missing]] and [[B]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "[[Missing]] and [[C]]");
}

#[test]
fn rename_leaves_wikilinks_in_code_blocks_byte_identical() {
    let vault = Vault::new("rename-code");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "```\n[[B]]\n```\nreal [[B]]");
    vault.bikeshed().rename("B.md", "C.md").unwrap();
    assert_eq!(vault.read("A.md"), "```\n[[B]]\n```\nreal [[C]]");
}

#[test]
fn rename_to_a_taken_path_is_refused_and_nothing_is_written() {
    let vault = Vault::new("rename-taken");
    vault.stage("B.md", "bee");
    vault.stage("C.md", "cee");
    vault.stage("A.md", "see [[B]]");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.rename("B.md", "C.md").unwrap_err(),
        DomainError::NoteAlreadyExists("C.md".to_string())
    );
    assert!(vault.exists("B.md"));
    assert_eq!(vault.read("C.md"), "cee");
    assert_eq!(vault.read("A.md"), "see [[B]]");
}

#[test]
fn rename_of_a_missing_note_is_unknown_note() {
    let vault = Vault::new("rename-missing");
    vault.stage("B.md", "bee");
    assert_eq!(
        vault.bikeshed().rename("Nope.md", "C.md").unwrap_err(),
        DomainError::UnknownNote("Nope.md".to_string())
    );
    assert!(vault.exists("B.md"));
}

#[test]
fn rename_to_an_invalid_new_name_is_refused_before_anything_is_written() {
    let vault = Vault::new("rename-invalid");
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");
    let bikeshed = vault.bikeshed();

    for bad in [
        "bad#name.md",
        "bad|name.md",
        "noext",
        "../escape.md",
        ".hidden.md",
    ] {
        assert_eq!(
            bikeshed.rename("B.md", bad).unwrap_err(),
            DomainError::InvalidName(bad.to_string()),
            "refused: {bad}"
        );
    }
    assert!(vault.exists("B.md"));
    assert_eq!(vault.read("A.md"), "see [[B]]");
}

#[test]
fn rename_into_a_new_folder_creates_the_parent_directories() {
    let vault = Vault::new("rename-parents");
    vault.stage("B.md", "bee");
    vault.bikeshed().rename("B.md", "deep/nested/C.md").unwrap();
    assert_eq!(vault.read("deep/nested/C.md"), "bee");
    assert!(!vault.exists("B.md"));
}
