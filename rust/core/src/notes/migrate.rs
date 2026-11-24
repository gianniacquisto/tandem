//! The one-time move of an older home's `bikeshed/` folder to `notes/`
//! (ADR 0055).
//!
//! [`migrate_home`] is the single door: serve start, the MCP methods and the CLI's
//! note verbs call it before they open the notes, and nothing else does.
//! `doctor` only reads (ADR 0014), so it asks [`pending`] instead, and `import`
//! refuses a non-empty home.
//!
//! The move is one `rename(2)` under both sidecar locks, so it is atomic on one
//! filesystem, and a second run finds no `bikeshed/` and returns at once. It
//! never merges: when both folders hold notes, or the old one is a link or a
//! file, it refuses and changes nothing.

use std::fs;
use std::io;

use super::{LockedDirectory, is_link, translate_store};
use crate::domain::error::{DomainError, DomainResult};
use crate::store::Home;
use crate::wire::slashed_path;

/// What an older folder means for this home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    /// No `bikeshed/`: nothing to move.
    Nothing,
    /// `bikeshed/` is a folder and `notes/` is absent or empty: the next open
    /// moves it.
    Move,
    /// Both hold notes, or `bikeshed/` is a link or a file: the next open refuses.
    Clash,
}

/// Read the home and say what the next open would do. Touches nothing.
pub fn pending(home: &Home) -> Pending {
    let Ok(legacy) = fs::symlink_metadata(home.legacy_notes_directory()) else {
        return Pending::Nothing;
    };
    if is_link(&legacy) || !legacy.is_dir() {
        return Pending::Clash;
    }
    if current_has_entries(home) {
        Pending::Clash
    } else {
        Pending::Move
    }
}

/// True when `notes/` is there and is not an empty folder. A link or a file
/// at that name counts: it is not ours to remove.
fn current_has_entries(home: &Home) -> bool {
    let current = home.notes_directory();
    match fs::symlink_metadata(&current) {
        Err(_) => false,
        Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => true,
        Ok(_) => fs::read_dir(&current)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true),
    }
}

/// Move `bikeshed/` to `notes/` if the home still has it. Idempotent.
pub fn migrate_home(home: &Home) -> DomainResult<()> {
    let clash = || DomainError::NotesFolderClash {
        legacy: slashed_path(&home.legacy_notes_directory()),
        current: slashed_path(&home.notes_directory()),
    };
    // A refusal leaves nothing behind, not even a lock file.
    match pending(home) {
        Pending::Nothing => return Ok(()),
        Pending::Clash => return Err(clash()),
        Pending::Move => {}
    }
    let io_error = |what: &str, error: io::Error| {
        DomainError::NotesIo(format!("could not {what} while moving the notes: {error}"))
    };

    // Both sidecars, the old one first: an older `tandem serve` still running
    // writes under `bikeshed.lock`, a current one under `notes.lock`.
    let _legacy_guard =
        LockedDirectory::for_notes(home.legacy_notes_directory(), home.legacy_notes_lock())
            .lock()
            .map_err(translate_store)?;
    let _current_guard = LockedDirectory::for_notes(home.notes_directory(), home.notes_lock())
        .lock()
        .map_err(translate_store)?;

    // Decide again under the locks: another process may have moved it.
    match pending(home) {
        Pending::Nothing => return Ok(()),
        Pending::Clash => return Err(clash()),
        Pending::Move => {}
    }
    let current = home.notes_directory();
    if fs::symlink_metadata(&current).is_ok() {
        fs::remove_dir(&current)
            .map_err(|error| io_error("clear the empty notes folder", error))?;
    }
    fs::rename(home.legacy_notes_directory(), &current)
        .map_err(|error| io_error("rename the folder", error))?;
    // The old sidecar is an empty file with no owner left to take it. A holder
    // that still has it open keeps its handle, so the removal is best effort.
    let _ = fs::remove_file(home.legacy_notes_lock());
    Ok(())
}
