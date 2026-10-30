//! `note stats` (`BikeshedStats.swift`): per-project memory health, the
//! curated-note count, the hidden saddlebag's pending/archived depth and
//! oldest-pending age, and the newest curated-note mtime, summed into totals.
//!
//! Read-only and lock-free: it reads the tree (the one command that deliberately
//! looks inside the hidden `.raw/` layer) and writes nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;
use crate::store::timestamp;

use super::{Bikeshed, sort_key};

/// One project's memory health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeshedProjectStats {
    /// The project folder name.
    pub project: String,
    /// Curated notes: `.md` files under the project, hidden subtrees pruned.
    pub curated_notes: usize,
    /// Pending captures: files under `.raw/`, excluding `.done/`.
    pub raw_pending: usize,
    /// Consumed captures: files under `.raw/.done/`.
    pub raw_archived: usize,
    /// The oldest pending capture's UTC stamp, ISO-8601, or null.
    pub oldest_pending: Option<String>,
    /// The newest curated-note mtime, ISO-8601, or null.
    pub last_consolidated: Option<String>,
}

/// The memory-health readout for the whole vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BikeshedStats {
    /// The vault root, absolute.
    pub bikeshed: String,
    /// One row per project folder, alphabetical.
    pub projects: Vec<BikeshedProjectStats>,
    pub totals: BikeshedStatsTotals,
}

/// The vault-wide sums. A timestamp does not sum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeshedStatsTotals {
    pub curated_notes: usize,
    pub raw_pending: usize,
    pub raw_archived: usize,
}

impl Bikeshed {
    /// `stats(project)`: per-project memory health, narrowed to one folder name
    /// when given. A missing vault, a missing `projects/`, or a name matching no
    /// folder is empty, never an error.
    pub fn stats(&self, project: Option<&str>) -> DomainResult<Operation<BikeshedStats>> {
        let rows: Vec<BikeshedProjectStats> = self
            .project_folders()
            .into_iter()
            .filter(|(name, _)| project.is_none_or(|wanted| name == wanted))
            .map(|(name, url)| self.project_stats(&name, &url))
            .collect();
        let totals = BikeshedStatsTotals {
            curated_notes: rows.iter().map(|row| row.curated_notes).sum(),
            raw_pending: rows.iter().map(|row| row.raw_pending).sum(),
            raw_archived: rows.iter().map(|row| row.raw_archived).sum(),
        };
        Ok(Operation {
            value: BikeshedStats {
                bikeshed: crate::wire::slashed_path(&self.directory),
                projects: rows,
                totals,
            },
            note: None,
        })
    }

    /// Every project folder: a visible directory directly under `projects/`,
    /// alphabetical. A missing `projects/` folder is an empty list.
    pub(crate) fn project_folders(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = fs::read_dir(self.directory.join("projects")) else {
            return Vec::new();
        };
        let mut projects = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(metadata) = fs::metadata(entry.path()) else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            projects.push((name, entry.path()));
        }
        projects.sort_by_key(|project| sort_key(&project.0));
        projects
    }

    fn project_stats(&self, name: &str, url: &Path) -> BikeshedProjectStats {
        let curated: Vec<PathBuf> = files(url, true, true)
            .into_iter()
            .filter(|file| {
                file.file_name()
                    .map(|name| name.to_string_lossy().to_lowercase().ends_with(".md"))
                    .unwrap_or(false)
            })
            .collect();
        let raw = url.join(".raw");
        let pending = files(&raw, true, true);
        let archived = files(&raw.join(".done"), true, false);

        let oldest = pending
            .iter()
            .filter_map(|file| capture_timestamp(file).or_else(|| modified_seconds(file)))
            .min();
        let last = curated
            .iter()
            .filter_map(|file| modified_seconds(file))
            .max();

        BikeshedProjectStats {
            project: name.to_string(),
            curated_notes: curated.len(),
            raw_pending: pending.len(),
            raw_archived: archived.len(),
            oldest_pending: oldest.map(timestamp::encode),
            last_consolidated: last.map(timestamp::encode),
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
        let Ok(metadata) = fs::metadata(entry.path()) else {
            continue;
        };
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

/// A capture file's own UTC stamp, decoded from its leading `<utc>` name.
fn capture_timestamp(file: &Path) -> Option<i64> {
    let name = file.file_name()?.to_string_lossy();
    let stamp = name.get(..16)?;
    let iso = format!(
        "{}-{}-{}T{}:{}:{}Z",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13],
        &stamp[13..15]
    );
    timestamp::decode(&iso)
}

/// A file's on-disk modification time, whole seconds since the epoch.
fn modified_seconds(file: &Path) -> Option<i64> {
    let modified = fs::metadata(file).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs() as i64)
}
