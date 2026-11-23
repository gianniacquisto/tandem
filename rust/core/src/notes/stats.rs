//! `note stats`: per-project notes health, the
//! curated-note count and the newest curated-note mtime, summed into totals.
//!
//! Read-only and lock-free: it reads the visible tree and writes nothing. The
//! capture layer that once lived under `.raw/` left the core with CUT-7, so this
//! read has no hidden subtree to count.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;
use crate::store::encode;

use super::{Notes, is_link, sort_key};

/// One project's notes health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesProjectStats {
    /// The project folder name.
    pub project: String,
    /// Curated notes: `.md` files under the project, hidden subtrees pruned.
    pub curated_notes: usize,
    /// The newest curated-note mtime, ISO-8601, or null.
    pub last_consolidated: Option<String>,
}

/// The notes-health readout for the whole vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotesStats {
    /// The vault root, absolute.
    pub notes: String,
    /// Deprecated: the old name of `notes`, the same value. It is removed in
    /// the next release (`docs/versioning.md`).
    pub bikeshed: String,
    /// One row per project folder, alphabetical.
    pub projects: Vec<NotesProjectStats>,
    pub totals: NotesStatsTotals,
}

/// The vault-wide sums. A timestamp does not sum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesStatsTotals {
    pub curated_notes: usize,
}

impl Notes {
    /// `stats(project)`: per-project notes health, narrowed to one folder name
    /// when given. A missing vault, a missing `projects/`, or a name matching no
    /// folder is empty, never an error.
    pub fn stats(&self, project: Option<&str>) -> DomainResult<Operation<NotesStats>> {
        let rows: Vec<NotesProjectStats> = self
            .project_folders()
            .into_iter()
            .filter(|(name, _)| project.is_none_or(|wanted| name == wanted))
            .map(|(name, url)| self.project_stats(&name, &url))
            .collect();
        let totals = NotesStatsTotals {
            curated_notes: rows.iter().map(|row| row.curated_notes).sum(),
        };
        Ok(Operation {
            value: NotesStats {
                notes: crate::wire::slashed_path(&self.directory),
                bikeshed: crate::wire::slashed_path(&self.directory),
                projects: rows,
                totals,
            },
            note: None,
        })
    }

    /// Every project folder: a visible directory directly under `projects/`,
    /// alphabetical. A missing `projects/` folder, or one that is a link, is an
    /// empty list: a linked root is skipped, never followed (ADR 0008).
    pub(crate) fn project_folders(&self) -> Vec<(String, PathBuf)> {
        let root = self.directory.join("projects");
        let Ok(root_metadata) = fs::symlink_metadata(&root) else {
            return Vec::new();
        };
        if is_link(&root_metadata) {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&root) else {
            return Vec::new();
        };
        let mut projects = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_dir() || is_link(&metadata) {
                continue;
            }
            projects.push((name, entry.path()));
        }
        projects.sort_by_key(|project| sort_key(&project.0));
        projects
    }

    fn project_stats(&self, name: &str, url: &Path) -> NotesProjectStats {
        let curated: Vec<PathBuf> = files(url, true, true)
            .into_iter()
            .filter(|file| {
                file.file_name()
                    .map(|name| name.to_string_lossy().to_lowercase().ends_with(".md"))
                    .unwrap_or(false)
            })
            .collect();
        let last = curated
            .iter()
            .filter_map(|file| modified_seconds(file))
            .max();

        NotesProjectStats {
            project: name.to_string(),
            curated_notes: curated.len(),
            last_consolidated: last.map(encode),
        }
    }
}

/// The regular files under `dir`, recursively when asked. A missing directory is
/// an empty list; hidden entries are skipped when `pruning_hidden` (a hidden
/// folder prunes its whole subtree).
pub(crate) fn files(under: &Path, recursive: bool, pruning_hidden: bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(under) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if pruning_hidden && name.starts_with('.') {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if is_link(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            if recursive {
                found.extend(files(&entry.path(), true, pruning_hidden));
            }
        } else {
            found.push(entry.path());
        }
    }
    found
}

/// A file's on-disk modification time, whole seconds since the epoch.
fn modified_seconds(file: &Path) -> Option<i64> {
    let modified = fs::metadata(file).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs() as i64)
}
