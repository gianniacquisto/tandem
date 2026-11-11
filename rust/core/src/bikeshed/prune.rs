//! `note prune`: sweep the empty folders the capture archive leaves behind.
//!
//! The consolidation pass consumes a capture with an `mv` into
//! `projects/<project>/.raw/.done/<session>/`, and a move leaves the source
//! session folder behind, empty. The layer's only cleanup rule is the empty
//! container: this sweep walks every project's `.raw` subtree bottom up and
//! removes each directory that is verifiably empty.
//!
//! **The guard closes on doubt.** Every candidate comes from a directory entry
//! under `.raw`, a symlink is never a directory here, emptiness is re-read
//! immediately before the removal, and `.raw` itself and any folder named
//! `.done` are never candidates. A file is never touched; a folder holding one
//! is left standing. A missing `.raw` is nothing to do, not an error.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::Serialize;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::operation::Operation;

use super::Bikeshed;

/// `note prune`'s report: how many empty folders the sweep removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BikeshedPrune {
    pub removed: usize,
}

impl Bikeshed {
    /// `prune()`: remove every empty directory under each project's `.raw`
    /// subtree, bottom up. `.raw` and `.done` themselves survive, and so does
    /// every file. Answers the number of folders removed, `0` when there is
    /// nothing to remove.
    pub fn prune(&self) -> DomainResult<Operation<BikeshedPrune>> {
        self.locked(|| {
            let mut removed = 0;
            for (_name, project) in self.project_folders() {
                removed += prune_empty_dirs(&project.join(".raw"))?;
            }
            Ok(Operation {
                value: BikeshedPrune { removed },
                note: None,
            })
        })
    }
}

/// Remove the empty directories under `dir`, bottom up, and answer the count.
/// `dir` itself is never removed, and neither is any child named `.done`: the
/// archive is a container the sweep keeps even when a pass empties it.
fn prune_empty_dirs(dir: &Path) -> DomainResult<usize> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(io_failure(dir, &error)),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(|error| io_failure(dir, &error))?;
        let path = entry.path();
        // `file_type` does not follow a symlink: a link is never a directory
        // this sweep removes.
        let file_type = entry
            .file_type()
            .map_err(|error| io_failure(&path, &error))?;
        if !file_type.is_dir() {
            continue;
        }
        removed += prune_empty_dirs(&path)?;
        if entry.file_name() == ".done" {
            continue;
        }
        // Emptiness is re-read right before the removal, so the verdict and the
        // act cannot be separated by a stale listing.
        if fs::read_dir(&path)
            .map_err(|error| io_failure(&path, &error))?
            .next()
            .is_none()
        {
            fs::remove_dir(&path).map_err(|error| io_failure(&path, &error))?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn io_failure(path: &Path, error: &std::io::Error) -> DomainError {
    DomainError::BikeshedIo(format!("could not prune {}: {error}", path.display()))
}
