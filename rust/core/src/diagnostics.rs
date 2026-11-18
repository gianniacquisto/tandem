//! The `doctor` report (ADR 0014), cut to the board and the notes store
//! (ADR 0045).
//!
//! The gathering is impure, it stats the filesystem and probes the lock, but it
//! produces a [`Facts`] value and [`report`] is a pure function of that value.
//! So the report's shape, wording and pass/fail rule are covered with injected
//! facts, and the CLI only wires the real world to it.
//!
//! Seven rows, in the pinned order: `home`, `store`, `integrity`, `references`,
//! `space`, `versions`, `bikeshed`. `home` is always green; an absent store is
//! a valid empty setup; a store this build cannot read, a failed integrity or
//! references check or a bikeshed path that is a file makes the CLI exit 1.
//! Every failing row says what to do next, and prints paths, counts and ids
//! only, never ticket text.

use std::path::PathBuf;

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::store::{
    BackupHealth, CURRENT_VERSION, Health, Home, HomeSource, Store, StoreInspection,
};
use crate::wire::{DoctorCheck, DoctorReport, slashed_path};

/// The check order, pinned: the CLI documents and tests it.
pub const CHECK_NAMES: [&str; 7] = [
    "home",
    "store",
    "integrity",
    "references",
    "space",
    "versions",
    "bikeshed",
];

/// Free pages past this share of the file make `space` say so.
const FREE_PAGE_HINT_PERCENT: i64 = 20;

/// Raw observations from the machine, injected as a value so [`report`] stays
/// pure and deterministic under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub home_path: PathBuf,
    pub home_source: HomeSource,
    pub store_path: PathBuf,
    /// The applied schema-migration version, `None` when there is no database.
    pub store_version: Option<i64>,
    pub store_health: Health,
    pub store_backup: BackupHealth,
    /// The store's revision (0 when absent or unreadable):     /// second write should carry a backup (it is rolled before the second write).
    pub store_revision: i64,
    /// The deep read, `None` when there is no database.
    pub store_inspection: Option<StoreInspection>,
    /// The binary's release version.
    pub binary_version: String,
    pub bikeshed_path: PathBuf,
    pub bikeshed_note_count: usize,
    /// The bikeshed path exists but is a regular file, not a folder.
    pub bikeshed_is_file: bool,
}

/// Assemble the report from observed `facts`. Pure.
pub fn report(facts: &Facts) -> DoctorReport {
    let checks = vec![
        DoctorCheck {
            name: "home".to_string(),
            ok: true,
            detail: format!(
                "{} ({})",
                slashed_path(&facts.home_path),
                home_why(facts.home_source)
            ),
        },
        store_check(facts),
        integrity_check(facts),
        references_check(facts),
        space_check(facts),
        versions_check(facts),
        bikeshed_check(facts),
    ];

    let ok = checks.iter().all(|check| check.ok);
    DoctorReport { ok, checks }
}

/// Why the home resolved where it did, in the words `doctor` prints. There is one home (`~/.tandem`), so the only choices are the `--home` flag
/// and the default.
fn home_why(source: HomeSource) -> &'static str {
    match source {
        HomeSource::Flag => "--home",
        HomeSource::Environment => "TANDEM_HOME override",
        HomeSource::Default => "default",
    }
}

