//! The wire: the JSON the agent face prints and the app reads.
//!
//! One owner. The CLI parses argv and prints these types; it shapes no message
//! of its own, so `--help` drift and wire drift can never be the same bug
//! twice. `ts-rs` derives the TypeScript declarations from the same structs,
//! so the app cannot ship a shape the core does not answer (PRT-12).

mod views;

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

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
    ArchivedView, CommentList, CommentResult, CommentView, LaneDeletionView, LaneList,
    NotePathView, SessionDetailView, SessionList, SessionResult, SprintList, SprintResult,
    TicketBlockView, TicketDetail, TicketList, TicketSummary,
};

/// What `doctor` answers: ADR 0014's shape, `{"ok": bool, "checks": [...]}`.
///
/// The eight checks land with PRT-10; this stub answers the one row it owns,
/// `home`, which is the half every verb already depends on. `home` is
/// informational in ADR 0014 and cannot fail, so `ok` is true here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DoctorReport {
    /// The AND of the rows that can fail. This stub's one row cannot.
    pub ok: bool,
    /// The checks that ran, in the order ADR 0014 pins for them.
    pub checks: Vec<DoctorCheck>,
}

/// One row of the report: exactly `name`, `ok` and `detail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DoctorCheck {
    /// The check's stable name; renaming one is a breaking change to the wire.
    pub name: String,
    /// Whether this check passed.
    pub ok: bool,
    /// What the check saw, in the words its reader needs.
    pub detail: String,
}
