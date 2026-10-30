//! The undo a completion leaves behind: **one** at a time, and it expires
//! (`CompletionUndo.swift`).
//!
//! Completing a ticket is one click and immediate, and it erases the stage the
//! ticket came from, because `done` is a status value and there is no previous
//! stage to restore (ADR 0017). A mis-click therefore costs a re-triage, not one
//! click back. The undo is the way back: a brief offer, not a history.
//!
//! The offer is raised by the *act* of completing, never by observing one: a
//! ticket completed elsewhere arrives as a reload and raises nothing.

use uuid::Uuid;

/// How long the offer stays open: six seconds.
pub const WINDOW: i64 = 6;

/// The offer to put one completion back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionUndo {
    /// The ticket to put back.
    pub id: Uuid,
    /// Its title, so the toast can name the ticket it is talking about.
    pub title: String,
    /// When the completion happened; the offer's clock starts here.
    pub at: i64,
}

impl CompletionUndo {
    pub fn new(id: Uuid, title: impl Into<String>, at: i64) -> Self {
        Self {
            id,
            title: title.into(),
            at,
        }
    }

    /// When the offer runs out.
    pub fn expires_at(&self) -> i64 {
        self.at + WINDOW
    }

    /// Whether the offer is still open at `now`.
    pub fn is_live(&self, now: i64) -> bool {
        now < self.expires_at()
    }
}
