//! The domain's failures, in the words a caller acts on.
//!
//! Every case names the value that failed and the
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
    /// The store was written by a newer Tandem: the words name the upgrade, and
    /// retrying cannot help.
    StoreNewer(String),
    /// A row this build cannot read: a ticket's id or timestamp is not the shape
    /// the store writes. A row that is silently repaired (a nil id, a zero time) is
    /// a ticket that moved or vanished, and the diff matches rows by id, so the
    /// repair would land in the queue of a different ticket.
    Unreadable(String),
    /// `add` was called with an empty or whitespace-only title.
    EmptyTitle,
    /// A comment body was empty after trimming.
    EmptyCommentBody,
    /// A stdin description body was empty after trimming.
    EmptyDescriptionBody,
    /// `set` named `--description` and `--append-description` together.
    ConflictingDescriptionEdit,
    /// A stdin body was not UTF-8. A body is text, not a vault path, so this is
    /// its own failure rather than the bikeshed's I/O one.
    StandardInputNotUtf8,
    /// `list` was given a filter that is not `open`, `done` or `all`.
    InvalidFilter(String),
    /// A status value that is empty, the retired `refined` spelling, or a word
    /// this build does not name.
    InvalidStatus(String),
    /// A label name was empty or whitespace-only.
    EmptyLabelName,
    /// A label name was already in use.
    DuplicateLabel(String),
    /// A label name was referenced but does not exist.
    UnknownLabel(String),
    /// A project name was empty or whitespace-only.
    EmptyProjectName,
    /// A project name was already in use.
    DuplicateProject(String),
    /// A `--project` named a project the store does not carry. `projects` is
    /// the declared list, so the refusal names what the caller may pick.
    UnknownProjectName { name: String, projects: Vec<String> },
    /// A project delete named a project that still holds labels. `labels` names
    /// them, so the caller re-parents first and deletes after: a delete never
    /// strands or silently re-homes numbered labels.
    ProjectNotEmpty {
        project: String,
        labels: Vec<String>,
    },
    /// A label key that is not uppercase alphanumerics.
    InvalidLabelKey(String),
    /// A label key another label already holds.
    DuplicateLabelKey(String),
    /// A label's key is frozen: a ticket in the label holds a number, so moving the
    /// key would move a ref a human has already written down.
    FrozenLabelKey {
        key: String,
        label: String,
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
    /// `ticket move` was given a position that is not a whole number.
    InvalidIndex(String),
    /// `--assignee self` / `--origin self` was used and `TANDEM_AGENT` names no
    /// agent.
    AgentNameUnset,
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
    /// A write carried an expected hash and the note on disk no longer has it:
    /// someone saved in between. Nothing was written.
    StaleNote(String),
    /// A targeted edit whose `oldText` is not in the note. Nothing was written.
    EditNoMatch(String),
    /// A targeted edit whose `oldText` is in the note more than once. Nothing
    /// was written.
    EditAmbiguous { path: String, count: usize },
    /// A targeted edit that would append nothing: both texts empty.
    EditEmpty,
    /// A name the bikeshed's create/rename rule refuses.
    InvalidName(String),
    /// A folder moved into itself or one of its own descendants.
    MoveCycle { path: String, into: String },
}

