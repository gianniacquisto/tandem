//! Ported from `BikeshedNoteOpsTests`: create / read / write / list / delete,
//! the tree shape, hidden files and attachments, missing folder = empty tree,
//! name validation, an existing target is an error not an overwrite, and write
//! stores exactly the bytes given.

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::domain::error::DomainError;

use super::bikeshed_support::{Vault, sha256};

fn shape(nodes: &[BikeshedNode]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    fn walk(nodes: &[BikeshedNode], out: &mut Vec<(String, bool)>) {
        for node in nodes {
            out.push((node.path.clone(), node.kind == NodeKind::Note));
            walk(&node.children, out);
        }
    }
    walk(nodes, &mut out);
    out
}

fn tree(bikeshed: &Bikeshed) -> Vec<BikeshedNode> {
    bikeshed.list().expect("the tree lists").value
}

#[test]
fn list_on_a_missing_folder_is_an_empty_tree() {
    let vault = Vault::new("ops-list-missing");
    let result = vault.bikeshed().list().expect("a missing vault lists");
    assert!(result.value.is_empty());
    assert_eq!(result.note, None);
    assert!(!vault.dir().exists());
}

#[test]
fn list_folders_first_alphabetically_within_each_level() {
    let vault = Vault::new("ops-list-shape");
    let bikeshed = vault.bikeshed();
    for path in [
        "b.md",
        "A.md",
        "z/note.md",
        "z/alpha.md",
        "z/beta.md",
        "apple/x.md",
    ] {
        bikeshed.create(path).expect("the note is created");
    }

    let expected: Vec<(String, bool)> = [
        ("apple", false),
        ("apple/x.md", true),
        ("z", false),
        ("z/alpha.md", true),
        ("z/beta.md", true),
        ("z/note.md", true),
        ("A.md", true),
        ("b.md", true),
    ]
    .into_iter()
    .map(|(path, note)| (path.to_string(), note))
    .collect();
    assert_eq!(shape(&tree(&bikeshed)), expected);
}

#[test]
fn list_excludes_hidden_files_hidden_folders_and_attachments() {
    let vault = Vault::new("ops-list-exclusions");
    vault.stage("ok.md", "visible");
    vault.stage(".hidden.md", "hidden note");
    vault.stage("photo.png", "not markdown");
    vault.stage(".DS_Store", "junk");
    vault.stage(".hiddendir/inside.md", "in a hidden folder");
    vault.stage("sub/figure.svg", "attachment in a folder");

    let found = shape(&tree(&vault.bikeshed()));
    assert_eq!(
        found,
        vec![("sub".to_string(), false), ("ok.md".to_string(), true)]
    );
}

#[test]
fn list_treats_md_case_insensitively() {
    let vault = Vault::new("ops-list-uppercase");
    vault.stage("NOTE.MD", "uppercase");
    assert_eq!(
        shape(&tree(&vault.bikeshed())),
        vec![("NOTE.MD".to_string(), true)]
    );
}

#[test]
fn create_makes_an_empty_note_creating_parent_folders() {
    let vault = Vault::new("ops-create");
    let bikeshed = vault.bikeshed();

    let result = bikeshed
        .create("Projects/My Note.md")
        .expect("the note is created");
    assert_eq!(result.value, "Projects/My Note.md");
    assert_eq!(result.note, None);
    assert!(vault.exists("Projects/My Note.md"));
    assert_eq!(vault.read("Projects/My Note.md"), "", "a new note is empty");
    assert!(vault.dir().exists());
}

#[test]
fn create_allows_spaces_but_rejects_hash_pipe_and_malformed_paths() {
    let vault = Vault::new("ops-create-names");
    let bikeshed = vault.bikeshed();
    bikeshed.create("My Note.md").expect("spaces are allowed");

    for bad in [
        "bad#name.md",
        "bad|name.md",
        "folder/bad#name.md",
        "folder/bad|name.md",
    ] {
        assert_eq!(
            bikeshed.create(bad).unwrap_err(),
            DomainError::InvalidName(bad.to_string())
        );
    }
    for bad in [
        "a//b.md",
        "/absolute.md",
        "trailing/.md",
        "../escape.md",
        "a/../b.md",
        "./dot.md",
        "",
    ] {
        assert_eq!(
            bikeshed.create(bad).unwrap_err(),
            DomainError::InvalidName(bad.to_string()),
            "refused: {bad}"
        );
    }

    assert_eq!(
        shape(&tree(&bikeshed)),
        vec![("My Note.md".to_string(), true)]
    );
}

