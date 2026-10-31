//! The vault's link-graph health (`BikeshedHealth.swift`): the wikilinks that
//! resolve to nothing, the notes nothing links to, and the totals. What `note
//! links` reports and what the app's Health panel lists, from one call.
//!
//! Read-only and lock-free: the file set is read once and every link resolved
//! against it with the same resolver the backlinks and the rename-rewrite use.
//! The hidden saddlebag is out of the graph, so a capture's unresolvable links
//! are never reported and a capture is never an orphan.

use serde::Serialize;
use ts_rs::TS;

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;

use super::Bikeshed;
use super::wikilink::{LinkResolution, line_number_of, link_occurrences};

/// One broken wikilink: the note that holds it, the target as written, and the
/// 1-based line it sits on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, TS)]
pub struct BrokenLink {
    pub note: String,
    pub link: String,
    pub line: usize,
}

/// The vault's link-graph health: `{"broken": [...], "orphans": [...], "counts": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BikeshedHealth {
    /// Every wikilink that resolves to no note, or to more than one.
    pub broken: Vec<BrokenLink>,
    /// The notes with no incoming wikilink, minus the entry points.
    pub orphans: Vec<String>,
    pub counts: BikeshedHealthCounts,
}

impl BikeshedHealth {
    /// The empty report: a vault with nothing in it.
    pub fn empty() -> Self {
        Self {
            broken: Vec::new(),
            orphans: Vec::new(),
            counts: BikeshedHealthCounts {
                notes: 0,
                links: 0,
                broken: 0,
                orphans: 0,
            },
        }
    }
}

/// The totals of a health readout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct BikeshedHealthCounts {
    pub notes: usize,
    pub links: usize,
    pub broken: usize,
    pub orphans: usize,
}

impl Bikeshed {
    /// `health()`: the vault's link-graph health.
    pub fn health(&self) -> DomainResult<Operation<BikeshedHealth>> {
        let notes = self.note_paths();
        let mut broken = Vec::new();
        let mut inbound = std::collections::HashSet::new();
        let mut link_count = 0;
        for path in &notes {
            let Ok(content) = self.read_note(path) else {
                continue;
            };
            let occurrences = link_occurrences(&content.text, &notes);
            link_count += occurrences.len();
            for occurrence in occurrences {
                match &occurrence.resolution {
                    LinkResolution::Broken => broken.push(BrokenLink {
                        note: path.clone(),
                        link: occurrence.link.target.clone(),
                        line: line_number_of(&content.text, occurrence.start),
                    }),
                    LinkResolution::Resolved(target) => {
                        inbound.insert(target.clone());
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
            .filter(|note| !inbound.contains(*note) && !is_anchor_note(note))
            .cloned()
            .collect();
        orphans.sort_by(|left, right| {
            (left.to_lowercase(), left.clone()).cmp(&(right.to_lowercase(), right.clone()))
        });
        let counts = BikeshedHealthCounts {
            notes: notes.len(),
            links: link_count,
            broken: broken.len(),
            orphans: orphans.len(),
        };
        Ok(Operation {
            value: BikeshedHealth {
                broken,
                orphans,
                counts,
            },
            note: None,
        })
    }
}

/// The entry points that are never orphans: the Hub (`Index.md`) and a project's
/// anchor note (`projects/<project>/<project>.md`).
pub(crate) fn is_anchor_note(path: &str) -> bool {
    if path.eq_ignore_ascii_case("index.md") {
        return true;
    }
    let components: Vec<&str> = path.split('/').collect();
    if components.len() != 3 || !components[0].eq_ignore_ascii_case("projects") {
        return false;
    }
    let project = components[1].to_lowercase();
    let name = components[2].to_lowercase();
    let name = name.strip_suffix(".md").unwrap_or(&name);
    name == project
}