impl DomainError {
    /// The message a caller reads: the words both faces print, so the
    /// two faces say one thing about one failure.
    pub fn message(&self) -> String {
        match self {
            Self::Locked(home) => format!(
                "The Tandem store at {} is locked by another writer. Please retry.",
                home.display()
            ),
            Self::Io(detail) => format!("Tandem store error: {detail}. Try again."),
            Self::StoreNewer(detail) => detail.clone(),
            Self::Unreadable(detail) => format!(
                "The store holds a row this build cannot read ({detail}). Restore the backup \
                 at tandem.db.bak, or import the home again."
            ),
            Self::EmptyTitle => {
                "Title is empty. Provide a non-empty ticket title and try again.".to_string()
            }
            Self::EmptyCommentBody => {
                "Comment body is empty. Provide a non-empty body and try again.".to_string()
            }
            Self::EmptyDescriptionBody => "Description body is empty. Provide a non-empty body on \
                 stdin, or pass --description \"\" to clear the description."
                .to_string(),
            Self::ConflictingDescriptionEdit => "--description and --append-description cannot be \
                 combined. Use one or the other and try again."
                .to_string(),
            Self::StandardInputNotUtf8 => "stdin is not valid UTF-8. Try again.".to_string(),
            Self::InvalidFilter(bad) => {
                format!("filter must be one of open, done, all (got \"{bad}\").")
            }
            Self::InvalidStatus(bad) => format!(
                "status must be one of {} (got \"{bad}\").",
                super::ticket::Status::WIRE_VALUES.join(", ")
            ),
            Self::EmptyLabelName => {
                "Label name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::DuplicateLabel(name) => {
                format!("A label named \"{name}\" already exists.")
            }
            Self::UnknownLabel(name) => {
                format!(
                    "Unknown label \"{name}\". Create it with `label add --name \"{name}\"`, or list labels with `label list`."
                )
            }
            Self::EmptyProjectName => {
                "Project name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::DuplicateProject(name) => {
                format!("A project named \"{name}\" already exists.")
            }
            Self::UnknownProjectName { name, projects } if projects.is_empty() => {
                format!(
                    "Unknown project \"{name}\". No projects are declared yet; create one first."
                )
            }
            Self::UnknownProjectName { name, projects } => format!(
                "Unknown project \"{name}\". The known projects are: {}.",
                projects.join(", ")
            ),
            Self::ProjectNotEmpty { project, labels } => format!(
                "Project \"{project}\" still holds labels: {}. Re-parent them first, then delete.",
                labels.join(", ")
            ),
            Self::InvalidLabelKey(bad) => format!(
                "Label key \"{bad}\" is not valid. A key is uppercase letters and digits (e.g. TAN)."
            ),
            Self::DuplicateLabelKey(key) => {
                format!("The label key \"{key}\" is already in use. Pick another key.")
            }
            Self::FrozenLabelKey { key, label, refs } => format!(
                "The key \"{key}\" of label \"{label}\" is frozen: {} already carry it. \
                 A key cannot move once a ticket in the label holds a number.",
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
            Self::InvalidIndex(bad) => {
                format!("index must be a whole number (got \"{bad}\").")
            }
            Self::AgentNameUnset => "Cannot resolve self: set TANDEM_AGENT to this agent's \
                 name, or pass a name instead of self."
                .to_string(),
            Self::BikeshedLocked => {
                "The bikeshed is locked by another process. Please retry.".to_string()
            }
            Self::BikeshedIo(detail) => format!("Bikeshed error: {detail}. Try again."),
            Self::UnknownNote(path) => format!(
                "No note at \"{path}\" in the bikeshed. List the bikeshed to see what is there."
            ),
            Self::StaleNote(path) => format!(
                "The note \"{path}\" changed since you read it, so nothing was written. \
                 Read it again with note_read, merge your change into the new text and write it back with the new hash."
            ),
            Self::EditNoMatch(path) => format!(
                "oldText is not in \"{path}\", so nothing was written. Search or read the note again and copy the text exactly."
            ),
            Self::EditAmbiguous { path, count } => format!(
                "oldText appears {count} times in \"{path}\", so nothing was written. Include more surrounding text so it matches once."
            ),
            Self::EditEmpty => {
                "oldText and newText are both empty, so there is nothing to edit.".to_string()
            }
            Self::NoteAlreadyExists(path) => {
                format!("A file already exists at \"{path}\". Choose a different name.")
            }
            Self::InvalidName(name) => format!(
                "Name \"{name}\" is not allowed: note names cannot contain # or |, and must \
                 end in .md. Choose a different name."
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
            newer @ StoreError::Newer { .. } => Self::StoreNewer(newer.to_string()),
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
