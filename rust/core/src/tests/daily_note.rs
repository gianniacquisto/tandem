//! Ported from `DailyNoteTests`: `daily/YYYY-MM-DD.md` at the vault root, the
//! app's open-or-create, the fold marker's wording and parse, and the pending set
//! found by name and never by mtime.

use std::time::{Duration, SystemTime};

use crate::bikeshed::{DailyNote, Day};

use super::bikeshed_support::Vault;

fn day(year: i32, month: u32, day: u32) -> Day {
    Day::new(year, month, day)
}

#[test]
fn a_days_note_is_daily_yyyy_mm_dd_at_the_vault_root() {
    assert_eq!(DailyNote::harvest_folders(), &["daily"]);
    assert_eq!(DailyNote::path(day(2026, 10, 26)), "daily/2026-10-26.md");
    assert_eq!(DailyNote::path(day(2026, 1, 5)), "daily/2026-01-05.md");
}

#[test]
fn opening_a_days_note_creates_it_once_and_never_overwrites() {
    let vault = Vault::new("daily-open");
    let bikeshed = vault.bikeshed();
    let today = day(2026, 10, 26);

    let path = bikeshed.open_daily_note(today).unwrap().value;
    assert_eq!(path, "daily/2026-10-26.md");
    assert!(vault.exists(&path));
    let tree = bikeshed.list().unwrap().value;
    let daily = tree.iter().find(|node| node.path == "daily").unwrap();
    assert_eq!(daily.kind, crate::bikeshed::NodeKind::Folder);
    assert_eq!(
        daily
            .children
            .iter()
            .map(|node| node.path.as_str())
            .collect::<Vec<_>>(),
        vec!["daily/2026-10-26.md"]
    );

    vault.stage(&path, "# Today\n\nwhat the day threw at me\n");
    let again = bikeshed.open_daily_note(today).unwrap().value;
    assert_eq!(again, path);
    assert_eq!(vault.read(&path), "# Today\n\nwhat the day threw at me\n");
}

#[test]
fn the_fold_marker_names_the_day_the_pass_advanced_through() {
    assert_eq!(
        DailyNote::folded_through("# Index\n\nHuman notes folded through 2026-10-26\n"),
        Some("2026-10-26".to_string())
    );
    assert_eq!(
        DailyNote::folded_through("\n- Human notes folded through 2026-01-05\n"),
        Some("2026-01-05".to_string())
    );
    assert_eq!(DailyNote::folded_through("# Index\n"), None);
    assert_eq!(
        DailyNote::folded_through("Human notes folded through soon"),
        None
    );
}

#[test]
fn the_pending_set_is_the_daily_notes_dated_after_the_marker() {
    let vault = Vault::new("daily-pending");
    for day in ["2026-10-24", "2026-10-25", "2026-10-26", "2026-10-27"] {
        vault.stage(&format!("daily/{day}.md"), &format!("# {day}\n"));
    }
    vault.stage(
        "Index.md",
        "# Index\n\nHuman notes folded through 2026-10-25\n",
    );
    assert_eq!(
        vault.bikeshed().pending_daily_notes().unwrap().value,
        vec![
            "daily/2026-10-26.md".to_string(),
            "daily/2026-10-27.md".to_string()
        ]
    );
}

#[test]
fn without_a_marker_every_daily_note_is_pending_and_nothing_else_is() {
    let vault = Vault::new("daily-no-marker");
    vault.stage("daily/2026-10-24.md", "# a\n");
    vault.stage("daily/2026-10-25.md", "# b\n");
    vault.stage("daily/2026-10-24.png", "png");
    vault.stage("daily/notes/2026-10-23.md", "# nested\n");
    vault.stage("daily/loose.md", "# undated\n");
    vault.stage("other/2026-10-26.md", "# elsewhere\n");
    assert_eq!(
        vault.bikeshed().pending_daily_notes().unwrap().value,
        vec![
            "daily/2026-10-24.md".to_string(),
            "daily/2026-10-25.md".to_string()
        ]
    );
}

#[test]
fn a_note_dated_before_the_marker_stays_folded_when_its_mtime_moves() {
    let vault = Vault::new("daily-mtime");
    vault.stage("daily/2026-10-20.md", "# old\n");
    vault.stage("daily/2026-10-27.md", "# new\n");
    vault.stage("Index.md", "Human notes folded through 2026-10-21\n");
    vault.set_modified(
        "daily/2026-10-20.md",
        SystemTime::now() - Duration::from_secs(1),
    );
    assert_eq!(
        vault.bikeshed().pending_daily_notes().unwrap().value,
        vec!["daily/2026-10-27.md".to_string()]
    );
}

#[test]
fn reading_a_pass_inputs_writes_nothing_and_one_advance_leaves_nothing_pending() {
    let vault = Vault::new("daily-pass");
    vault.stage(
        "daily/2026-10-26.md",
        "# Today\n\nthe forum breaks on Safari\n",
    );
    vault.stage(
        "Index.md",
        "# Index\n\nHuman notes folded through 2026-10-25\n",
    );
    let bikeshed = vault.bikeshed();
    let before = vault.read("daily/2026-10-26.md");
    let index_before = vault.read("Index.md");

    let marker = DailyNote::folded_through(&index_before).unwrap_or_default();
    let pending = bikeshed.pending_daily_notes().unwrap().value;
    assert_eq!(marker, "2026-10-25");
    assert_eq!(pending, vec!["daily/2026-10-26.md".to_string()]);
    let folded = vault.read("daily/2026-10-26.md");
    vault.stage(
        "projects/tandem/tandem.md",
        &format!("# Tandem\n\nwhat the day held: {folded}"),
    );
    vault.stage(
        "Index.md",
        &index_before.replace(
            "Human notes folded through 2026-10-25",
            "Human notes folded through 2026-10-26",
        ),
    );

    assert_eq!(vault.read("daily/2026-10-26.md"), before);
    assert!(bikeshed.pending_daily_notes().unwrap().value.is_empty());
    let index_after = vault.read("Index.md");
    assert_eq!(
        index_after
            .split("Human notes folded through 2026-10-26")
            .count(),
        2,
        "the marker names one day, once"
    );
}

#[test]
fn a_day_parses_and_renders_zero_padded() {
    assert_eq!(Day::parse("2026-01-05"), Some(day(2026, 1, 5)));
    assert_eq!(Day::parse("2026-1-5"), None);
    assert_eq!(Day::parse("soon"), None);
    assert_eq!(day(2026, 1, 5).text(), "2026-01-05");
}
