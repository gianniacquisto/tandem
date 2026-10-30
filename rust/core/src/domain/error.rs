//! The domain's failures, in the words a caller acts on.
//!
//! Ported from `Errors.swift`: every case names the value that failed and the
//! next step, because the CLI prints the message verbatim on stderr and the app
//! shows it in an alert. A store failure is wrapped here too ([`DomainError::Io`],
//! [`DomainError::Locked`]), so the domain's surface is one error type.

use std::fmt;

use crate::store::StoreError;

use super::ticket::Ticket;

/// What a domain operation answers, or why it did not.
pub type DomainResult<T> = Result<T, DomainError>;

/// A failure a human or an agent can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Another process held the store past its 2 s window.
    Locked,
    /// The store itself failed: an unreadable file, a full disk.
    Io(String),
    /// A row this build cannot read: a ticket's id or timestamp is not the shape
    /// the store writes. A row that is silently repaired (a nil id, a zero time) is
    /// a ticket that moved or vanished, and the diff matches rows by id, so the
    /// repair would land in the queue of a different ticket.
    Unreadable(String),
    /// `add` was called with an empty or whitespace-only title.
    EmptyTitle,
    /// `add_from_note` was given no text to title a ticket from.
    EmptySelection,
    /// `add_from_note` was given a path that is not bikeshed-relative.
    InvalidNotePath(String),
    /// A comment body was empty after trimming.
    EmptyCommentBody,
    /// A stdin description body was empty after trimming.
    EmptyDescriptionBody,
    /// `list` was given a filter that is not `open`, `done` or `all`.
    InvalidFilter(String),
    /// A status value that is not one of the six.
    InvalidStatus(String),
    /// A cycle lane name was empty or whitespace-only.
    EmptyLaneName,
    /// A cycle lane name was already in use.
    DuplicateLane(String),
    /// A lane name was referenced but does not exist.
    UnknownLane(String),
    /// A lane key that is not uppercase alphanumerics.
    InvalidLaneKey(String),
    /// A lane key another lane already holds.
    DuplicateLaneKey(String),
    /// A lane's key is frozen: a ticket in the lane holds a number, so moving the
    /// key would move a ref a human has already written down.
    FrozenLaneKey {
        key: String,
        lane: String,
        refs: Vec<String>,
    },
    /// A handle looked like a ref (`KEY-N`) but named nothing; `hint` says
    /// whether the key is unknown or the number was never issued.
    UnknownRef { reference: String, hint: String },
    /// A handle named no ticket on the board. `open` is the open list, so the
    /// caller recovers in one turn.
    UnknownId { id: String, open: Vec<Ticket> },
    /// `block` was asked to make a ticket wait on itself.
    TicketBlocksItself { reference: String },
    /// `block` would have closed a cycle. `path` is the whole loop.
    TicketCycle { path: Vec<String> },
    /// `block` named a blocker that is already done.
    TicketBlockerDone { reference: String },
    /// `block` named a blocker that is archived.
    TicketBlockerArchived { reference: String },
    /// A ticket that already links to a note cannot be promoted again.
    AlreadyLinked { path: String },
    /// `start` was given an empty or whitespace-only brief.
    EmptySessionBrief,
    /// A session id was empty or whitespace-only.
    EmptySessionId,
    /// A `--summary -` body was empty or whitespace-only: an empty pipe, not a
    /// session that ended without saying anything.
    EmptySessionSummary,
    /// `end`/`link`/`show` named a session id the store does not carry. `known`
    /// is the ids it does carry, so the caller recovers in one turn.
    UnknownSession { id: String, known: Vec<String> },
    /// `run`/`close` was given an empty note; `close` with no note is `None`.
    EmptySprintNote,
    /// `sprint ticket` named an id the board does not carry.
    UnknownTicketId(String),
    /// A (lane, number) the store does not carry. `known` is the numbers the
    /// lane does hold.
    UnknownSprint {
        lane: String,
        number: i64,
        known: Vec<i64>,
    },
    /// A lane-only sprint handle resolved to no open record.
    NoOpenSprint(String),
    /// A `--skills` value that is not comma-separated `name=count` pairs.
    InvalidSkills(String),
    /// A caller-supplied timestamp is not the store's own ISO-8601 UTC form. A
    /// stamp the write cannot read is a record every later read fails on.
    InvalidStamp(String),
    /// The harness database the run asked for is not a file.
    NoHarnessDatabase { path: String },
    /// The harness database carries no row for the session id.
    UnknownHarnessSession { id: String },
}

