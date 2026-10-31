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
    let root = std::env::temp_dir().join(format!(
        "tandem-fixture-db-{}-{}",
        std::process::id(),
        frozen
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("db")
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
