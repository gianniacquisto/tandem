//! The domain's failures, in the words a caller acts on.
//!
//! The Swift core's `DomainError` (`Sources/TandemCore/Errors.swift`) is the
//! model: a failure names the value that failed and the next step, and the CLI
//! prints `message()` to stderr and exits 1. The port keeps the ported
//! sentences verbatim where a golden pins them, so the two binaries answer an
//! agent with the same words.
//!
//! A store failure is a variant, not a second error type: contention is
//! [`DomainError::Locked`] (the store's own word), and everything else keeps the
//! store's detail ([`DomainError::Store`]) exactly as the Swift store translated
//! `SQLiteError` (ADR 0007).

use std::fmt;

use crate::store::StoreError;

/// What a domain operation answers, or why it did not.
pub type DomainResult<T> = Result<T, DomainError>;

/// A contract-level failure: the caller can fix what it named and try again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// The store's sidecar lock or write transaction was held past the window.
    Locked,
    /// `start` was given an empty or whitespace-only brief.
    EmptySessionBrief,
    /// A session id was empty or whitespace-only.
    EmptySessionId,
    /// A `--summary -` body was empty or whitespace-only: an empty pipe, not a
    /// session that ended without saying anything.
    EmptySessionSummary,
    /// `end`/`link`/`show` named a session id the store does not carry.
    UnknownSession { id: String, known: Vec<String> },
    /// A lane name was empty or whitespace-only.
    EmptyLaneName,
    /// `run`/`close` was given an empty note; `close` with no note is `None`.
    EmptySprintNote,
    /// `sprint ticket` named an id the board does not carry.
    UnknownTicketId(String),
    /// A (lane, number) the store does not carry. `known` is the numbers the
    /// lane does hold, so the caller recovers in one turn.
    UnknownSprint {
        lane: String,
        number: i64,
        known: Vec<i64>,
    },
    /// A lane-only sprint handle resolved to no open record. The next step is
    /// the lane's own `sprint open`.
    NoOpenSprint(String),
    /// A `--skills` value that is not comma-separated `name=count` pairs.
    InvalidSkills(String),
    /// The harness database the run asked for is not a file.
    NoHarnessDatabase { path: String },
    /// The harness database carries no row for the session id.
    UnknownHarnessSession { id: String },
    /// The store failed for a reason the domain has no better words for.
    Store(StoreError),
}

impl DomainError {
    /// The one plain-text sentence a caller surfaces verbatim.
    pub fn message(&self) -> String {
        match self {
            Self::Locked => "The Tandem store is locked by another process, please retry.".to_string(),
            Self::EmptySessionBrief => "Session brief is empty. Provide a non-empty brief, what the session is for and what done looks like, and try again.".to_string(),
            Self::EmptySessionId => "Session id is empty. Provide a non-empty session id, the handle a session is known by, and try again.".to_string(),
            Self::EmptySessionSummary => "Session summary is empty. Provide a non-empty summary on stdin, or omit --summary to end the session without one.".to_string(),
            Self::UnknownSession { id, known } if known.is_empty() => {
                format!("Unknown session id \"{id}\". No session is recorded yet.")
            }
            Self::UnknownSession { id, known } => format!(
                "Unknown session id \"{id}\". Here are the recorded sessions to pick the right one from: {}.",
                known.join(", ")
            ),
            Self::EmptyLaneName => {
                "Cycle lane name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::EmptySprintNote => {
                "Sprint note is empty. Provide a non-empty note and try again.".to_string()
            }
            Self::UnknownTicketId(id) => format!(
                "Unknown ticket id \"{id}\", it is not on the board. Add the ticket to the board first, or check the id, and try again."
            ),
            Self::UnknownSprint {
                lane,
                number,
                known,
            } if known.is_empty() => format!(
                "Unknown sprint \"Sprint {number} — {lane}\". No sprint is recorded in lane \"{lane}\" yet."
            ),
            Self::UnknownSprint {
                lane,
                number,
                known,
            } => format!(
                "Unknown sprint \"Sprint {number} — {lane}\". Here are the sprints lane \"{lane}\" holds: {}.",
                known
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NoOpenSprint(lane) => format!(
                "No sprint is open in lane \"{lane}\". Run `sprint open --lane <lane>` first, or check the lane, and try again."
            ),
            Self::InvalidSkills(bad) => format!(
                "--skills must be comma-separated name=count pairs, each a name and a non-negative whole count (got \"{bad}\")."
            ),
            Self::NoHarnessDatabase { path } => format!(
                "No harness database at {path}. Check the path, or let the harness run once, and try again."
            ),
            Self::UnknownHarnessSession { id } => format!(
                "No harness row for session \"{id}\". Check the session id, or end the record without harness usage, and try again."
            ),
            Self::Store(error) => error.to_string(),
        }
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for DomainError {}

impl From<StoreError> for DomainError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Locked => Self::Locked,
            other => Self::Store(other),
        }
    }
}
