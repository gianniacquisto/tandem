//! The vault as a graph of typed nodes and edges (ADR 0054), the one answer the
//! notes graph view draws. Nodes are `project`, `label`, `folder` and `note`;
//! edges are `contains` and `links`.
//!
//! One walk of the tree and one read of each note's links, through the same
//! folder matching, typed-link resolver and orphan rule `note_health` uses. The
//! bikeshed stays store-free: the board's names come in as a value.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::domain::error::{DomainError, DomainResult};

use super::health::{is_orphan, unmatched_folders};
use super::wikilink::{BoardNames, LinkResolution, link_occurrences};
use super::{Bikeshed, BikeshedNode, NodeKind};

/// The most nodes one answer carries.
pub const GRAPH_CAP: usize = 1000;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphNodeKind {
    Project,
    Label,
    Folder,
    Note,
}

impl GraphNodeKind {
    /// The word the wire carries.
    pub fn word(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Label => "label",
            Self::Folder => "folder",
            Self::Note => "note",
        }
    }
}

/// What an edge means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraphEdgeKind {
    Contains,
    Links,
}

impl GraphEdgeKind {
    pub fn word(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Links => "links",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteGraphNode {
    /// The folder or note path; `project:<Name>` or `label:<name>` for a typed
    /// link's target whose folder is missing.
    pub id: String,
    pub kind: GraphNodeKind,
    pub name: String,
    pub path: Option<String>,
    pub orphan: bool,
    pub daily: bool,
    /// `no project` or `no label` for a folder `note_health` lists.
    pub unmatched: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteGraphEdge {
    pub source: String,
    pub target: String,
    pub kind: GraphEdgeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteGraph {
    pub nodes: Vec<NoteGraphNode>,
    pub edges: Vec<NoteGraphEdge>,
    /// The nodes before the cap.
    pub total: usize,
    /// The nodes in this answer.
    pub shown: usize,
    pub cap: usize,
}

/// The notes face's project focus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphScope {
    pub project: Option<String>,
}

impl Bikeshed {
    /// `graph(names, scope)`: the vault, or one project's slice of it, capped
    /// at [`GRAPH_CAP`] nodes.
    pub fn graph(&self, names: &BoardNames, scope: &GraphScope) -> DomainResult<NoteGraph> {
        self.graph_capped(names, scope, GRAPH_CAP)
    }

    /// `graph` with the cap named, so a test needs no 1000-note vault.
    pub fn graph_capped(
        &self,
        names: &BoardNames,
        scope: &GraphScope,
        cap: usize,
    ) -> DomainResult<NoteGraph> {
        let focus = match scope.project.as_deref() {
            None => None,
            Some(name) => {
                Some(
                    names
                        .project(name)
                        .ok_or_else(|| DomainError::UnknownProjectName {
                            name: name.to_string(),
                            projects: names.projects.iter().map(|p| p.name.clone()).collect(),
                        })?,
                )
            }
        };
        let tree = self.nodes(&self.directory, "");
        let mut note_paths = Vec::new();
        super::collect_notes(&tree, &mut note_paths);
        let unmatched: HashMap<String, String> = unmatched_folders(&tree, names)
            .into_iter()
            .map(|folder| (folder.path, folder.reason))
            .collect();

        let mut nodes: HashMap<String, NoteGraphNode> = HashMap::new();
        let mut edges: BTreeSet<(String, String, GraphEdgeKind)> = BTreeSet::new();
        // The board's name of a project or label, to the node that carries it.
        let mut project_ids: HashMap<String, String> = HashMap::new();
        let mut label_ids: HashMap<String, String> = HashMap::new();
        let mut parents: HashMap<String, String> = HashMap::new();
        walk(
            &tree,
            None,
            names,
            &unmatched,
            &mut nodes,
            &mut edges,
            &mut project_ids,
            &mut label_ids,
            &mut parents,
        );

        // The links, once per note.
        let mut linked: HashSet<String> = HashSet::new();
        let mut link_edges: BTreeSet<(String, String)> = BTreeSet::new();
        for path in &note_paths {
            let Ok(content) = self.read_note(path) else {
                continue;
            };
            for occurrence in link_occurrences(&content.text, &note_paths, names) {
                let target = match occurrence.resolution {
                    LinkResolution::Resolved(target) => {
                        linked.insert(path.clone());
                        linked.insert(target.clone());
                        target
                    }
                    LinkResolution::Project(name) => {
                        linked.insert(path.clone());
                        let id = project_ids
                            .entry(name.to_lowercase())
                            .or_insert_with(|| format!("project:{name}"))
                            .clone();
                        nodes
                            .entry(id.clone())
                            .or_insert_with(|| typed_node(&id, GraphNodeKind::Project, &name));
                        id
                    }
                    LinkResolution::Label(name) => {
                        linked.insert(path.clone());
                        let id = label_ids
                            .entry(name.to_lowercase())
                            .or_insert_with(|| format!("label:{name}"))
                            .clone();
                        nodes
                            .entry(id.clone())
                            .or_insert_with(|| typed_node(&id, GraphNodeKind::Label, &name));
                        id
                    }
                    LinkResolution::Broken => continue,
                };
                if &target != path {
                    link_edges.insert((path.clone(), target));
                }
            }
        }
        for path in &note_paths {
            if let Some(node) = nodes.get_mut(path) {
                node.orphan = is_orphan(path, &linked, names);
            }
        }
        for (source, target) in link_edges {
            edges.insert((source, target, GraphEdgeKind::Links));
        }

        if let Some(project) = focus {
            let keep = scope_ids(
                project.name.as_str(),
                &project.labels,
                &nodes,
                &edges,
                &project_ids,
                &parents,
            );
            nodes.retain(|id, _| keep.contains(id));
            edges.retain(|(source, target, _)| keep.contains(source) && keep.contains(target));
            // A project with no folder and no link still has its own node.
            let id = project_ids
                .get(&project.name.to_lowercase())
                .cloned()
                .unwrap_or_else(|| format!("project:{}", project.name));
            nodes
                .entry(id.clone())
                .or_insert_with(|| typed_node(&id, GraphNodeKind::Project, &project.name));
        }

        let total = nodes.len();
        if total > cap {
            let mut degree: HashMap<&str, usize> = HashMap::new();
            for (source, target, kind) in &edges {
                if *kind == GraphEdgeKind::Links {
                    *degree.entry(source.as_str()).or_default() += 1;
                    *degree.entry(target.as_str()).or_default() += 1;
                }
            }
            let mut notes: Vec<&NoteGraphNode> = nodes
                .values()
                .filter(|node| node.kind == GraphNodeKind::Note)
                .collect();
            notes.sort_by(|left, right| {
                let left_degree = degree.get(left.id.as_str()).copied().unwrap_or(0);
                let right_degree = degree.get(right.id.as_str()).copied().unwrap_or(0);
                right_degree
                    .cmp(&left_degree)
                    .then_with(|| left.id.cmp(&right.id))
            });
            let structure = total - notes.len();
            let room = cap.saturating_sub(structure);
            let dropped: HashSet<String> = notes
                .iter()
                .skip(room)
                .map(|node| node.id.clone())
                .collect();
            nodes.retain(|id, _| !dropped.contains(id));
            edges.retain(|(source, target, _)| {
                !dropped.contains(source) && !dropped.contains(target)
            });
        }

        let mut nodes: Vec<NoteGraphNode> = nodes.into_values().collect();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        let edges = edges
            .into_iter()
            .map(|(source, target, kind)| NoteGraphEdge {
                source,
                target,
                kind,
            })
            .collect();
        Ok(NoteGraph {
            shown: nodes.len(),
            nodes,
            edges,
            total,
            cap,
        })
    }
}

/// A project or label a link names that no folder holds: a node with no path.
fn typed_node(id: &str, kind: GraphNodeKind, name: &str) -> NoteGraphNode {
    NoteGraphNode {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        path: None,
        orphan: false,
        daily: false,
        unmatched: None,
    }
}

/// The ids a project's slice keeps: its folder's subtree, every note with a
/// `links` edge into it and that note's direct parent folder.
fn scope_ids(
    project: &str,
    labels: &[String],
    nodes: &HashMap<String, NoteGraphNode>,
    edges: &BTreeSet<(String, String, GraphEdgeKind)>,
    project_ids: &HashMap<String, String>,
    parents: &HashMap<String, String>,
) -> HashSet<String> {
    let root = project_ids.get(&project.to_lowercase());
    let mut inside: HashSet<String> = HashSet::new();
    if let Some(root) = root {
        inside.insert(root.clone());
        let mut grew = true;
        while grew {
            grew = false;
            for (source, target, kind) in edges {
                if *kind == GraphEdgeKind::Contains
                    && inside.contains(source)
                    && inside.insert(target.clone())
                {
                    grew = true;
                }
            }
        }
    }
    // A label of this project a link names, with no folder to hang from.
    for node in nodes.values() {
        if node.kind == GraphNodeKind::Label
            && node.path.is_none()
            && labels
                .iter()
                .any(|label| label.to_lowercase() == node.name.to_lowercase())
        {
            inside.insert(node.id.clone());
        }
    }
    let mut keep = inside.clone();
    for (source, target, kind) in edges {
        if *kind == GraphEdgeKind::Links && inside.contains(target) && !inside.contains(source) {
            keep.insert(source.clone());
            if let Some(parent) = parents.get(source) {
                keep.insert(parent.clone());
            }
        }
    }
    keep
}

#[allow(clippy::too_many_arguments)]
fn walk(
    level: &[BikeshedNode],
    parent: Option<&BikeshedNode>,
    names: &BoardNames,
    unmatched: &HashMap<String, String>,
    nodes: &mut HashMap<String, NoteGraphNode>,
    edges: &mut BTreeSet<(String, String, GraphEdgeKind)>,
    project_ids: &mut HashMap<String, String>,
    label_ids: &mut HashMap<String, String>,
    parents: &mut HashMap<String, String>,
) {
    for node in level {
        if let Some(parent) = parent {
            edges.insert((
                parent.path.clone(),
                node.path.clone(),
                GraphEdgeKind::Contains,
            ));
            parents.insert(node.path.clone(), parent.path.clone());
        }
        match node.kind {
            NodeKind::Note => {
                let name = node.name.strip_suffix(".md").unwrap_or(&node.name);
                let daily = node.path.starts_with("daily/") && node.path.matches('/').count() == 1;
                nodes.insert(
                    node.path.clone(),
                    NoteGraphNode {
                        id: node.path.clone(),
                        kind: GraphNodeKind::Note,
                        name: name.to_string(),
                        path: Some(node.path.clone()),
                        orphan: false,
                        daily,
                        unmatched: None,
                    },
                );
            }
            NodeKind::Folder => {
                let (kind, name) = folder_kind(node, names);
                match kind {
                    GraphNodeKind::Project => {
                        project_ids.insert(name.to_lowercase(), node.path.clone());
                    }
                    GraphNodeKind::Label => {
                        label_ids.insert(name.to_lowercase(), node.path.clone());
                    }
                    _ => {}
                }
                nodes.insert(
                    node.path.clone(),
                    NoteGraphNode {
                        id: node.path.clone(),
                        kind,
                        name,
                        path: Some(node.path.clone()),
                        orphan: false,
                        daily: false,
                        unmatched: unmatched.get(&node.path).cloned(),
                    },
                );
                walk(
                    &node.children,
                    Some(node),
                    names,
                    unmatched,
                    nodes,
                    edges,
                    project_ids,
                    label_ids,
                    parents,
                );
            }
        }
    }
}

/// What a folder is: `projects/<p>` for a board project, `projects/<p>/<l>` for
/// one of its labels (not the label named like the project), else a folder.
fn folder_kind(node: &BikeshedNode, names: &BoardNames) -> (GraphNodeKind, String) {
    let parts: Vec<&str> = node.path.split('/').collect();
    if parts[0].eq_ignore_ascii_case("projects") {
        if parts.len() == 2
            && let Some(project) = names.project(parts[1])
        {
            return (GraphNodeKind::Project, project.name.clone());
        }
        if parts.len() == 3
            && let Some(project) = names.project(parts[1])
            && project.name.to_lowercase() != parts[2].to_lowercase()
            && let Some(label) = project
                .labels
                .iter()
                .find(|label| label.to_lowercase() == parts[2].to_lowercase())
        {
            return (GraphNodeKind::Label, label.clone());
        }
    }
    (GraphNodeKind::Folder, node.name.clone())
}
