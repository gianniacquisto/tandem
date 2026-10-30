//! What a record write answers: the record, and the note the caller surfaces
//! when the write changed nothing.
//!
//! The Swift core's `OperationResult`: a re-run of `start`, `open`, `end` or
//! `close` is not a failure, it is a success that says the record was already
//! there. The note is the caller's to print (the CLI puts it on stderr), so a
//! script sees a zero exit and a human sees why nothing moved.

/// A write's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation<T> {
    /// The record the write answered with, changed or unchanged.
    pub value: T,
    /// Why nothing changed, when nothing did.
    pub note: Option<String>,
}
