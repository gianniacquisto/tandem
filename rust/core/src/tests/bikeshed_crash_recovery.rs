//! Ported from `BikeshedCrashRecoveryTests`: a mid-sequence crash leaves no torn
//! files and at most broken links; re-running the rename self-heals; an orphaned
//! temp older than 60 s is swept by the next locked operation.

use std::time::{Duration, SystemTime};

use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

/// The state a kill between the rewrite pass and the file rename leaves: A.md
/// already references C.md; B.md is not moved yet.
fn stage_crashed_rename(vault: &Vault) {
    vault.stage("A.md", "see [[C]]");
    vault.stage("B.md", "bee");
}

#[test]
fn a_crashed_rename_state_has_no_torn_files_and_at_most_broken_links() {
    let vault = Vault::new("crash-state");
    stage_crashed_rename(&vault);
    let bikeshed = vault.bikeshed();

    let entries = std::fs::read_dir(vault.dir()).unwrap();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".md") {
            let data = std::fs::read(entry.path()).unwrap();
            assert!(
                String::from_utf8(data).is_ok(),
                "{name} should be valid UTF-8"
            );
        }
        assert!(
            !(name.starts_with(".note-") && name.ends_with(".tmp")),
            "a completed write left a temp: {name}"
        );
    }
    assert_eq!(vault.read("B.md"), "bee");
    assert_eq!(vault.read("A.md"), "see [[C]]");

    assert_eq!(
        bikeshed.resolve("C"),
        crate::bikeshed::wikilink::LinkResolution::Broken
    );
    assert_eq!(
        bikeshed.resolve("B"),
        crate::bikeshed::wikilink::LinkResolution::Resolved("B.md".to_string())
    );
    assert_eq!(
        bikeshed.backlinks("C.md").unwrap_err(),
        DomainError::UnknownNote("C.md".to_string())
    );
}

#[test]
fn re_running_the_crashed_rename_self_heals() {
    let vault = Vault::new("crash-heal");
    stage_crashed_rename(&vault);
    let bikeshed = vault.bikeshed();

    bikeshed.rename("B.md", "C.md").unwrap();

    assert_eq!(vault.read("C.md"), "bee");
    assert!(!vault.exists("B.md"));
    assert_eq!(vault.read("A.md"), "see [[C]]");
    assert_eq!(
        bikeshed.resolve("C"),
        crate::bikeshed::wikilink::LinkResolution::Resolved("C.md".to_string())
    );
    assert_eq!(
        bikeshed.backlinks("C.md").unwrap().value,
        vec![crate::bikeshed::Backlink {
            path: "A.md".to_string(),
            line: "see [[C]]".to_string(),
        }]
    );
}

#[test]
fn an_orphaned_temp_older_than_60_seconds_is_cleaned_by_the_next_locked_op() {
    let vault = Vault::new("crash-orphan");
    let bikeshed = vault.bikeshed();
    vault.stage("B.md", "bee");
    vault.stage("A.md", "see [[B]]");

    let orphan = ".note-crash-0000-0000-0000-000000000001.tmp";
    vault.stage(orphan, "partial bytes");
    vault.set_modified(orphan, SystemTime::now() - Duration::from_secs(120));
    assert!(vault.exists(orphan));

    bikeshed.write("A.md", "see [[B]] still").unwrap();

    assert!(!vault.exists(orphan));
    assert_eq!(vault.read("A.md"), "see [[B]] still");
}

#[test]
fn a_recovery_names_the_temp_until_the_next_locked_op_sweeps_it() {
    let vault = Vault::new("crash-recovery");
    let bikeshed = vault.bikeshed();
    let temp = ".note-crash-0000-0000-0000-000000000001.tmp";
    vault.stage(temp, "partial bytes");

    let recoveries = bikeshed.recoveries();
    assert_eq!(recoveries.len(), 1);
    assert_eq!(recoveries[0].file, temp);
    assert_eq!(recoveries[0].bytes, "partial bytes".len() as u64);

    // A fresh temp is a writer's, not a crash's: the next locked operation
    // leaves it alone.
    bikeshed.write("A.md", "x").unwrap();
    assert_eq!(bikeshed.recoveries().len(), 1);

    // Past the sweep's age, the next locked operation takes it.
    vault.set_modified(temp, SystemTime::now() - Duration::from_secs(120));
    bikeshed.write("A.md", "y").unwrap();
    assert!(bikeshed.recoveries().is_empty());
}
