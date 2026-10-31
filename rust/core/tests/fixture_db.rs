//! The committed fixture stores (`Fixtures/port/Home/tandem.db` and
//! `Fixtures/port/Board/tandem.db`) are artifacts of their generator, byte for
//! byte.
//!
//! `Scripts/make-fixture-home.sh` writes each database from its seed
//! (`Fixtures/port/tandem.seed.json`, `Fixtures/port/board.seed.json`): every id
//! and timestamp is in the seed and nothing reads the clock, so the files in the
//! tree are reproducible. This suite runs the same generator into a temp path
//! and fails on any difference, which is what keeps a hand-edited database out
//! of the tree.
//!
//! The store is in rollback-journal mode, not WAL: a read of a WAL file would
//! need `-shm` and `-wal` sidecars beside it, and the fixture home is content,
//! not scratch. The second case reads the committed file through the store and
//! proves it leaves no sidecar behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tandem_core::domain::session::Sessions;
use tandem_core::domain::{Board, Filter};
use tandem_core::store::{Home, Store};

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // <repo>/rust/core
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("{} sits two levels below the repo root", manifest.display()))
        .to_path_buf()
}

fn seed() -> PathBuf {
    repo_root()
        .join("Fixtures")
        .join("port")
        .join("tandem.seed.json")
}

fn committed() -> PathBuf {
    repo_root()
        .join("Fixtures")
        .join("port")
        .join("Home")
        .join("tandem.db")
}

fn board_seed() -> PathBuf {
    repo_root()
        .join("Fixtures")
        .join("port")
        .join("board.seed.json")
}

fn board_committed() -> PathBuf {
    repo_root()
        .join("Fixtures")
        .join("port")
        .join("Board")
        .join("tandem.db")
}

fn board_home() -> PathBuf {
    repo_root().join("Fixtures").join("port").join("Board")
}

fn fixture_home() -> PathBuf {
    repo_root().join("Fixtures").join("port").join("Home")
}

/// The first byte the two files disagree on, or `None` when they agree.
fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    left.iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .or_else(|| (left.len() != right.len()).then(|| left.len().min(right.len())))
}

/// Run the generator on `seed` and fail naming the first byte that differs from
/// the committed `frozen` file.
fn assert_generated(seed: &Path, frozen: &Path) {
    // Both homes are named `tandem.db`, and two tests run in parallel in one
    // process, so the scratch root is named for the home and the seed, never
    // for the store's file name alone.
    let home = frozen
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("home");
    let seed_name = seed
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("seed");
    let root = std::env::temp_dir().join(format!(
        "tandem-fixture-db-{}-{home}-{seed_name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let target = root.join("tandem.db");

    let generator = std::env::var("CARGO_BIN_EXE_make-fixture-home")
        .expect("cargo builds the generator for this suite");
    let output = Command::new(generator)
        .arg(seed)
        .arg(&target)
        .output()
        .expect("the generator runs");
    assert!(
        output.status.success(),
        "the generator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = fs::read(&target).expect("the generator wrote the database");
    let committed_bytes = fs::read(frozen).unwrap_or_else(|error| {
        panic!(
            "{} is committed ({error}): run Scripts/make-fixture-home.sh",
            frozen.display()
        )
    });

    if let Some(offset) = first_difference(&committed_bytes, &written) {
        panic!(
            "{} and the generator disagree at byte {offset} ({} vs {} bytes): \
             the committed file is not the script's output, so run \
             Scripts/make-fixture-home.sh and commit what it writes",
            frozen.display(),
            committed_bytes.len(),
            written.len()
        );
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_committed_database_is_what_the_generator_writes() {
    assert_generated(&seed(), &committed());
}

#[test]
fn the_committed_board_database_is_what_the_generator_writes() {
    assert_generated(&board_seed(), &board_committed());
}

#[test]
fn the_board_home_holds_the_78_ticket_shape() {
    let store = Store::new(Home::resolve(
        Some(&board_home()),
        None,
        Path::new("/nonexistent-user-home"),
    ));
    let tickets = Board::new(store)
        .list_filter(Filter::All)
        .expect("the board home reads");

    assert_eq!(tickets.len(), 78, "the frozen board is 78 tickets");
    let lanes: std::collections::BTreeSet<Option<&str>> = tickets
        .iter()
        .map(|ticket| ticket.lane.as_deref())
        .collect();
    assert_eq!(lanes.len(), 5, "four named lanes and the default area");

    let edges: usize = tickets.iter().map(|ticket| ticket.blocked_by.len()).sum();
    assert_eq!(edges, 3, "the frozen board carries three dependency edges");
    assert_eq!(
        tickets.iter().filter(|ticket| ticket.blocked()).count(),
        3,
        "three open tickets carry the blocked mark the pictures show"
    );
}

#[test]
fn reading_the_fixture_home_leaves_it_untouched() {
    let home = fixture_home();
    let store = Store::new(Home::resolve(
        Some(&home),
        None,
        Path::new("/nonexistent-user-home"),
    ));

    let sessions = Sessions::new(&store)
        .list(None, None)
        .expect("the fixture home's records read");
    assert_eq!(sessions.len(), 4, "the frozen set is four records");

    for sidecar in [
        "tandem.db-wal",
        "tandem.db-shm",
        "tandem.db-journal",
        "tandem.db.bak",
    ] {
        assert!(
            !home.join(sidecar).exists(),
            "a read of the fixture home left {sidecar} behind: the store is \
             committed in rollback-journal mode so a reader needs no sidecar"
        );
    }
}