fn store_check(facts: &Facts) -> DoctorCheck {
    let backup = backup_detail(facts.store_backup);
    let backup_missing = facts.store_backup == BackupHealth::Absent && facts.store_revision > 2;
    let version = facts
        .store_version
        .map(|version| format!("v{version}; "))
        .unwrap_or_default();
    let path = slashed_path(&facts.store_path);

    match facts.store_health {
        Health::Absent => DoctorCheck {
            name: "store".to_string(),
            ok: true,
            detail: format!("{path} \u{2014} absent (a valid empty store); {backup}"),
        },
        Health::Counts { open, done } if backup_missing => DoctorCheck {
            name: "store".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} {version}{open} open, {done} done; {backup}, though the store is \
                 at revision {}. A write takes the backup; check that the home's folder is \
                 writable and that no other process holds the store, then write once",
                facts.store_revision
            ),
        },
        Health::Counts { open, done } => DoctorCheck {
            name: "store".to_string(),
            ok: true,
            detail: format!("{path} \u{2014} {version}{open} open, {done} done; {backup}"),
        },
        Health::Legacy { version } => DoctorCheck {
            name: "store".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} schema v{version}, which this build does not read; the one-off \
                 import of a v5 home is recorded in ADR 0031 and the git history (PRT-18); {backup}"
            ),
        },
        Health::Newer { store, build } => DoctorCheck {
            name: "store".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} schema v{store}; this build reads up to v{build}. The store was \
                 written by a newer Tandem. Upgrade this binary (run the installer again), and \
                 restart `tandem serve`; {backup}"
            ),
        },
        Health::Unreadable => DoctorCheck {
            name: "store".to_string(),
            ok: false,
            detail: format!(
                "{path} \u{2014} present but unreadable or corrupt; fix the file, or restore it \
                 from tandem.db.bak (remove tandem.db-wal and tandem.db-shm first); {backup}"
            ),
        },
    }
}

fn row(name: &str, ok: bool, detail: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        ok,
        detail,
    }
}

fn integrity_check(facts: &Facts) -> DoctorCheck {
    match &facts.store_inspection {
        None => row(
            "integrity",
            true,
            "no store yet, nothing to check".to_string(),
        ),
        Some(inspection) => match &inspection.integrity {
            Ok(()) => row("integrity", true, "quick_check ok".to_string()),
            Err(findings) => row(
                "integrity",
                false,
                format!(
                    "quick_check failed: {}. Stop `tandem serve`, then restore the store from \
                     tandem.db.bak (remove tandem.db-wal and tandem.db-shm first)",
                    findings.join("; ")
                ),
            ),
        },
    }
}

fn named(label: &str, count: i64, ids: &[String]) -> Option<String> {
    (count > 0).then(|| {
        if ids.is_empty() {
            format!("{count} {label}")
        } else {
            format!("{count} {label} (first: {})", ids.join(", "))
        }
    })
}