impl DomainError {
    /// The message a caller reads: the same words the Swift core used, so the
    /// two faces say one thing about one failure.
    pub fn message(&self) -> String {
        match self {
            Self::Locked => {
                "The Tandem store is locked by another process. Please retry.".to_string()
            }
            Self::Io(detail) => format!("Tandem store error: {detail}. Try again."),
            Self::Unreadable(detail) => format!(
                "The store holds a row this build cannot read ({detail}). Restore the backup \
                 at tandem.db.bak, or import the home again."
            ),
            Self::EmptyTitle => {
                "Title is empty. Provide a non-empty ticket title and try again.".to_string()
            }
            Self::EmptySelection => "Nothing to send. Select text in the note, or put the caret \
                 on a checkbox line, and try again."
                .to_string(),
            Self::InvalidNotePath(bad) => format!(
                "Note path \"{bad}\" is not a bikeshed-relative path (e.g. projects/App.md)."
            ),
            Self::EmptyCommentBody => {
                "Comment body is empty. Provide a non-empty body and try again.".to_string()
            }
            Self::EmptyDescriptionBody => "Description body is empty. Provide a non-empty body on \
                 stdin, or pass --description \"\" to clear the description."
                .to_string(),
            Self::InvalidFilter(bad) => {
                format!("filter must be one of open, done, all (got \"{bad}\").")
            }
            Self::InvalidStatus(bad) => format!(
                "status must be one of {} (got \"{bad}\").",
                super::ticket::Status::WIRE_VALUES.join(", ")
            ),
            Self::EmptyLaneName => {
                "Cycle lane name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::DuplicateLane(name) => {
                format!("A cycle lane named \"{name}\" already exists.")
            }
            Self::UnknownLane(name) => {
                format!("Unknown cycle lane \"{name}\". Create it first.")
            }
            Self::InvalidLaneKey(bad) => format!(
                "Lane key \"{bad}\" is not valid. A key is uppercase letters and digits (e.g. TAN)."
            ),
            Self::DuplicateLaneKey(key) => {
                format!("The lane key \"{key}\" is already in use. Pick another key.")
            }
            Self::FrozenLaneKey { key, lane, refs } => format!(
                "The key \"{key}\" of lane \"{lane}\" is frozen: {} already carry it. \
                 A key cannot move once a ticket in the lane holds a number.",
                refs.join(", ")
            ),
            Self::UnknownRef { reference, hint } => {
                format!("Unknown ticket ref \"{reference}\". {hint}")
            }
            Self::UnknownId { id, .. } => format!(
                "Unknown ticket id \"{id}\". Here is the current open list to pick the right one from:"
            ),
            Self::TicketBlocksItself { reference } => format!(
                "{reference} cannot block on itself. Name the ticket that has to finish first."
            ),
            Self::TicketCycle { path } => format!(
                "That edge would close a cycle: {}. Two tickets cannot wait on each other; \
                 unblock one of them first.",
                path.join(" -> ")
            ),
            Self::TicketBlockerDone { reference } => format!(
                "{reference} is done, so the block would lift the moment it is written. \
                 Block on the ticket that is still open."
            ),
            Self::TicketBlockerArchived { reference } => format!(
                "{reference} is archived, so the block could never be seen to lift. \
                 Bring it back to the board first."
            ),
            Self::AlreadyLinked { path } => format!(
                "This ticket already links to a note (\"{path}\"). Open that note, or remove \
                 the link before promoting again."
            ),
            Self::EmptySessionBrief => "Session brief is empty. Provide a non-empty brief, what \
                 the session is for and what done looks like, and try again."
                .to_string(),
            Self::EmptySessionId => "Session id is empty. Provide a non-empty session id, the \
                 handle a session is known by, and try again."
                .to_string(),
            Self::EmptySessionSummary => "Session summary is empty. Provide a non-empty summary \
                 on stdin, or omit --summary to end the session without one."
                .to_string(),
            Self::UnknownSession { id, known } if known.is_empty() => {
                format!("Unknown session id \"{id}\". No session is recorded yet.")
            }
            Self::UnknownSession { id, known } => format!(
                "Unknown session id \"{id}\". Here are the recorded sessions to pick the right \
                 one from: {}.",
                known.join(", ")
            ),
            Self::EmptySprintNote => {
                "Sprint note is empty. Provide a non-empty note and try again.".to_string()
            }
            Self::UnknownTicketId(id) => format!(
                "Unknown ticket id \"{id}\", it is not on the board. Add the ticket to the \
                 board first, or check the id, and try again."
            ),
            Self::UnknownSprint {
                lane,
                number,
                known,
            } if known.is_empty() => format!(
                "Unknown sprint \"Sprint {number} — {lane}\". No sprint is recorded in lane \
                 \"{lane}\" yet."
            ),
            Self::UnknownSprint {
                lane,
                number,
                known,
            } => format!(
                "Unknown sprint \"Sprint {number} — {lane}\". Here are the sprints lane \
                 \"{lane}\" holds: {}.",
                known
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NoOpenSprint(lane) => format!(
                "No sprint is open in lane \"{lane}\". Run `sprint open --lane <lane>` first, \
                 or check the lane, and try again."
            ),
            Self::InvalidSkills(bad) => format!(
                "--skills must be comma-separated name=count pairs, each a name and a \
                 non-negative whole count (got \"{bad}\")."
            ),
            Self::InvalidStamp(bad) => format!(
                "\"{bad}\" is not a timestamp. Stamps are ISO-8601 UTC, \
                 YYYY-MM-DDTHH:MM:SSZ; omit the value to use the current time."
            ),
            Self::NoHarnessDatabase { path } => format!(
                "No harness database at {path}. Check the path, or let the harness run once, \
                 and try again."
            ),
            Self::UnknownHarnessSession { id } => format!(
                "No harness row for session \"{id}\". Check the session id, or end the record \
                 without harness usage, and try again."
            ),
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
            other => Self::Io(other.to_string()),
        }
    }
}
