//! The join between the board and the vault (ADR 0054, TAN-531): the folders
//! `projects/<project>/` and `projects/<project>/<label>/` follow the board.
//!
//! The board write commits first; every step here runs after it, and a step
//! that fails never undoes the write. It becomes a note on the answer, and
//! `note_health` shows the leftovers as unmatched folders, missing folders and
//! broken typed links. Nothing here deletes a note or a folder.

use crate::bikeshed::health::is_component;
use crate::bikeshed::{Bikeshed, BikeshedNode, BoardNames, BoardProject, NodeKind};
use crate::domain::Board;
use crate::store::Store;

use super::open_bikeshed;

/// One board name the vault could not follow, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Leftover {
    project: String,
    label: Option<String>,
    reason: String,
}

impl Leftover {
    fn text(&self) -> String {
        match &self.label {
            Some(label) => format!(
                "no folder was made for label \"{label}\" of project \"{}\": {}",
                self.project, self.reason
            ),
            None => format!(
                "no folder was made for project \"{}\": {}",
                self.project, self.reason
            ),
        }
    }
}

/// The board's names as the vault reads them. An unreadable board is the empty
/// set: the board answer has its own error path, and the vault follows nothing.
pub(crate) fn names(board: &Board) -> BoardNames {
    match board.projects_snapshot() {
        Ok((projects, _)) => BoardNames {
            projects: projects
                .into_iter()
                .map(|entry| BoardProject {
                    name: entry.name,
                    labels: entry.labels,
                })
                .collect(),
        },
        Err(_) => BoardNames::default(),
    }
}

/// The project a label sits in, as the board spells both.
fn project_of<'a>(names: &'a BoardNames, label: &str) -> Option<&'a str> {
    names
        .projects
        .iter()
        .find(|project| project.labels.iter().any(|known| known == label))
        .map(|project| project.name.as_str())
}

/// Create every folder the board names that the vault lacks. Idempotent; it
/// never moves, renames or deletes. A folder that matches ignoring case is
/// reused. What could not be made comes back as leftovers.
pub(super) fn sync(bikeshed: &Bikeshed, names: &BoardNames) -> Vec<Leftover> {
    let tree = bikeshed.tree();
    let projects_dir = child(&tree, "projects");
    let projects_name = projects_dir
        .map(|node| node.name.clone())
        .unwrap_or_else(|| "projects".to_string());
    let mut leftovers = Vec::new();
    for project in &names.projects {
        let mut attempt = |label: Option<&str>| {
            let outcome = ensure(bikeshed, projects_dir, &projects_name, project, label);
            if let Err(reason) = outcome {
                leftovers.push(Leftover {
                    project: project.name.clone(),
                    label: label.map(str::to_string),
                    reason,
                });
            }
        };
        attempt(None);
        for label in &project.labels {
            attempt(Some(label));
        }
    }
    leftovers
}

/// The folder of one project or label, created when no folder matches it
/// ignoring case.
fn ensure(
    bikeshed: &Bikeshed,
    projects_dir: Option<&BikeshedNode>,
    projects_name: &str,
    project: &BoardProject,
    label: Option<&str>,
) -> Result<(), String> {
    if let Some(label) = label
        && label.to_lowercase() == project.name.to_lowercase()
    {
        return Ok(());
    }
    for name in std::iter::once(project.name.as_str()).chain(label) {
        if !is_component(name) {
            return Err(format!(
                "\"{name}\" is not a single path component (no \"/\", no \"..\", no leading dot)"
            ));
        }
    }
    let project_dir = projects_dir.and_then(|dir| child(&dir.children, &project.name));
    let project_name = project_dir
        .map(|node| node.name.clone())
        .unwrap_or_else(|| project.name.clone());
    let path = match label {
        None => {
            if project_dir.is_some() {
                return Ok(());
            }
            format!("{projects_name}/{project_name}")
        }
        Some(label) => {
            if project_dir
                .and_then(|dir| child(&dir.children, label))
                .is_some()
            {
                return Ok(());
            }
            format!("{projects_name}/{project_name}/{label}")
        }
    };
    bikeshed
        .create_folder(&path)
        .map(|_| ())
        .map_err(|error| error.message())
}

/// The direct child folder whose name matches `name` ignoring case.
fn child<'a>(nodes: &'a [BikeshedNode], name: &str) -> Option<&'a BikeshedNode> {
    nodes.iter().find(|node| {
        node.kind == NodeKind::Folder && node.name.to_lowercase() == name.to_lowercase()
    })
}

/// The folder `projects/<project>[/<label>]` as the vault spells it, if any.
fn existing(bikeshed: &Bikeshed, project: &str, label: Option<&str>) -> Option<String> {
    let tree = bikeshed.tree();
    let projects = child(&tree, "projects")?;
    let project = child(&projects.children, project)?;
    match label {
        None => Some(project.path.clone()),
        Some(label) => child(&project.children, label).map(|node| node.path.clone()),
    }
}

