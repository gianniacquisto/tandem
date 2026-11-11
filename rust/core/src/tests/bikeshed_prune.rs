//! `note prune`: the capture archive moves files into `.done/` and leaves the
//! source session folder behind, empty, forever. This suite pins the sweep's
//! guarantees at the Bikeshed seam, one test per guarantee the consolidation
//! protocol names: the empty stub outside and inside `.done`, the nested chain,
//! the files and the non-empty folders that are never candidates, the two
//! containers that always survive, and the empty or missing `.raw` that is
//! simply nothing to do.

use std::fs;

use super::bikeshed_support::Vault;

/// Run the sweep against the sandbox vault and answer the removed count.
fn prune(vault: &Vault) -> usize {
    vault
        .bikeshed()
        .prune()
        .expect("the sweep runs")
        .value
        .removed
}

fn folder(vault: &Vault, relative: &str) {
    fs::create_dir_all(vault.dir().join(relative)).expect("the folder is creatable");
}

fn is_dir(vault: &Vault, relative: &str) -> bool {
    vault.dir().join(relative).is_dir()
}

#[test]
fn an_empty_session_folder_outside_done_is_removed() {
    let vault = Vault::new("prune-stub");
    folder(&vault, "projects/tandem/.raw/ses_stub");
    folder(&vault, "projects/code-rank/.raw/ses_other");
    folder(&vault, "projects/tandem/empty_outside_raw");

    assert_eq!(prune(&vault), 2);
    assert!(!is_dir(&vault, "projects/tandem/.raw/ses_stub"));
    assert!(!is_dir(&vault, "projects/code-rank/.raw/ses_other"));
    assert!(is_dir(&vault, "projects/tandem/.raw"), "`.raw` survives");
    assert!(
        is_dir(&vault, "projects/tandem/empty_outside_raw"),
        "a folder outside `.raw` is not the sweep's to remove"
    );
}

#[test]
fn an_empty_session_folder_inside_done_is_removed() {
    let vault = Vault::new("prune-done-stub");
    folder(&vault, "projects/tandem/.raw/.done/ses_stub");

    assert_eq!(prune(&vault), 1);
    assert!(!is_dir(&vault, "projects/tandem/.raw/.done/ses_stub"));
    assert!(
        is_dir(&vault, "projects/tandem/.raw/.done"),
        "`.done` survives its last stub"
    );
}

#[test]
fn a_nested_chain_of_empty_folders_is_removed_whole() {
    let vault = Vault::new("prune-chain");
    folder(&vault, "projects/tandem/.raw/ses_chain/shots/inner");

    assert_eq!(prune(&vault), 3);
    assert!(!is_dir(&vault, "projects/tandem/.raw/ses_chain"));
}

#[test]
fn capture_files_pending_or_archived_are_never_touched() {
    let vault = Vault::new("prune-files");
    let pending = "projects/tandem/.raw/ses_pending/20261018T140211Z-fact-pending.md";
    let archived = "projects/tandem/.raw/.done/ses_archived/20261018T140211Z-fact-archived.md";
    vault.stage(pending, "pending\n");
    vault.stage(archived, "archived\n");
    folder(&vault, "projects/tandem/.raw/ses_empty");

    assert_eq!(prune(&vault), 1);
    assert_eq!(vault.read(pending), "pending\n");
    assert_eq!(vault.read(archived), "archived\n");
    assert!(is_dir(&vault, "projects/tandem/.raw/ses_pending"));
    assert!(is_dir(&vault, "projects/tandem/.raw/.done/ses_archived"));
}

#[test]
fn a_non_empty_folder_is_never_removed() {
    let vault = Vault::new("prune-non-empty");
    let capture = "projects/tandem/.raw/ses_busy/20261018T140211Z-gotcha-kept.md";
    vault.stage(capture, "kept\n");

    assert_eq!(prune(&vault), 0);
    assert!(is_dir(&vault, "projects/tandem/.raw/ses_busy"));
    assert_eq!(vault.read(capture), "kept\n");
}

#[test]
fn raw_and_done_survive_even_when_nothing_is_left_inside() {
    let vault = Vault::new("prune-containers");
    folder(&vault, "projects/tandem/.raw/.done");

    assert_eq!(prune(&vault), 0);
    assert!(is_dir(&vault, "projects/tandem/.raw"));
    assert!(is_dir(&vault, "projects/tandem/.raw/.done"));
}

#[test]
fn an_empty_or_missing_raw_reports_zero_and_no_error() {
    let empty = Vault::new("prune-empty-raw");
    folder(&empty, "projects/tandem/.raw");
    assert_eq!(prune(&empty), 0);
    assert!(is_dir(&empty, "projects/tandem/.raw"));

    let missing = Vault::new("prune-missing-raw");
    folder(&missing, "projects/tandem");
    assert_eq!(prune(&missing), 0);
    assert!(!missing.dir().join("projects/tandem/.raw").exists());
}
