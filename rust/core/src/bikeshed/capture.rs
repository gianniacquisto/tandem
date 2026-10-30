//! `note capture` (`BikeshedCapture.swift`, ADR 0016): one raw memory note per
//! moment, filed to the hidden per-project layer
//! `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`.
//!
//! The layer is out of the curated graph by construction: `list`, `note_paths`,
//! `search` and `resolve` all prune it. The filename is the invariant a plain
//! filesystem write cannot own: the UTC stamp orders the queue, the kind labels
//! it, and a same-second/same-slug collision gets a `-2`, `-3`, suffix rather
//! than overwriting a note.
//!
//! **The junk gate.** An empty body is refused, and so is a body whose first
//! non-empty line carries no words (the slug would fall back to the kind's own
//! name). Both throw before the lock, so a refused capture creates nothing.
//!
//! **The duplicate no-op.** A re-file with the same project, kind and
//! byte-identical body writes nothing and names the existing pending capture. The
//! archive is out of the scan, so a consumed capture may be filed again.

use std::fs;
use std::time::SystemTime;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::operation::Operation;
use crate::store::timestamp;

use super::Bikeshed;
use super::stats::files;

/// The four kinds of raw memory note `capture` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureKind {
    Decision,
    Gotcha,
    Fact,
    Thread,
}

impl CaptureKind {
    /// The kinds the wire names, in the order the error messages list them.
    pub const WIRE_VALUES: [&'static str; 4] = ["decision", "gotcha", "fact", "thread"];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Gotcha => "gotcha",
            Self::Fact => "fact",
            Self::Thread => "thread",
        }
    }

    /// Parse a `--kind` token. `work` is retired (the session summary is the work
    /// record), and anything else is unknown; both are named failures.
    pub fn parse(kind: &str) -> DomainResult<Self> {
        match kind {
            "decision" => Ok(Self::Decision),
            "gotcha" => Ok(Self::Gotcha),
            "fact" => Ok(Self::Fact),
            "thread" => Ok(Self::Thread),
            "work" => Err(DomainError::RetiredCaptureKind(kind.to_string())),
            other => Err(DomainError::InvalidCaptureKind(other.to_string())),
        }
    }
}

impl Bikeshed {
    /// `capture(project, kind, body, session, at)`: append one raw memory note and
    /// return its bikeshed-root-relative path.
    pub fn capture(
        &self,
        project: &str,
        kind: CaptureKind,
        body: &str,
        session: &str,
        at: SystemTime,
    ) -> DomainResult<Operation<String>> {
        validate_capture_component(project)?;
        validate_capture_component(session)?;
        if body.trim().is_empty() {
            return Err(DomainError::EmptyCaptureBody);
        }
        let slug = capture_slug(body);
        if slug.is_empty() {
            return Err(DomainError::EmptyCaptureSlug(kind.as_str().to_string()));
        }
        let folder = format!("projects/{project}/.raw/{session}");
        let base = format!("{}-{}-{}", utc_stamp(at), kind.as_str(), slug);
        self.locked(|| {
            if let Some(existing) = self.existing_pending_capture(project, kind, body) {
                return Ok(Operation {
                    value: existing.clone(),
                    note: Some(format!(
                        "An identical capture (same project, kind, and body) is already \
                         pending at \"{existing}\" — nothing new was filed."
                    )),
                });
            }
            let mut candidate = base.clone();
            let mut suffix = 2;
            loop {
                let relative = format!("{folder}/{candidate}.md");
                let Some(url) = self.file_url(&relative) else {
                    return Err(DomainError::InvalidCaptureName(relative));
                };
                if !url.exists() {
                    self.atomic_write(body.as_bytes(), &url)?;
                    return Ok(Operation {
                        value: relative,
                        note: None,
                    });
                }
                candidate = format!("{base}-{suffix}");
                suffix += 1;
            }
        })
    }

    /// The pending capture of `project` whose kind and body match, run inside the
    /// lock so the check and the write are one atomic step.
    fn existing_pending_capture(
        &self,
        project: &str,
        kind: CaptureKind,
        body: &str,
    ) -> Option<String> {
        let raw = self.directory.join(format!("projects/{project}/.raw"));
        let wanted = format!("{}-", kind.as_str());
        for url in files(&raw, true, true) {
            let name = url.file_name()?.to_string_lossy().to_string();
            if !name.to_lowercase().ends_with(".md") {
                continue;
            }
            let stem = &name[..name.len() - 3];
            if stem.len() < 17 {
                continue;
            }
            if !stem.get(17..).is_some_and(|rest| rest.starts_with(&wanted)) {
                continue;
            }
            let Ok(existing) = fs::read(&url) else {
                continue;
            };
            if existing != body.as_bytes() {
                continue;
            }
            return Some(self.relative_path(&url));
        }
        None
    }
}

/// A capture component is one visible folder name: no empty, `.` or `..`, no
/// leading dot, and no `/`, `#` or `|`.
fn validate_capture_component(value: &str) -> DomainResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.starts_with('.')
        || value.contains('/')
        || value.contains('#')
        || value.contains('|')
    {
        return Err(DomainError::InvalidCaptureName(value.to_string()));
    }
    Ok(())
}

/// The `<utc>` filename stamp, `yyyyMMdd'T'HHmmss'Z'`.
fn utc_stamp(at: SystemTime) -> String {
    let seconds = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let iso = timestamp::encode(seconds);
    // `YYYY-MM-DDTHH:MM:SSZ` → `YYYYMMDDTHHMMSSZ`.
    iso.chars()
        .filter(|character| *character != '-' && *character != ':')
        .collect()
}

/// The slug: the first non-empty line, a leading `#`/whitespace run stripped,
/// lowercased, every run of non-`[a-z0-9]` collapsed to one `-`, trimmed of `-`,
/// truncated to 40 characters.
fn capture_slug(body: &str) -> String {
    let first_line = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let stripped = first_line
        .trim_start_matches(|character: char| character == '#' || character.is_whitespace());
    let mut slug = String::new();
    let mut in_gap = false;
    for character in stripped.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if in_gap && !slug.is_empty() {
                slug.push('-');
            }
            in_gap = false;
            slug.push(character);
        } else {
            in_gap = true;
        }
    }
    slug.chars().take(40).collect()
}