/// The notes a run adds to its answer, in one line, or none.
fn join(parts: Vec<String>) -> Option<String> {
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// `project.add` / `label.add` / any write: create what is missing and report
/// what could not be made for the name this write named.
pub(crate) fn added(
    store: &Store,
    board: &Board,
    project: Option<&str>,
    label: Option<&str>,
) -> Option<String> {
    let after = names(board);
    let leftovers = sync(&open_bikeshed(store), &after);
    join(
        leftovers
            .iter()
            .filter(|found| match (label, project) {
                (Some(label), _) => found.label.as_deref() == Some(label),
                (None, Some(project)) => found.label.is_none() && found.project == project,
                (None, None) => false,
            })
            .map(Leftover::text)
            .collect(),
    )
}

/// `project.rename`: move `projects/<from>/` to `projects/<to>/`, rewrite the
/// typed links, then create what is missing.
pub(crate) fn project_renamed(
    store: &Store,
    board: &Board,
    from: &str,
    to: &str,
) -> Option<String> {
    let (from, to) = (from.trim(), to.trim());
    let bikeshed = open_bikeshed(store);
    let mut notes = Vec::new();
    if from != to {
        if let Some(old) = existing(&bikeshed, from, None) {
            let target = if !is_component(to) {
                Err(format!(
                    "\"{to}\" is not a single path component, so the folder has no new name"
                ))
            } else {
                let parent = old.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("");
                match existing(&bikeshed, to, None) {
                    Some(taken) if taken.to_lowercase() != old.to_lowercase() => {
                        Err(format!("\"{taken}\" already exists"))
                    }
                    _ => Ok(format!("{parent}/{to}")),
                }
            };
            match target {
                Ok(target) if target == old => {}
                Ok(target) => {
                    if let Err(error) = bikeshed.rename_folder(&old, &target) {
                        notes.push(format!(
                            "the folder \"{old}\" was not moved: {}",
                            error.message()
                        ));
                    }
                }
                Err(reason) => notes.push(format!("the folder \"{old}\" was not moved: {reason}")),
            }
        }
        if let Err(error) = bikeshed.rewrite_typed_links(true, from, to) {
            notes.push(format!(
                "the links [[project:{from}]] were not all rewritten: {}",
                error.message()
            ));
        }
    }
    let after = names(board);
    notes.extend(
        sync(&bikeshed, &after)
            .iter()
            .filter(|found| found.project == to && found.label.is_none())
            .map(Leftover::text),
    );
    join(notes)
}

/// `label.rename` (and a re-parent): move the label folder to its new name and
/// project, rewrite the typed links, then create what is missing.
pub(crate) fn label_renamed(
    store: &Store,
    board: &Board,
    before: &BoardNames,
    from: &str,
    to: &str,
) -> Option<String> {
    let (from, to) = (from.trim(), to.trim());
    let after = names(board);
    let old_project = project_of(before, from);
    let new_project = project_of(&after, to);
    let bikeshed = open_bikeshed(store);
    let mut notes = Vec::new();
    let moved = from != to || old_project != new_project;
    if moved
        && let (Some(old_project), Some(new_project)) = (old_project, new_project)
        && old_project.to_lowercase() != from.to_lowercase()
        && let Some(old) = existing(&bikeshed, old_project, Some(from))
    {
        let target = if new_project.to_lowercase() == to.to_lowercase() {
            Err("the label is named like its project, so it has no folder of its own".to_string())
        } else if !is_component(to) || !is_component(new_project) {
            Err(format!(
                "\"{to}\" is not a single path component, so the folder has no new name"
            ))
        } else {
            let tree = bikeshed.tree();
            let projects = child(&tree, "projects");
            let parent = match existing(&bikeshed, new_project, None) {
                Some(path) => path,
                None => format!(
                    "{}/{new_project}",
                    projects
                        .map(|node| node.name.as_str())
                        .unwrap_or("projects")
                ),
            };
            let target = format!("{parent}/{to}");
            match existing(&bikeshed, new_project, Some(to)) {
                Some(taken) if taken.to_lowercase() != old.to_lowercase() => {
                    Err(format!("\"{taken}\" already exists"))
                }
                _ => Ok(target),
            }
        };
        match target {
            Ok(target) if target == old => {}
            Ok(target) => {
                if let Err(error) = bikeshed.rename_folder(&old, &target) {
                    notes.push(format!(
                        "the folder \"{old}\" was not moved: {}",
                        error.message()
                    ));
                }
            }
            Err(reason) => notes.push(format!("the folder \"{old}\" was not moved: {reason}")),
        }
    }
    if from != to
        && let Err(error) = bikeshed.rewrite_typed_links(false, from, to)
    {
        notes.push(format!(
            "the links [[label:{from}]] were not all rewritten: {}",
            error.message()
        ));
    }
    notes.extend(
        sync(&bikeshed, &after)
            .iter()
            .filter(|found| found.label.as_deref() == Some(to))
            .map(Leftover::text),
    );
    join(notes)
}

/// Reconcile at `tandem serve` start: create every missing folder, move
/// nothing. A failure is logged, and `note_health` lists what is still missing.
pub fn reconcile(store: &Store, actor: &crate::store::Actor) {
    let board = Board::with_actor(store.clone(), actor.clone());
    for leftover in sync(&open_bikeshed(store), &names(&board)) {
        tracing::warn!(note = %leftover.text(), "a notes folder could not be created");
    }
}
