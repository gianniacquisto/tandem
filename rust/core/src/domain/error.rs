//! The domain's failures, in the words a caller acts on.
//!
//! Ported from `Errors.swift`: every case names the value that failed and the
//! next step, because the CLI prints the message verbatim on stderr and the app
//! shows it in an alert. A store failure is wrapped here too ([`DomainError::Io`],
//! [`DomainError::Locked`]), so the domain's surface is one error type.

use std::fmt;
use std::path::PathBuf;

use crate::store::StoreError;

use super::ticket::Ticket;

/// What a domain operation answers, or why it did not.
pub type DomainResult<T> = Result<T, DomainError>;

/// A failure a human or an agent can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// The store at this home was held past its 2 s window, by another
    /// process's transaction or by a sibling thread of this one. It names the
    /// home rather than the holder, because the same words are true for both.
    Locked(PathBuf),
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
    /// `--brief` named a regular file: the flag never read it, so the path would
    /// be filed as the brief, and a session start has no repair.
    BriefNamesAFile(String),
    /// A session id was empty or whitespace-only.
    EmptySessionId,
    /// No session id was given and the environment names none: an invented
    /// handle is a record nothing can look up again.
    SessionIdUnset,
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
    /// A usage flag (`--tokens-input`, `--cost`, …) was given a value that is not
    /// a non-negative number.
    InvalidUsageFlag { flag: String, value: String },
    /// `sprint show` was given a number that is not a whole number.
    InvalidSprintNumber(String),
    /// `ticket move` was given a position that is not a whole number.
    InvalidIndex(String),
    /// `--assignee self` / `--origin self` was used and `TANDEM_AGENT` names no
    /// agent.
    AgentNameUnset,
    /// A caller-supplied timestamp is not the store's own ISO-8601 UTC form. A
    /// stamp the write cannot read is a record every later read fails on.
    InvalidStamp(String),
    /// The harness database the run asked for is not a file.
    NoHarnessDatabase { path: String },
    /// The harness database carries no row for the session id.
    UnknownHarnessSession { id: String },
    /// The bikeshed lock could not be acquired within its bounded retry window.
    BikeshedLocked,
    /// The bikeshed itself failed: an unreadable note, a failed rename.
    BikeshedIo(String),
    /// `read` / `delete` / `rename` / `move` / `backlinks` was called with a path
    /// not present in the bikeshed.
    UnknownNote(String),
    /// A create, rename or move target that already exists. An error, never an
    /// overwrite.
    NoteAlreadyExists(String),
    /// A name the bikeshed's create/rename rule refuses.
    InvalidName(String),
    /// A capture `project` or `session` that is not one visible folder name.
    InvalidCaptureName(String),
    /// A capture kind that is not one of the four.
    InvalidCaptureKind(String),
    /// The retired `work` capture kind: the session summary is the work record.
    RetiredCaptureKind(String),
    /// A capture body that is empty or whitespace-only.
    EmptyCaptureBody,
    /// A capture body whose first non-empty line carries no words, so the
    /// filename would fall back to the kind's own name.
    EmptyCaptureSlug(String),
    /// A folder moved into itself or one of its own descendants.
    MoveCycle { path: String, into: String },
}

impl DomainError {
    /// The message a caller reads: the same words the Swift core used, so the
    /// two faces say one thing about one failure.
    pub fn message(&self) -> String {
        match self {
            Self::Locked(home) => format!(
                "The Tandem store at {} is locked by another writer. Please retry.",
                home.display()
            ),
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
            Self::BriefNamesAFile(path) => format!(
                "\"{path}\" is a file, not a brief. `--brief` takes the literal brief text, or \
                 `--brief -` to read it from stdin; pipe the file instead."
            ),
            Self::EmptySessionId => "Session id is empty. Provide a non-empty session id, the \
                 handle a session is known by, and try again."
                .to_string(),
            Self::SessionIdUnset => "Cannot resolve the session id: set OPENCODE_SESSION_ID to \
                 this session's id, or pass --session <id>."
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
            Self::InvalidUsageFlag { flag, value } => {
                format!("--{flag} must be a non-negative number (got \"{value}\").")
            }
            Self::InvalidSprintNumber(bad) => {
                format!("number must be a whole number (got \"{bad}\").")
            }
            Self::InvalidIndex(bad) => {
                format!("index must be a whole number (got \"{bad}\").")
            }
            Self::AgentNameUnset => "Cannot resolve self: set TANDEM_AGENT to this agent's \
                 name, or pass a name instead of self."
                .to_string(),
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
            Self::BikeshedLocked => {
                "The bikeshed is locked by another process. Please retry.".to_string()
            }
            Self::BikeshedIo(detail) => format!("Bikeshed error: {detail}. Try again."),
            Self::UnknownNote(path) => format!(
                "No note at \"{path}\" in the bikeshed. List the bikeshed to see what is there."
            ),
            Self::NoteAlreadyExists(path) => {
                format!("A file already exists at \"{path}\". Choose a different name.")
            }
            Self::InvalidName(name) => format!(
                "Name \"{name}\" is not allowed: note names cannot contain # or |, and must \
                 end in .md. Choose a different name."
            ),
            Self::InvalidCaptureName(name) => format!(
                "Capture name \"{name}\" is not allowed: project and session must each be one \
                 folder name with no /, #, |, or leading dot. Choose a different name."
            ),
            Self::InvalidCaptureKind(kind) => format!(
                "Unknown capture kind \"{kind}\". Use one of: {}.",
                crate::bikeshed::CaptureKind::WIRE_VALUES.join(", ")
            ),
            Self::RetiredCaptureKind(kind) => format!(
                "The capture kind \"{kind}\" is retired. The summary at `session end \
                 --summary` is the work record (ADR 0027). Use one of: {}.",
                crate::bikeshed::CaptureKind::WIRE_VALUES.join(", ")
            ),
            Self::EmptyCaptureBody => "Capture body is empty. Provide a non-empty body on \
                 stdin and try again."
                .to_string(),
            Self::EmptyCaptureSlug(kind) => format!(
                "Capture \"{kind}\" has no usable slug: the body's first non-empty line \
                 carries no words. Start the body with a short title line (e.g. \"# Caret not \
                 placed\") and try again."
            ),
            Self::MoveCycle { path, into } => {
                let destination = if into.is_empty() {
                    "the bikeshed root".to_string()
                } else {
                    format!("\"{into}\"")
                };
                format!(
                    "Cannot move \"{path}\" into {destination}: a folder cannot be moved \
                     into itself. Choose a different folder."
                )
            }
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
            StoreError::Locked { home } => Self::Locked(home),
            other => Self::Io(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Contention answers the same failure whether the holder is a thread of
    /// this process or a writer in another one, so the words name the store
    /// (the home the caller chose) rather than a process the caller never saw.
    #[test]
    fn the_locked_message_names_the_home_and_not_a_process() {
        let home = PathBuf::from("/tmp/tandem-locked-home");
        let error: DomainError = StoreError::Locked { home: home.clone() }.into();
        let message = error.message();

        assert_eq!(error, DomainError::Locked(home.clone()));
        assert!(
            message.contains(&home.display().to_string()),
            "the message names the home: {message}"
        );
        assert!(
            !message.contains("process"),
            "a sibling thread answers the same words, so the message blames no process: {message}"
        );
    }
}
