//! The `doctor` report (ADR 0014), cut to the board and the notes store
//! (ADR 0045).
//!
//! The gathering is impure, it stats the filesystem and probes the lock, but it
//! produces a [`Facts`] value and [`report`] is a pure function of that value.
//! So the report's shape, wording and pass/fail rule are covered with injected
//! facts, and the CLI only wires the real world to it.
//!
//! Three rows, in the pinned order: `home`, `store`, `bikeshed`. `home` is
//! always green; an absent store is a valid empty setup; a store this build
//! cannot read or a bikeshed path that is a file makes the CLI exit 1.

use std::path::PathBuf;

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::store::{BackupHealth, Health, Home, HomeSource, Store};
use crate::wire::{DoctorCheck, DoctorReport, slashed_path};

/// The check order, pinned: the CLI documents and tests it.
pub const CHECK_NAMES: [&str; 3] = ["home", "store", "bikeshed"];

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
        bikeshed_check(facts),
    ];

    let ok = checks.iter().all(|check| check.ok);
    DoctorReport { ok, checks }
}

/// Why the home resolved where it did, in the words `doctor` prints. The port
/// has one home (`~/.tandem`), so the Swift's shipped-app/dev-build split is
/// replaced by the flag and the default.
fn home_why(source: HomeSource) -> &'static str {
    match source {
        HomeSource::Flag => "--home",
        HomeSource::Environment => "TANDEM_HOME override",
        HomeSource::Default => "default",
    }
}

fn store_check(facts: &Facts) -> DoctorCheck {
    let backup = backup_detail(facts.store_backup);
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