fn references_check(facts: &Facts) -> DoctorCheck {
    let Some(inspection) = &facts.store_inspection else {
        return row(
            "references",
            true,
            "no store yet, nothing to check".to_string(),
        );
    };
    let Some(refs) = &inspection.references else {
        return row(
            "references",
            true,
            "not checked: the store row above says why the tables cannot be read".to_string(),
        );
    };
    let found: Vec<String> = [
        named(
            "orphan comment(s)",
            refs.orphan_comments,
            &refs.comment_samples,
        ),
        named("orphan edge(s)", refs.orphan_edges, &refs.edge_samples),
        named(
            "ticket(s) naming a missing label",
            refs.missing_labels,
            &refs.label_samples,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    if found.is_empty() {
        return row(
            "references",
            true,
            "0 orphan comments, 0 orphan edges, 0 tickets naming a missing label".to_string(),
        );
    }
    row(
        "references",
        false,
        format!(
            "{}. Nothing repairs these on its own: export the home (`tandem export`), then \
             remove or re-create the named rows, and file a bug with the ids",
            found.join("; ")
        ),
    )
}

fn space_check(facts: &Facts) -> DoctorCheck {
    let Some(inspection) = &facts.store_inspection else {
        return row("space", true, "no store yet".to_string());
    };
    let wal = format!("WAL {} bytes", inspection.wal_bytes);
    let Some(pages) = inspection.pages else {
        return row("space", true, format!("pages unreadable; {wal}"));
    };
    let mut detail = format!(
        "{} page(s) of {} bytes, {} free; {wal}",
        pages.page_count, pages.page_size, pages.free_pages
    );
    if pages.page_count > 0 && pages.free_pages * 100 > pages.page_count * FREE_PAGE_HINT_PERCENT {
        detail.push_str(
            "; over 20 percent of the file is free pages, which `sqlite3 tandem.db VACUUM` \
             reclaims while `tandem serve` is stopped",
        );
    }
    row("space", true, detail)
}

fn versions_check(facts: &Facts) -> DoctorCheck {
    let store = match (&facts.store_health, facts.store_version) {
        (Health::Absent, _) => "no store yet".to_string(),
        (Health::Newer { store, .. } | Health::Legacy { version: store }, _) => {
            format!("store schema v{store}")
        }
        (_, Some(version)) => format!("store schema v{version}"),
        (_, None) => "store schema unknown".to_string(),
    };
    let detail = format!(
        "binary {}; this build writes schema v{CURRENT_VERSION}; {store}",
        facts.binary_version
    );
    match facts.store_health {
        Health::Newer { .. } => row(
            "versions",
            false,
            format!(
                "{detail}. The store is newer than this binary: upgrade it (run the installer again) and restart `tandem serve`"
            ),
        ),
        Health::Legacy { .. } => row(
            "versions",
            false,
            format!("{detail}. This build does not read that schema; see the store row above"),
        ),
        _ => row("versions", true, detail),
    }
}

fn bikeshed_check(facts: &Facts) -> DoctorCheck {
    let path = slashed_path(&facts.bikeshed_path);
    if facts.bikeshed_is_file {
        DoctorCheck {
            name: "bikeshed".to_string(),
            ok: false,
            detail: format!("{path} \u{2014} a file, not a folder; pick a folder"),
        }
    } else {
        DoctorCheck {
            name: "bikeshed".to_string(),
            ok: true,
            detail: format!("{path} \u{2014} {} note(s)", facts.bikeshed_note_count),
        }
    }
}

/// The rolling backup's clause for the store check: presence and age.
fn backup_detail(health: BackupHealth) -> String {
    match health {
        BackupHealth::Absent => "no backup yet".to_string(),
        BackupHealth::Present { age } => format!("backup {} old", age_text(age.as_secs_f64())),
    }
}

/// An age in the shortest unit that still reads as one: seconds, minutes,
/// hours, days. Coarse on purpose.
pub fn age_text(age: f64) -> String {
    let seconds = age.round().max(0.0) as u64;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3_600 {
        return format!("{}m", seconds / 60);
    }
    if seconds < 86_400 {
        return format!("{}h", seconds / 3_600);
    }
    format!("{}d", seconds / 86_400)
}

// MARK: - Gathering (impure)

/// Gather the real machine's facts, then assemble. The CLI calls this.
pub fn run(home: &Home) -> DoctorReport {
    report(&gather(home))
}

/// Gather the machine's facts. The only impure entry point besides [`run`].
pub fn gather(home: &Home) -> Facts {
    let store = Store::new(home.clone());
    let bikeshed = Bikeshed::new(home.directory().join("bikeshed"));
    let notes = bikeshed
        .list()
        .map(|operation| operation.value)
        .unwrap_or_default();
    let bikeshed_path = home.directory().join("bikeshed");
    let bikeshed_is_file = bikeshed_path.exists() && !bikeshed_path.is_dir();

    Facts {
        home_path: home.directory().to_path_buf(),
        home_source: home.source(),
        store_path: store.database(),
        store_version: store.schema_version().ok().flatten(),
        store_health: store.health(),
        store_backup: store.backup_health(),
        store_revision: store.revision().unwrap_or(0),
        store_inspection: store.inspect(),
        binary_version: crate::release_version().to_string(),
        bikeshed_path,
        bikeshed_note_count: note_count(&notes),
        bikeshed_is_file,
    }
}

fn note_count(nodes: &[BikeshedNode]) -> usize {
    nodes
        .iter()
        .map(|node| usize::from(node.kind == NodeKind::Note) + note_count(&node.children))
        .sum()
}
