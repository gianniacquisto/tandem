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
use super::{NodeKind, Notes, NotesNode};

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
    /// The folder, notes-root-relative, in the vault's own spelling.
    pub path: String,
    /// `no project` or `no label`.
    pub reason: String,
}

/// A board project or label that has no folder in the vault (TAN-531): the
/// folder `projects/<project>/` or `projects/<project>/<label>/` it should have,
/// or the path it would have had when its name is not a single path component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissingFolder {
    pub project: String,
    pub label: Option<String>,
    /// The folder the board name maps to, notes-root-relative.
    pub path: String,
}

/// The vault's link-graph health:
/// `{"broken": [...], "orphans": [...], "unmatchedFolders": [...], "missingFolders": [...], "counts": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotesHealth {
    /// Every wikilink that resolves to no note, or to more than one.
    pub broken: Vec<BrokenLink>,
    /// The notes with no resolved link in or out that are not filed under a
    /// matched project folder.
    pub orphans: Vec<String>,
    /// The project and label folders that match nothing on the board.
    #[serde(rename = "unmatchedFolders")]
    pub unmatched_folders: Vec<UnmatchedFolder>,
    /// The board's projects and labels that have no folder.
    #[serde(rename = "missingFolders")]
    pub missing_folders: Vec<MissingFolder>,
    pub counts: NotesHealthCounts,
}

impl NotesHealth {
    /// The empty report: a vault with nothing in it.
    pub fn empty() -> Self {
        Self {
            broken: Vec::new(),
            orphans: Vec::new(),
            unmatched_folders: Vec::new(),
            missing_folders: Vec::new(),
            counts: NotesHealthCounts {
                notes: 0,
                links: 0,
                broken: 0,
                orphans: 0,
                unmatched_folders: 0,
                missing_folders: 0,
            },
        }
    }
}

/// The totals of a health readout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotesHealthCounts {
    pub notes: usize,
    pub links: usize,
    pub broken: usize,
    pub orphans: usize,
    #[serde(rename = "unmatchedFolders")]
    pub unmatched_folders: usize,
    #[serde(rename = "missingFolders")]
    pub missing_folders: usize,
}

impl Notes {
    /// `health(names)`: the vault's link-graph health. The board's names come in
    /// as a value; the vault never opens the store.
    pub fn health(&self, names: &BoardNames) -> DomainResult<Operation<NotesHealth>> {
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
            .filter(|note| is_orphan(note, &linked, names))
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
        let missing_folders = missing_folders(&tree, names);
        let counts = NotesHealthCounts {
            notes: notes.len(),
            links: link_count,
            broken: broken.len(),
            orphans: orphans.len(),
            unmatched_folders: unmatched_folders.len(),
            missing_folders: missing_folders.len(),
        };
        Ok(Operation {
            value: NotesHealth {
                broken,
                orphans,
                unmatched_folders,
                missing_folders,
                counts,
            },
            note: None,
        })
    }
}

/// The orphan rule (ADR 0054): a note with no resolved link in or out that is
/// not filed under a matched project folder. `note_health` and the graph read
/// call this one function.
pub(super) fn is_orphan(
    path: &str,
    linked: &std::collections::HashSet<String>,
    names: &BoardNames,
) -> bool {
    !linked.contains(path) && !is_filed_under_project(path, names)
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
pub(super) fn unmatched_folders(tree: &[NotesNode], names: &BoardNames) -> Vec<UnmatchedFolder> {
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

/// The folder a project maps to: `projects/<project>`, or `None` when the name
/// is not a single path component (a `/`, `..`, a leading dot).
pub fn project_folder(project: &str) -> Option<String> {
    is_component(project).then(|| format!("projects/{project}"))
}

/// The folder a label maps to inside its project: `projects/<project>/<label>`,
/// `None` when either name is not a single path component, and `None` too when
/// the label is named like its project (that label is the project folder).
pub fn label_folder(project: &str, label: &str) -> Option<String> {
    if label.to_lowercase() == project.to_lowercase() {
        return None;
    }
    (is_component(project) && is_component(label)).then(|| format!("projects/{project}/{label}"))
}

/// A name that is one legal path component: it holds no separator and is not
/// `.`, `..` or a hidden name. The path guard of ADR 0008 still judges the
/// folder it would build.
pub fn is_component(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name == name.trim_matches(|c: char| c.is_whitespace())
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '#', '|', '\0'])
        && !name.chars().any(char::is_control)
}

/// The board's projects and labels with no folder: a project with no
/// `projects/<project>` (matched ignoring case), a label with no folder under
/// it. A name that is not a single component has no folder and is listed.
fn missing_folders(tree: &[NotesNode], names: &BoardNames) -> Vec<MissingFolder> {
    let projects = tree
        .iter()
        .find(|node| node.kind == NodeKind::Folder && node.name.eq_ignore_ascii_case("projects"));
    let child = |parent: Option<&NotesNode>, name: &str| -> Option<NotesNode> {
        parent?
            .children
            .iter()
            .find(|node| {
                node.kind == NodeKind::Folder && node.name.to_lowercase() == name.to_lowercase()
            })
            .cloned()
    };
    let mut missing = Vec::new();
    for project in &names.projects {
        let folder = if is_component(&project.name) {
            child(projects, &project.name)
        } else {
            None
        };
        if folder.is_none() {
            missing.push(MissingFolder {
                project: project.name.clone(),
                label: None,
                path: format!("projects/{}", project.name),
            });
        }
        for label in &project.labels {
            if label.to_lowercase() == project.name.to_lowercase() {
                continue;
            }
            let present = is_component(label) && child(folder.as_ref(), label).is_some();
            if !present {
                missing.push(MissingFolder {
                    project: project.name.clone(),
                    label: Some(label.clone()),
                    path: format!("projects/{}/{}", project.name, label),
                });
            }
        }
    }
    missing.sort_by(|left, right| {
        (left.path.to_lowercase(), left.path.clone())
            .cmp(&(right.path.to_lowercase(), right.path.clone()))
    });
    missing
}
