//! `<TandemHome>/agent-install.json`: every path the install made.
//!
//! Ported from `SetupInstall.Manifest`. It sits beside `agent/`, never inside
//! it, because the face is replaced wholesale and the record of what was
//! installed must outlive the replace. It records the links, the host
//! instruction edits and the hosts a run targeted, each edit's `text` the exact
//! bytes written, so removal never re-derives them.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Failure, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Manifest {
    // The fields are declared in sorted order, so the written file has the
    // Swift encoder's `.sortedKeys` shape and a diff is a real change.
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// The hosts a run targeted, independent of what it managed to write.
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub path: String,
    #[serde(default = "symlink")]
    pub kind: String,
    pub target: String,
    pub host: String,
}

fn symlink() -> String {
    "symlink".to_string()
}

impl Link {
    pub fn new(path: String, target: String, host: &str) -> Self {
        Self {
            path,
            kind: symlink(),
            target,
            host: host.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub file: String,
    /// `instructions-entry` or `claude-block`.
    pub kind: String,
    /// The exact text written, so removal never re-derives it.
    pub text: String,
    pub host: String,
}

impl Edit {
    pub fn new(file: String, kind: &str, text: String, host: &str) -> Self {
        Self {
            file,
            kind: kind.to_string(),
            text,
            host: host.to_string(),
        }
    }
}

/// The manifest at `<home>/agent-install.json`, or `None` when it is absent or
/// unreadable.
pub fn manifest(home: &Path) -> Option<Manifest> {
    let path = home.join("agent-install.json");
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the manifest atomically: a same-directory temp file, then `rename(2)`.
///
/// The bytes are stable for a stable manifest, so a second install that changes
/// nothing leaves the file byte-identical, which is what the idempotence test
/// reads. Keys are sorted (`serde_json` writes struct fields in declaration
/// order, and the declaration is alphabetical) so a diff is a real change.
pub fn write_manifest(manifest: &Manifest, url: &Path) -> Result<()> {
    let text =
        serde_json::to_string_pretty(manifest).map_err(|error| Failure::Io(error.to_string()))?;
    let parent = url.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".agent-install-{}.json", uuid::Uuid::new_v4()));
    fs::write(&temporary, text.as_bytes())?;
    fs::rename(&temporary, url)?;
    Ok(())
}

/// Record one link, keyed by path, in path order.
pub fn record_link(manifest: &mut Manifest, link: Link) {
    manifest.links.retain(|existing| existing.path != link.path);
    manifest.links.push(link);
    manifest
        .links
        .sort_by(|left, right| left.path.cmp(&right.path));
}

/// Record one edit, keyed by file and kind.
pub fn record_edit(manifest: &mut Manifest, edit: Edit) {
    manifest
        .edits
        .retain(|existing| !(existing.file == edit.file && existing.kind == edit.kind));
    manifest.edits.push(edit);
    manifest
        .edits
        .sort_by(|left, right| (&left.file, &left.kind).cmp(&(&right.file, &right.kind)));
}
