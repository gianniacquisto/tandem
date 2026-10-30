//! The wiki front page (`BikeshedFront.swift`, 4E5E9A0B): one row per project
//! under `projects/`, each the project's Overview with its scaffold page set, its
//! "where this left off" line and its freshness stamp.
//!
//! Derived live from the file set; a missing `projects/` folder is an empty front
//! page, never an error. The matching rules follow the scaffold convention: the
//! Overview is the project folder's note whose name matches the folder's (case,
//! spaces and hyphens normalized away), the scaffold pages are
//! `<Overview stem> Decisions|Risks|Processes.md` in the fixed order, and group
//! sub-folders are neither rows nor pages. The hidden saddlebag is never read.

use std::fs;
use std::time::SystemTime;

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;

use super::{Bikeshed, is_note_name, sort_key, strip_md};

/// One scaffold page of a project's page set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldPage {
    pub role: ScaffoldRole,
    /// The note's bikeshed-root-relative path.
    pub path: String,
}

/// The four fixed roles, in the order the face draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldRole {
    Overview,
    Decisions,
    Risks,
    Processes,
}

impl ScaffoldRole {
    /// The chip label the face draws.
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Decisions => "Decisions",
            Self::Risks => "Risks",
            Self::Processes => "Processes",
        }
    }

    /// The page's word in a scaffold filename.
    pub fn raw(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Decisions => "decisions",
            Self::Risks => "risks",
            Self::Processes => "processes",
        }
    }
}

/// One project row of the front page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFront {
    /// The Overview's title line, `#`s stripped; the folder name when empty.
    pub name: String,
    /// The project folder, bikeshed-root-relative (`projects/tandem`).
    pub folder: String,
    /// The scaffold page set, Overview first, only the pages that exist.
    pub pages: Vec<ScaffoldPage>,
    /// The first non-empty line after `## Where left off`, when present.
    pub state_line: Option<String>,
    /// The Overview's on-disk mtime: the freshness stamp.
    pub freshness: Option<SystemTime>,
}

impl Bikeshed {
    /// `frontPage()`: one row per project folder, sorted by project name.
    pub fn front_page(&self) -> DomainResult<Operation<Vec<ProjectFront>>> {
        let projects = self.directory.join("projects");
        let Ok(entries) = fs::read_dir(&projects) else {
            return Ok(Operation {
                value: Vec::new(),
                note: None,
            });
        };
        let mut rows = Vec::new();
        for entry in entries.flatten() {
            let Ok(metadata) = fs::metadata(entry.path()) else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            let folder = format!("projects/{}", entry.file_name().to_string_lossy());
            if let Some(row) = self.project_front(&folder, &entry.path()) {
                rows.push(row);
            }
        }
        rows.sort_by_key(|row| sort_key(&row.name));
        Ok(Operation {
            value: rows,
            note: None,
        })
    }

    /// One project's row, or `None` when the folder has no Overview.
    fn project_front(&self, folder: &str, url: &std::path::Path) -> Option<ProjectFront> {
        let entries = fs::read_dir(url).ok()?;
        let mut note_names: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let Ok(metadata) = fs::metadata(entry.path()) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if !metadata.is_dir() && is_note_name(&name) {
                note_names.push(name);
            }
        }

        let folder_key = match_key(&url.file_name()?.to_string_lossy());
        let overview_name = note_names
            .iter()
            .filter(|name| match_key(strip_md(name)) == folder_key)
            .min_by(|left, right| sort_key(left).cmp(&sort_key(right)))?
            .clone();
        let overview_path = format!("{folder}/{overview_name}");
        let stem = strip_md(&overview_name);

        let mut pages = vec![ScaffoldPage {
            role: ScaffoldRole::Overview,
            path: overview_path.clone(),
        }];
        for role in [
            ScaffoldRole::Decisions,
            ScaffoldRole::Risks,
            ScaffoldRole::Processes,
        ] {
            let wanted = match_key(&format!("{stem} {}", role.raw()));
            if let Some(name) = note_names
                .iter()
                .find(|name| match_key(strip_md(name)) == wanted)
            {
                pages.push(ScaffoldPage {
                    role,
                    path: format!("{folder}/{name}"),
                });
            }
        }

        let content = self.read_note(&overview_path).ok();
        Some(ProjectFront {
            name: content
                .as_ref()
                .and_then(|content| title(&content.text))
                .unwrap_or_else(|| stem.to_string()),
            folder: folder.to_string(),
            pages,
            state_line: content
                .as_ref()
                .and_then(|content| where_left_off(&content.text)),
            freshness: content.as_ref().map(|content| content.modified_at),
        })
    }
}

/// The matching key: lowercase, spaces and hyphens removed.
fn match_key(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|character| *character != ' ' && *character != '-')
        .collect()
}

/// The note's title: the first line, leading `#`s stripped, whitespace trimmed.
fn title(text: &str) -> Option<String> {
    let first = text.split('\n').next().unwrap_or("");
    let trimmed = first.trim_start_matches('#').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// The first non-empty line after the `## Where left off` heading.
fn where_left_off(text: &str) -> Option<String> {
    let mut past = false;
    for line in text.split('\n') {
        let line = line.trim();
        if !past {
            if line == "## Where left off" {
                past = true;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        return Some(line.to_string());
    }
    None
}
