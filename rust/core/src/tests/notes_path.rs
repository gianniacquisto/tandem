//! Ported from `NotesPathTests`: the path-hygiene rule (ADR 0008), the one
//! place that decides whether a notes-root-relative path is legal or an
//! escape.

use std::path::{Path, PathBuf};

use crate::notes::NotesPath;

fn root() -> PathBuf {
    PathBuf::from("/tmp/tandem-notes")
}

#[test]
fn legal_paths_resolve_under_the_root() {
    assert_eq!(
        NotesPath::url(&root(), "Note.md"),
        Some(PathBuf::from("/tmp/tandem-notes/Note.md"))
    );
    assert_eq!(
        NotesPath::url(&root(), "a/b/C.md"),
        Some(PathBuf::from("/tmp/tandem-notes/a/b/C.md"))
    );
    // Spaces are legal: the create/rename rule allows them.
    assert_eq!(
        NotesPath::url(&root(), "My Note.md"),
        Some(PathBuf::from("/tmp/tandem-notes/My Note.md"))
    );
    // `#` and `|` are tolerated here (pre-existing on-disk violators).
    assert!(NotesPath::url(&root(), "a#b.md").is_some());
}

#[test]
fn an_empty_path_empty_components_and_dot_escapes_are_refused() {
    for bad in [
        "",
        "/Note.md",
        "a//b.md",
        "a/",
        ".",
        "..",
        "../outside.md",
        "a/../../outside.md",
        "a/./b.md",
    ] {
        assert_eq!(NotesPath::url(&root(), bad), None, "refused: {bad}");
    }
}

#[test]
fn the_resolved_path_is_standardized_regardless_of_the_roots_shape() {
    let messy = Path::new("/tmp/tandem-notes/./");
    assert_eq!(
        NotesPath::url(messy, "a/b.md"),
        Some(PathBuf::from("/tmp/tandem-notes/a/b.md"))
    );
}

#[test]
fn a_component_containing_a_slash_cannot_smuggle_a_path() {
    assert_eq!(NotesPath::url(&root(), "a/../b.md"), None);
}

#[test]
fn a_windows_shaped_traversal_component_is_refused() {
    // A `\` is a Windows separator and `C:` a drive: `push` would reinterpret
    // either as a path of its own, so no such component may reach the vault.
    for bad in [
        "..\\..\\evil.md",
        "a\\..\\..\\evil.md",
        "C:\\evil.md",
        "\\Users\\me\\evil.md",
        "sub\\file.md",
    ] {
        assert_eq!(NotesPath::url(&root(), bad), None, "refused: {bad}");
    }
}

#[test]
fn a_non_ascii_name_is_not_sliced_mid_character() {
    assert_eq!(NotesPath::name("naive"), "naive");
    assert_eq!(NotesPath::name("naïve"), "naïve");
    assert_eq!(NotesPath::name("éé"), "éé");
    assert_eq!(NotesPath::name("naïve.md"), "naïve");
    assert_eq!(NotesPath::name("éé.MD"), "éé");
    assert_eq!(NotesPath::name("a/b/naïve.md"), "naïve");
    assert_eq!(NotesPath::name(".md"), "");
}

// MARK: - Links (COR-14)
//
// A symlink inside the vault is never followed (ADR 0008). The link tests are
// unix-only: creating a symlink or a junction needs a privilege the Windows
// runner does not always grant, so the Windows branch of the rule is covered by
// the same metadata check without a test of its own here.

#[cfg(unix)]
mod links {
    use std::fs;
    use std::os::unix::fs::symlink;

    use crate::notes::NotesPath;

    use super::super::notes_support::Vault;

    /// A folder outside the vault holding a secret note, plus a directory link
    /// and a file link to it inside the vault.
    fn vault_with_links(name: &str) -> (Vault, std::path::PathBuf) {
        let vault = Vault::new(name);
        fs::create_dir_all(vault.dir()).expect("the vault exists");
        let outside = vault.parent().join("outside");
        fs::create_dir_all(&outside).expect("the outside folder exists");
        fs::write(outside.join("secret.md"), "SECRET needle").expect("the secret is written");
        symlink(&outside, vault.dir().join("link")).expect("the directory link is made");
        symlink(outside.join("secret.md"), vault.dir().join("note-link.md"))
            .expect("the file link is made");
        (vault, outside)
    }