#[test]
fn create_requires_a_markdown_extension() {
    let vault = Vault::new("ops-create-extension");
    let bikeshed = vault.bikeshed();
    assert_eq!(
        bikeshed.create("notmarkdown").unwrap_err(),
        DomainError::InvalidName("notmarkdown".to_string())
    );
    assert_eq!(
        bikeshed.create("image.png").unwrap_err(),
        DomainError::InvalidName("image.png".to_string())
    );
    bikeshed
        .create("UPPER.MD")
        .expect("uppercase .MD is a note");
    assert_eq!(
        shape(&tree(&bikeshed)),
        vec![("UPPER.MD".to_string(), true)]
    );
}

#[test]
fn create_on_an_existing_target_is_an_error() {
    let vault = Vault::new("ops-create-existing");
    let bikeshed = vault.bikeshed();
    bikeshed.write("a.md", "original").unwrap();

    assert_eq!(
        bikeshed.create("a.md").unwrap_err(),
        DomainError::NoteAlreadyExists("a.md".to_string())
    );
    assert_eq!(bikeshed.read("a.md").unwrap().value.text, "original");

    bikeshed.create("f/n.md").unwrap();
    assert_eq!(
        bikeshed.create("f/n.md").unwrap_err(),
        DomainError::NoteAlreadyExists("f/n.md".to_string())
    );
    bikeshed.create("f.md").unwrap();
    assert_eq!(
        bikeshed.create("f.md/n.md").unwrap_err(),
        DomainError::NoteAlreadyExists("f.md".to_string())
    );
}

#[test]
fn create_folder_makes_a_nested_folder() {
    let vault = Vault::new("ops-create-folder");
    let bikeshed = vault.bikeshed();

    let result = bikeshed
        .create_folder("projects/tandem")
        .expect("the folder is created");
    assert_eq!(result.value, "projects/tandem");
    assert_eq!(result.note, None);
    assert!(vault.dir().join("projects/tandem").is_dir());
    assert_eq!(
        std::fs::read_dir(vault.dir().join("projects/tandem"))
            .expect("the folder is readable")
            .count(),
        0
    );
    assert_eq!(
        shape(&tree(&bikeshed)),
        vec![
            ("projects".to_string(), false),
            ("projects/tandem".to_string(), false)
        ]
    );
}

#[test]
fn create_folder_on_an_existing_target_is_an_error() {
    let vault = Vault::new("ops-create-folder-existing");
    let bikeshed = vault.bikeshed();
    bikeshed.create_folder("projects").unwrap();
    assert_eq!(
        bikeshed.create_folder("projects").unwrap_err(),
        DomainError::NoteAlreadyExists("projects".to_string())
    );
    bikeshed.create("a.md").unwrap();
    assert_eq!(
        bikeshed.create_folder("a.md").unwrap_err(),
        DomainError::NoteAlreadyExists("a.md".to_string())
    );
}

#[test]
fn create_folder_name_validation() {
    let vault = Vault::new("ops-create-folder-names");
    let bikeshed = vault.bikeshed();
    bikeshed
        .create_folder("My Project")
        .expect("spaces are allowed");

    for bad in [
        "bad#name",
        "bad|name",
        "folder/bad#name",
        "folder/bad|name",
        ".hidden",
        "sub/.hidden",
        "",
        "a//b",
        "/absolute",
        "trailing/",
        "../escape",
        "a/../b",
        "./dot",
    ] {
        assert_eq!(
            bikeshed.create_folder(bad).unwrap_err(),
            DomainError::InvalidName(bad.to_string()),
            "refused: {bad}"
        );
    }
    assert_eq!(
        shape(&tree(&bikeshed)),
        vec![("My Project".to_string(), false)]
    );
}

#[test]
fn create_folder_refuses_an_ancestor_that_is_a_file() {
    let vault = Vault::new("ops-create-folder-ancestor");
    let bikeshed = vault.bikeshed();
    bikeshed.create("file.md").unwrap();
    assert_eq!(
        bikeshed.create_folder("file.md/inner/deep").unwrap_err(),
        DomainError::NoteAlreadyExists("file.md".to_string())
    );
    assert_eq!(bikeshed.read("file.md").unwrap().value.text, "");
    assert_eq!(shape(&tree(&bikeshed)), vec![("file.md".to_string(), true)]);
}

#[test]
fn read_returns_text_hash_and_mtime() {
    let vault = Vault::new("ops-read");
    let bikeshed = vault.bikeshed();
    let text = "# Heading\n\nbody text";
    bikeshed.write("a.md", text).unwrap();
    let before = std::time::SystemTime::now();

    let note = bikeshed.read("a.md").unwrap().value;
    assert_eq!(note.text, text);
    assert_eq!(note.hash, sha256(text));
    assert!(note.modified_at + std::time::Duration::from_secs(5) >= before);
    assert!(note.modified_at <= std::time::SystemTime::now() + std::time::Duration::from_secs(5));
}

