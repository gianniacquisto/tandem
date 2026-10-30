//! The notes a session wrote (`BikeshedSessionNotes.swift`): every file under
//! `projects/*/.raw/**/<session>/**`, pending captures and consumed ones alike.
//!
//! Derived, not stored: the session id is the folder a capture landed in, and the
//! consolidation pass moves what it consumes to `.raw/.done/<session>/`, the same
//! folder name in the archive. The scan cannot drift from the files. A missing
//! vault, a missing `projects/`, or a session that wrote nothing is an empty
//! list, never an error.

use crate::domain::error::DomainResult;
use crate::domain::operation::Operation;

use super::Bikeshed;
use super::stats::files;

impl Bikeshed {
    /// `sessionNotes(session)`: the capture paths a session wrote, sorted.
    pub fn session_notes(&self, session: &str) -> DomainResult<Operation<Vec<String>>> {
        let mut found = Vec::new();
        for (project, url) in self.project_folders() {
            let raw = url.join(".raw");
            let root = raw.to_string_lossy().to_string();
            for file in files(&raw, true, false) {
                let path = file.to_string_lossy().to_string();
                let Some(relative) = path.strip_prefix(&format!("{root}/")) else {
                    continue;
                };
                let components: Vec<&str> = relative.split('/').collect();
                // The session folder is a directory: a file's own name is never
                // the session.
                if components.len() > 1 && components[..components.len() - 1].contains(&session) {
                    found.push(format!("projects/{project}/.raw/{relative}"));
                }
            }
        }
        found.sort();
        Ok(Operation {
            value: found,
            note: None,
        })
    }
}
