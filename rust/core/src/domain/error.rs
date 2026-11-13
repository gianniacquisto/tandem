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
    /// `set` named `--description` and `--append-description` together.
    ConflictingDescriptionEdit,
    /// A stdin body was not UTF-8. A body is text, not a vault path, so this is
    /// its own failure rather than the bikeshed's I/O one.
    StandardInputNotUtf8,
    /// `list` was given a filter that is not `open`, `done` or `all`.
    InvalidFilter(String),
    /// A status value that is empty, or the retired `refined` spelling.
    InvalidStatus(String),
    /// A status value the active workflow does not declare. `stages` is the
    /// valid stage-key list, so the refusal names what the caller may pick.
    InvalidStage { value: String, stages: Vec<String> },
    /// A workflow handle (id or name) named nothing. `workflows` is every
    /// declared name, so the refusal lets the caller recover in one turn.
    UnknownWorkflow {
        handle: String,
        workflows: Vec<String>,
    },
    /// A workflow name was empty or whitespace-only.
    EmptyWorkflowName,
    /// A workflow name another workflow already holds.
    DuplicateWorkflowName(String),
    /// A workflow scope other than `home`; per-project adoption is later.
    InvalidWorkflowScope(String),
    /// A workflow document carries no stages.
    EmptyWorkflowStages,
    /// Two stages share a key.
    DuplicateStageKey(String),
    /// A stage key was empty or whitespace-only.
    EmptyStageKey,
    /// A stage label was empty or whitespace-only.
    EmptyStageLabel(String),
    /// A stage role appears a cardinality the model refuses.
    StageRoleCount {
        role: String,
        count: usize,
        expected: String,
    },
    /// A binding names a roster role the store does not carry.
    UnknownBindingRole { role: String, roster: Vec<String> },
    /// A stage a workflow set would delete still holds tickets.
    StageOccupied { key: String, count: usize },
    /// `workflow use` was refused because tickets sit in stages the target
    /// lacks. Each pair is the stage key and how many tickets sit in it.
    WorkflowMissingStages { stages: Vec<(String, usize)> },
    /// The default workflow cannot be deleted.
    WorkflowIsDefault(String),
    /// The active workflow cannot be deleted.
    WorkflowIsActive(String),
    /// A cycle lane name was empty or whitespace-only.
    EmptyLaneName,
    /// A cycle lane name was already in use.
    DuplicateLane(String),
    /// A lane name was referenced but does not exist.
    UnknownLane(String),
    /// A project name was empty or whitespace-only.
    EmptyProjectName,
    /// A project name was already in use.
    DuplicateProject(String),
    /// A `--project` named a project the store does not carry. `projects` is
    /// the declared list, so the refusal names what the caller may pick.
    UnknownProjectName { name: String, projects: Vec<String> },
    /// A project delete named a project that still holds lanes. `lanes` names
    /// them, so the caller re-parents first and deletes after: a delete never
    /// strands or silently re-homes numbered lanes.
    ProjectNotEmpty { project: String, lanes: Vec<String> },
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
    /// `ticket move` was given a position that is not a whole number.
    InvalidIndex(String),
    /// `--assignee self` / `--origin self` was used and `TANDEM_AGENT` names no
    /// agent.
    AgentNameUnset,
    /// A caller-supplied timestamp is not the store's own ISO-8601 UTC form. A
    /// stamp the write cannot read is a record every later read fails on.
    InvalidStamp(String),
    /// The bikeshed lock could not be acquired within its bounded retry window.
    BikeshedLocked,
    /// The bikeshed itself failed: an unreadable note, a failed rename.
    BikeshedIo(String),
    /// `read` / `delete` / `rename` / `move` / `backlinks` was called with a path
    /// not present in the bikeshed.
    UnknownNote(String),
    /// `asset` was given a file that is not one of the servable image types.
    NotAnAsset(String),
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
            Self::InvalidStage { value, stages } => format!(
                "Unknown stage \"{value}\". The active workflow's stages are: {}.",
                stages.join(", ")
            ),
            Self::UnknownWorkflow { handle, workflows } if workflows.is_empty() => {
                format!("Unknown workflow \"{handle}\". No workflows are declared yet.")
            }
            Self::UnknownWorkflow { handle, workflows } => format!(
                "Unknown workflow \"{handle}\". The known workflows are: {}.",
                workflows.join(", ")
            ),
            Self::EmptyWorkflowName => {
                "Workflow name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::DuplicateWorkflowName(name) => {
                format!("A workflow named \"{name}\" already exists.")
            }
            Self::InvalidWorkflowScope(scope) => format!(
                "Workflow scope must be home (got \"{scope}\"). Per-project workflows land later."
            ),
            Self::EmptyWorkflowStages => {
                "A workflow needs at least one stage. Add a stage and try again.".to_string()
            }
            Self::DuplicateStageKey(key) => {
                format!("The stage key \"{key}\" is already in use. Pick another key.")
            }
            Self::EmptyStageKey => {
                "A stage key is empty. Provide a non-empty key and try again.".to_string()
            }
            Self::EmptyStageLabel(key) => {
                format!("The stage \"{key}\" has an empty label. Give it a label.")
            }
            Self::StageRoleCount {
                role,
                count,
                expected,
            } => format!("A workflow needs {expected} {role} stage (found {count})."),
            Self::UnknownBindingRole { role, roster } => format!(
                "Unknown binding role \"{role}\". The roster is: {}.",
                roster.join(", ")
            ),
            Self::StageOccupied { key, count } => format!(
                "Stage \"{key}\" still holds {count} ticket(s). Move them out before deleting it."
            ),
            Self::WorkflowMissingStages { stages } => format!(
                "The target workflow lacks stages tickets sit in: {}. Move them first.",
                stages
                    .iter()
                    .map(|(key, count)| format!("\"{key}\" ({count})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::WorkflowIsDefault(handle) => {
                format!("Workflow \"{handle}\" is the default. It cannot be deleted.")
            }
            Self::WorkflowIsActive(handle) => {
                format!("Workflow \"{handle}\" is active. Activate another one before deleting it.")
            }
            Self::EmptyLaneName => {
                "Cycle lane name is empty. Provide a non-empty name and try again.".to_string()
            }
            Self::DuplicateLane(name) => {
                format!("A cycle lane named \"{name}\" already exists.")
            }
            Self::UnknownLane(name) => {
                format!(
                    "Unknown cycle lane \"{name}\". Create it with `lane add --name \"{name}\"`, or list lanes with `lane list`."
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
            Self::ProjectNotEmpty { project, lanes } => format!(
                "Project \"{project}\" still holds lanes: {}. Re-parent them first, then delete.",
                lanes.join(", ")
            ),
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
            Self::BikeshedLocked => {
                "The bikeshed is locked by another process. Please retry.".to_string()
            }
            Self::BikeshedIo(detail) => format!("Bikeshed error: {detail}. Try again."),
            Self::UnknownNote(path) => format!(
                "No note at \"{path}\" in the bikeshed. List the bikeshed to see what is there."
            ),
            Self::NotAnAsset(path) => format!(
                "No image at \"{path}\" in the bikeshed. Only PNG, JPG, JPEG, GIF, WebP, AVIF, \
                 SVG, BMP and ICO files are served."
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