#[test]
fn read_on_a_missing_note_is_unknown_note() {
    let vault = Vault::new("ops-read-missing");
    assert_eq!(
        vault.bikeshed().read("missing.md").unwrap_err(),
        DomainError::UnknownNote("missing.md".to_string())
    );
}

#[test]
fn write_stores_exactly_the_bytes_given() {
    let vault = Vault::new("ops-write-bytes");
    let bikeshed = vault.bikeshed();
    let text = "---\ntags: [x, y]\nlink: [[Somewhere]]\n---\n\n# Title\n\
                See [[Somewhere#Section|here]] and `[[inline code is not a link]]`.\n";
    let record = bikeshed.write("fm.md", text).unwrap().value;
    assert_eq!(record.path, "fm.md");
    assert_eq!(record.hash, sha256(text));
    assert_eq!(vault.read("fm.md"), text);
    assert_eq!(bikeshed.read("fm.md").unwrap().value.text, text);
}

#[test]
fn write_overwrites_an_existing_note_and_records_the_new_hash() {
    let vault = Vault::new("ops-write-overwrite");
    let bikeshed = vault.bikeshed();
    let first = bikeshed.write("a.md", "one").unwrap().value;
    let second = bikeshed.write("a.md", "two").unwrap().value;
    assert_ne!(first.hash, second.hash);
    assert_eq!(second.hash, sha256("two"));
    assert_eq!(bikeshed.read("a.md").unwrap().value.text, "two");
}

#[test]
fn delete_removes_a_note_and_a_folder_recursively() {
    let vault = Vault::new("ops-delete");
    let bikeshed = vault.bikeshed();
    bikeshed.create("a.md").unwrap();
    bikeshed.create("f/inner.md").unwrap();
    bikeshed.create("keep.md").unwrap();

    bikeshed.delete("a.md").unwrap();
    assert!(!vault.exists("a.md"));
    bikeshed.delete("f").unwrap();
    assert!(!vault.exists("f"));
    assert_eq!(shape(&tree(&bikeshed)), vec![("keep.md".to_string(), true)]);
}

#[test]
fn delete_on_a_missing_path_is_unknown_note() {
    let vault = Vault::new("ops-delete-missing");
    assert_eq!(
        vault.bikeshed().delete("nope.md").unwrap_err(),
        DomainError::UnknownNote("nope.md".to_string())
    );
}

#[test]
fn write_new_never_overwrites_and_suffixes_the_free_variant() {
    use crate::domain::ticket_to_note::Notes;

    let vault = Vault::new("ops-write-new");
    let bikeshed = vault.bikeshed();
    assert_eq!(
        bikeshed.write_new("projects/t/a.md", "one").unwrap(),
        "projects/t/a.md"
    );
    assert_eq!(
        bikeshed.write_new("projects/t/a.md", "two").unwrap(),
        "projects/t/a-2.md"
    );
    assert_eq!(vault.read("projects/t/a.md"), "one");
    assert_eq!(vault.read("projects/t/a-2.md"), "two");
}

#[test]
fn pre_existing_name_violators_are_tolerated() {
    let vault = Vault::new("ops-violators");
    // `#` is legal on every platform and is one wikilink separator.
    vault.stage("bad#name.md", "an agent wrote this");
    // `|` is the grammar's other separator but an illegal Windows filename, so
    // the pipe half is staged only where the host can hold it.
    #[cfg(not(windows))]
    vault.stage("bad|pipe.md", "another one");
    let bikeshed = vault.bikeshed();

    let mut paths: Vec<String> = shape(&tree(&bikeshed))
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    paths.sort();
    #[cfg(not(windows))]
    assert_eq!(
        paths,
        vec!["bad#name.md".to_string(), "bad|pipe.md".to_string()]
    );
    #[cfg(windows)]
    assert_eq!(paths, vec!["bad#name.md".to_string()]);

    assert_eq!(
        bikeshed.read("bad#name.md").unwrap().value.text,
        "an agent wrote this"
    );
    bikeshed.write("bad#name.md", "edited").unwrap();
    assert_eq!(bikeshed.read("bad#name.md").unwrap().value.text, "edited");
    assert_eq!(
        bikeshed.create("bad#name.md").unwrap_err(),
        DomainError::InvalidName("bad#name.md".to_string())
    );
}
