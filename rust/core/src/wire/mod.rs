//! The wire: the JSON the agent face prints and the app reads.
//!
//! One owner. The CLI parses argv and prints these types; it shapes no message
//! of its own, so `--help` drift and wire drift can never be the same bug
//! once: the CLI parses argv and prints these types, and every face reads the
//! same shape, so `--help` drift and wire drift can never be the same bug twice.

mod views;

pub mod rpc;
use std::path::Path;

use serde::Serialize;

/// A path in the wire's spelling: `/`-separated on every platform (PRT-7).
///
/// The host's separator is a fact about the machine, not about the contract: a
/// Windows `\` in a path on stdout is a shape the app and the skills would have
/// to learn. The drive or root is kept, so an absolute path still reads as one.
pub fn slashed_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

pub use views::{
    ArchivedView, BacklinkView, CommentList, CommentResult, CommentView, LaneDeletionView,
    LaneList, MarkdownBlockView, MarkdownInlineView, MovedNoteView, NoteContentView, NoteMoveView,
    NoteNodeView, NotePathView, NoteWriteView, ProjectList, ProjectView, RecoveryView,
    TicketBlockView, TicketDetail, TicketList, TicketSummary,
};

/// What `doctor` answers: ADR 0014's shape, `{"ok": bool, "checks": [...]}`.
///
/// The three rows, in the pinned order, are built by
/// [`crate::diagnostics::report`] (ADR 0045 cut the report to `home`, `store`
/// and `bikeshed`). `ok` is the AND of the rows that can fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    /// The AND of the rows that can fail.
    pub ok: bool,
    /// The checks that ran, in the order ADR 0014 pins for them.
    pub checks: Vec<DoctorCheck>,
}

/// One row of the report: exactly `name`, `ok` and `detail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    /// The check's stable name; renaming one is a breaking change to the wire.
    pub name: String,
    /// Whether this check passed.
    pub ok: bool,
    /// What the check saw, in the words its reader needs.
    pub detail: String,
}