    fn outside_is_untouched(outside: &std::path::Path) {
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).expect("the secret survives"),
            "SECRET needle"
        );
        assert!(
            !outside.join("new.md").exists(),
            "nothing was created outside"
        );
        assert!(
            !outside.join("sub").exists(),
            "no folder was created outside"
        );
    }

    #[test]
    fn a_path_through_a_link_is_no_url() {
        let (vault, _) = vault_with_links("link-url");
        assert_eq!(NotesPath::url(vault.dir(), "link/secret.md"), None);
        assert_eq!(NotesPath::url(vault.dir(), "note-link.md"), None);
        assert!(NotesPath::url(vault.dir(), "real/ok.md").is_some());
    }

    #[test]
    fn a_directory_link_refuses_read_write_create_and_folder_create() {
        let (vault, outside) = vault_with_links("link-dir-ops");
        let notes = vault.notes();
        assert!(notes.read("link/secret.md").is_err());
        assert!(notes.write("link/secret.md", "PWNED", None).is_err());
        assert!(notes.create("link/new.md").is_err());
        assert!(notes.create_folder("link/sub").is_err());
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).expect("the secret survives"),
            "SECRET needle"
        );
        outside_is_untouched(&outside);
    }

    #[test]
    fn a_file_link_refuses_read_write_delete_rename_and_move() {
        let (vault, outside) = vault_with_links("link-file-ops");
        let notes = vault.notes();
        assert!(notes.read("note-link.md").is_err());
        assert!(notes.write("note-link.md", "PWNED", None).is_err());
        assert!(notes.delete("note-link.md").is_err());
        assert!(notes.rename("note-link.md", "moved.md").is_err());
        assert!(notes.move_into("note-link.md", "").is_err());
        assert!(
            vault.dir().join("note-link.md").symlink_metadata().is_ok(),
            "the link itself is not removed"
        );
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).expect("the secret survives"),
            "SECRET needle"
        );
    }

    #[test]
    fn a_move_into_a_directory_link_is_refused() {
        let (vault, outside) = vault_with_links("link-move-dest");
        vault.stage("keep.md", "keep");
        assert!(vault.notes().move_into("keep.md", "link").is_err());
        assert!(vault.exists("keep.md"), "the note stays in the vault");
        assert!(!outside.join("keep.md").exists());
    }

    #[test]
    fn the_tree_skips_links_and_search_health_and_backlinks_never_read_through() {
        let (vault, _) = vault_with_links("link-readers");
        vault.stage("real.md", "needle inside the vault");
        let notes = vault.notes();
        let paths: Vec<String> = notes
            .list()
            .expect("the tree lists")
            .value
            .into_iter()
            .map(|node| node.path)
            .collect();
        assert_eq!(paths, vec!["real.md".to_string()]);
        let hits = notes.search("SECRET").expect("search runs").value;
        assert!(hits.is_empty(), "search does not read through a link");
        let hits = notes.search("needle").expect("search runs").value;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "real.md");
        notes
            .health(&crate::notes::BoardNames::default())
            .expect("health runs");
        assert!(notes.backlinks("link/secret.md").is_err());
    }

    #[test]
    fn a_link_cycle_inside_the_vault_does_not_hang_the_walks() {
        let vault = Vault::new("link-cycle");
        vault.stage("a.md", "one");
        symlink(vault.dir(), vault.dir().join("loop")).expect("the cycle is made");
        let notes = vault.notes();
        let paths: Vec<String> = notes
            .list()
            .expect("the tree lists")
            .value
            .into_iter()
            .map(|node| node.path)
            .collect();
        assert_eq!(paths, vec!["a.md".to_string()]);
        notes
            .health(&crate::notes::BoardNames::default())
            .expect("health returns");
        notes.stats(None).expect("stats returns");
    }

    #[test]
    fn a_linked_projects_root_is_skipped_by_stats_and_health() {
        let vault = Vault::new("link-projects-root");
        fs::create_dir_all(vault.dir()).expect("the vault exists");
        let outside = vault.parent().join("outside-projects");
        fs::create_dir_all(outside.join("secret")).expect("the outside project exists");
        fs::write(outside.join("secret").join("secret.md"), "SECRET needle")
            .expect("the secret is written");
        symlink(&outside, vault.dir().join("projects")).expect("the projects link is made");
        let notes = vault.notes();
        let stats = notes.stats(None).expect("stats runs").value;
        assert!(
            stats.projects.is_empty(),
            "stats do not read through a linked projects root"
        );
        assert_eq!(stats.totals.curated_notes, 0);
        let named = notes.stats(Some("secret")).expect("stats runs").value;
        assert!(named.projects.is_empty());
        let health = notes
            .health(&crate::notes::BoardNames::default())
            .expect("health runs")
            .value;
        assert_eq!(
            health.counts.notes, 0,
            "health does not read through the link"
        );
        assert!(outside.join("secret").join("secret.md").exists());
    }

    #[test]
    fn a_vault_whose_root_is_itself_a_link_still_works() {
        let vault = Vault::new("link-root");
        fs::create_dir_all(vault.dir()).expect("the real vault exists");
        let alias = vault.parent().join("alias");
        symlink(vault.dir(), &alias).expect("the root link is made");
        let notes = crate::notes::Notes::new(&alias);
        notes
            .create("a.md")
            .expect("a note is created through the root link");
        notes
            .write("a.md", "hello", None)
            .expect("the note is written");
        assert_eq!(
            notes.read("a.md").expect("the note reads").value.text,
            "hello"
        );
        assert_eq!(vault.read("a.md"), "hello");
    }
}
