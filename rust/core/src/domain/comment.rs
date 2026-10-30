//! Comments on a ticket (`Comments.swift`, `CommentRead.swift`,
//! `CommentWrite.swift`; ADR 0017's 2026-10-20 amendment, re-backed on the
//! database by ADR 0031).
//!
//! **The log is append-only.** There is no edit and no delete, and a deleted
//! ticket's log stays. `at` is whole seconds since the Unix epoch, the same fact
//! the store writes as ISO-8601 UTC.
//!
//! The legacy `comments/<ticket-id>.jsonl` logs have no reader in the port: the
//! one-off import (PRT-18) seeds the rows once, and the table is the log's truth.

/// One comment on a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The one who commented: an assignee, an agent's name, or `""` when unnamed.
    pub author: String,
    /// When it was appended, whole seconds since the Unix epoch.
    pub at: i64,
    /// The body: markdown, trimmed at its edges; never empty.
    pub body: String,
}

/// How a read of a ticket's comments ended.
///
/// The three cases mean different things to whoever reads them, so they stay
/// apart. An empty log is nothing to say. A **busy** store is a read that waited
/// out the store's ~2 s window and gave up: nothing is wrong with the log, and the
/// read can simply be tried again. A **failed** read is a store failure the app
/// shows in the conversation's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentRead {
    /// The comments, in append order (oldest first). Empty when there are none.
    List(Vec<Comment>),
    /// The store was locked for longer than the read waited.
    Busy(String),
    /// The store could not be read; the message is the core's own.
    Failed(String),
}
