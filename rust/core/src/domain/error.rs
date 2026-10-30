//! The domain's failures, in the words a caller acts on.
//!
//! Ported from `Errors.swift`: every case names the value that failed and the
//! next step, because the CLI prints the message verbatim on stderr and the app
//! shows it in an alert. A store failure is wrapped here too ([`DomainError::Io`],
//! [`DomainError::Locked`]), so the domain's surface is one error type.

use std::fmt;

use crate::store::StoreError;

use super::ticket::Ticket;

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
