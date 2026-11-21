//! The vault's link-graph health: the wikilinks that
//! resolve to nothing, the notes that float free, the project and label folders
//! that match nothing on the board, and the totals (ADR 0054). What `note
//! links` reports and what the app's Health panel lists, from one call.
//!
//! Read-only and lock-free: the file set is read once and every link resolved
//! against it with the same resolver the backlinks and the rename-rewrite use.
//! The hidden saddlebag is out of the graph, so a capture's unresolvable links
//! are never reported and a capture is never an orphan.

use serde::Serialize;

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;

use super::wikilink::{BoardNames, LinkResolution, line_number_of, link_occurrences};
use super::{Bikeshed, BikeshedNode, NodeKind};

/// One broken wikilink: the note that holds it, the target as written, and the
/// 1-based line it sits on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct BrokenLink {
    pub note: String,
    pub link: String,
    pub line: usize,
}

/// A folder under `projects/` that matches no project or label on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmatchedFolder {
    /// The folder, bikeshed-root-relative, in the vault's own spelling.
    pub path: String,
    /// `no project` or `no label`.
    pub reason: String,
}

/// The vault's link-graph health:
/// `{"broken": [...], "orphans": [...], "unmatchedFolders": [...], "counts": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BikeshedHealth {
    /// Every wikilink that resolves to no note, or to more than one.
    pub broken: Vec<BrokenLink>,
    /// The notes with no resolved link in or out that are not filed under a
    /// matched project folder.
    pub orphans: Vec<String>,
    /// The project and label folders that match nothing on the board.
    #[serde(rename = "unmatchedFolders")]
    pub unmatched_folders: Vec<UnmatchedFolder>,
    pub counts: BikeshedHealthCounts,
}

impl BikeshedHealth {
    /// The empty report: a vault with nothing in it.
    pub fn empty() -> Self {
        Self {
            broken: Vec::new(),
            orphans: Vec::new(),
            unmatched_folders: Vec::new(),
            counts: BikeshedHealthCounts {
                notes: 0,
                links: 0,
                broken: 0,
                orphans: 0,
                unmatched_folders: 0,
            },
        }
    }
}

/// The totals of a health readout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BikeshedHealthCounts {
    pub notes: usize,
    pub links: usize,
    pub broken: usize,
    pub orphans: usize,
    #[serde(rename = "unmatchedFolders")]
    pub unmatched_folders: usize,
}

impl Bikeshed {
    /// `health(names)`: the vault's link-graph health. The board's names come in
    /// as a value; the vault never opens the store.
    pub fn health(&self, names: &BoardNames) -> DomainResult<Operation<BikeshedHealth>> {
        let tree = self.nodes(&self.directory, "");
        let mut notes = Vec::new();
        super::collect_notes(&tree, &mut notes);
        let mut broken = Vec::new();
        let mut linked = std::collections::HashSet::new();
        let mut link_count = 0;
        for path in &notes {
            let Ok(content) = self.read_note(path) else {
                continue;
            };
            let occurrences = link_occurrences(&content.text, &notes, names);
            link_count += occurrences.len();
            for occurrence in occurrences {
                match &occurrence.resolution {
                    LinkResolution::Broken => broken.push(BrokenLink {
                        note: path.clone(),
                        link: occurrence.link.target.clone(),
                        line: line_number_of(&content.text, occurrence.start),
                    }),
                    LinkResolution::Resolved(target) => {
                        linked.insert(path.clone());
                        linked.insert(target.clone());
                    }
                    LinkResolution::Project(_) | LinkResolution::Label(_) => {
                        linked.insert(path.clone());
                    }
                }
            }
        }
        broken.sort_by(|left, right| {
            (
                left.note.to_lowercase(),
                left.note.clone(),
                left.line,
                left.link.clone(),
            )
                .cmp(&(
                    right.note.to_lowercase(),
                    right.note.clone(),
                    right.line,
                    right.link.clone(),
                ))
        });
        let mut orphans: Vec<String> = notes
            .iter()
            .filter(|note| !linked.contains(*note) && !is_filed_under_project(note, names))
            .cloned()
            .collect();
        orphans.sort_by(|left, right| {
            (left.to_lowercase(), left.clone()).cmp(&(right.to_lowercase(), right.clone()))
        });
        let mut unmatched_folders = unmatched_folders(&tree, names);
        unmatched_folders.sort_by(|left, right| {
            (left.path.to_lowercase(), left.path.clone())
                .cmp(&(right.path.to_lowercase(), right.path.clone()))
        });
        let counts = BikeshedHealthCounts {
            notes: notes.len(),
            links: link_count,
            broken: broken.len(),
            orphans: orphans.len(),
            unmatched_folders: unmatched_folders.len(),
        };
        Ok(Operation {
            value: BikeshedHealth {
                broken,
                orphans,
                unmatched_folders,
                counts,
            },
            note: None,
        })
    }
}

/// A note filed anywhere under `projects/<p>/` where `<p>` is a board project,
/// matched case-insensitively. Filing is the project link a folder gives.
fn is_filed_under_project(path: &str, names: &BoardNames) -> bool {
    let components: Vec<&str> = path.split('/').collect();
    components.len() >= 3
        && components[0].eq_ignore_ascii_case("projects")
        && names.project(components[1]).is_some()
}

/// The direct children of `projects/` that match no project, and the direct
/// children of a matched project folder that match none of its labels. Deeper
/// folders are free.
fn unmatched_folders(tree: &[BikeshedNode], names: &BoardNames) -> Vec<UnmatchedFolder> {
    let mut found = Vec::new();
    let Some(projects) = tree
        .iter()
        .find(|node| node.kind == NodeKind::Folder && node.name.eq_ignore_ascii_case("projects"))
    else {
        return found;
    };
    for folder in projects
        .children
        .iter()
        .filter(|node| node.kind == NodeKind::Folder)
    {
        let Some(project) = names.project(&folder.name) else {
            found.push(UnmatchedFolder {
                path: folder.path.clone(),
                reason: "no project".to_string(),
            });
            continue;
        };
        for child in folder
            .children
            .iter()
            .filter(|node| node.kind == NodeKind::Folder)
        {
            let matched = project
                .labels
                .iter()
                .any(|label| label.to_lowercase() == child.name.to_lowercase());
            if !matched {
                found.push(UnmatchedFolder {
                    path: child.path.clone(),
                    reason: "no label".to_string(),
                });
            }
        }
    }
    found
}
