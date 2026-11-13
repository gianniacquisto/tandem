//! The tandem binary: the agent face, and later the `rpc` the app holds.
//!
//! One binary (`docs/port.md`, Target shape): the verbs agents call and the
//! core the app trusts are the same executable, so they cannot drift. This rung
//! carries the whole grammar the skills speak (PRT-9): `ticket`, `lane`, `note`,
//! `sprint`, and `doctor`'s pinned report. `rpc` lands with PRT-11.
//!
//! What lives here is what belongs to a process rather than to the domain:
//! argv, the environment, the two streams and the exit code. Every verb maps to
//! one domain call and prints the wire type that call answers with; no boundary
//! rule is re-implemented here, so the two binaries cannot disagree about what a
//! verb means, only about how a flag was spelled.

mod usage;

use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use tandem_core::bikeshed::{Bikeshed, CaptureKind, Day};
use tandem_core::diagnostics;
use tandem_core::domain::sprint::{SprintMode, Sprints};
use tandem_core::domain::workflow::{WorkflowDraft, Workflows};
use tandem_core::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, SetFields, Settings, Status, TicketQuery,
};
use tandem_core::env::Environment;
use tandem_core::store::{self, Actor, Home, Store};
use tandem_core::wire::rpc::{SettingsReply, WorkflowListReply, WorkflowReply, WorkflowSetParams};
use tandem_core::wire::{
    ArchivedView, CommentList, CommentResult, CommentView, LaneDeletionView, LaneList,
    NoteMoveView, NotePathView, NoteWriteView, ProjectList, SprintList, SprintResult, TicketDetail,
    TicketList, TicketSummary, WorkflowRunList, WorkflowRunResult,
};

// MARK: - The grammar

/// The root: `--home` on every verb, and the group the run names.
///
/// `--help` is the CLI's own flag, not clap's: the texts in [`usage`] are the
/// Swift binary's, and a generator's layout would be a second grammar to keep.
#[derive(Parser)]
#[command(
    name = "tandem",
    version,
    about = "Tandem: the board, the bikeshed and the records",
    disable_help_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// The home this run keeps its state in (default: $TANDEM_HOME, else ~/.tandem).
    #[arg(long, global = true, value_name = "PATH")]
    home: Option<PathBuf>,

    /// Print the usage text for this verb (the Swift binary's own).
    #[arg(short = 'h', long = "help", global = true, action = clap::ArgAction::SetTrue)]
    help: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// The board's ticket surface.
    Ticket {
        #[command(subcommand)]
        command: Option<TicketCommand>,
    },
    /// The lane lifecycle.
    Lane {
        #[command(subcommand)]
        command: Option<LaneCommand>,
    },
    /// The project lifecycle: a lane's parent.
    Project {
        #[command(subcommand)]
        command: Option<ProjectCommand>,
    },
    /// The bikeshed's agent face.
    Note {
        #[command(subcommand)]
        command: Option<NoteCommand>,
    },
    /// The sprint record.
    Sprint {
        #[command(subcommand)]
        command: Option<SprintCommand>,
    },
    /// The workflow: the stage set the board reads (TAN-328).
    Workflow {
        #[command(subcommand)]
        command: Option<WorkflowCommand>,
    },
    /// The store's settings: the roster the pickers offer and the shared focus
    /// the faces draw (TAN-258).
    Settings {
        #[command(subcommand)]
        command: Option<SettingsCommand>,
    },
    /// Report whether this machine's store and notes are sound (ADR 0014).
    Doctor,
    /// Speak the app's JSON protocol on stdin and stdout (PRT-11).
    Rpc,
    /// Serve the agent face as an MCP server over stdio (TAN-274).
    Mcp,
}

#[derive(Subcommand)]
enum TicketCommand {
    List(ListArgs),
    Show(ShowArgs),
    Add(AddArgs),
    File(FileArgs),
    Set(SetArgs),
    Move(MoveArgs),
    Block(BlockArgs),
    Unblock(UnblockArgs),
    Complete(IdArgs),
    Archive(ArchiveArgs),
    Comment {
        #[command(subcommand)]
        command: Option<CommentCommand>,
    },
}

#[derive(Subcommand)]
enum CommentCommand {
    Add(CommentAddArgs),
    List(IdArgs),
}

#[derive(Subcommand)]
enum LaneCommand {
    List,
    Add(LaneAddArgs),
    Rename(LaneRenameArgs),
    Delete(LaneDeleteArgs),
    Move(LaneMoveArgs),
}

#[derive(Subcommand)]
enum ProjectCommand {
    List,
    Add(ProjectAddArgs),
    Rename(ProjectRenameArgs),
    Delete(ProjectDeleteArgs),
}

#[derive(Subcommand)]
enum NoteCommand {
    Rename(NoteRenameArgs),
    Capture(NoteCaptureArgs),
    Stats(NoteStatsArgs),
    Prune,
    Links,
    Write(NoteWriteArgs),
    Move(NoteMoveArgs),
    Create(NoteCreateArgs),
    CreateFolder(NoteCreateFolderArgs),
    Daily,
}

#[derive(Subcommand)]
enum SprintCommand {
    Open(SprintOpenArgs),
    Ticket(SprintTicketArgs),
    Run(SprintRunArgs),
    Close(SprintCloseArgs),
    List(SprintListArgs),
    Show(SprintShowArgs),
}

#[derive(Subcommand)]
enum WorkflowCommand {
    List,
    Show(WorkflowShowArgs),
    Set(WorkflowSetArgs),
    Use(WorkflowIdArgs),
    Delete(WorkflowIdArgs),
    /// The workflow's current run and its run history (TAN-332).
    Run {
        #[command(subcommand)]
        command: Option<WorkflowRunCommand>,
    },
}

#[derive(Subcommand)]
enum WorkflowRunCommand {
    Open(WorkflowRunOpenArgs),
    Ticket(WorkflowRunTicketArgs),
    Run(WorkflowRunRunArgs),
    Close(WorkflowRunCloseArgs),
    List(WorkflowRunListArgs),
    Show(WorkflowRunShowArgs),
}

#[derive(Args)]
struct WorkflowRunOpenArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    workflow: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    mode: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
}

#[derive(Args)]
struct WorkflowRunTicketArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    ticket: Option<String>,
}

#[derive(Args)]
struct WorkflowRunRunArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    note: Option<String>,
}

#[derive(Args)]
struct WorkflowRunCloseArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    number: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    note: Option<String>,
}

#[derive(Args)]
struct WorkflowRunListArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    workflow: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
}

#[derive(Args)]
struct WorkflowRunShowArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    number: Option<String>,
}

#[derive(Args)]
struct WorkflowShowArgs {
    /// The workflow's UUID or its name; omitted reads the active one.
    #[arg(long)]
    id: Option<String>,
}

#[derive(Args)]
struct WorkflowSetArgs {
    /// The workflow to update; omitted creates one.
    #[arg(long)]
    id: Option<String>,
}

#[derive(Args)]
struct WorkflowIdArgs {
    #[arg(long)]
    id: Option<String>,
}

#[derive(Args)]
struct ListArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    filter: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    text: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    assignee: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    /// The archive as its own view, or joining a named filter.
    #[arg(long)]
    archived: bool,
    /// Only tickets waiting on an open blocker.
    #[arg(long)]
    blocked: bool,
}

#[derive(Args)]
struct FileArgs {
    /// The note the text came from (bikeshed-root-relative).
    #[arg(long = "from-note", num_args = 0..=1, default_missing_value = "")]
    from_note: Option<String>,
    /// The selection; `-` or an absent flag reads stdin.
    #[arg(long, num_args = 0..=1, default_missing_value = "", allow_hyphen_values = true)]
    text: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    assignee: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    origin: Option<String>,
}

#[derive(Args)]
struct ShowArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
}

#[derive(Args)]
struct AddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    title: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    assignee: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    origin: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    description: Option<String>,
}

#[derive(Args)]
struct SetArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    title: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    assignee: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    description: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    append_description: Option<String>,
}

#[derive(Args)]
struct MoveArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    index: Option<String>,
}

#[derive(Args)]
struct BlockArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    on: Option<String>,
}

#[derive(Args)]
struct UnblockArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    on: Option<String>,
}

#[derive(Args)]
struct IdArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
}

#[derive(Args)]
struct ArchiveArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
}

#[derive(Args)]
struct CommentAddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    author: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    body: Option<String>,
}

#[derive(Args)]
struct LaneAddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    abbr: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Args)]
struct LaneRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    abbr: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Args)]
struct LaneDeleteArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
}

#[derive(Args)]
struct LaneMoveArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    index: Option<String>,
}

#[derive(Args)]
struct ProjectAddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
}

#[derive(Args)]
struct ProjectRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
}

#[derive(Args)]
struct ProjectDeleteArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
}

#[derive(Args)]
struct NoteRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
}

#[derive(Args)]
struct NoteWriteArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    path: Option<String>,
    /// The whole text; `-` or an absent flag reads stdin.
    #[arg(long, num_args = 0..=1, default_missing_value = "", allow_hyphen_values = true)]
    text: Option<String>,
}

#[derive(Args)]
struct NoteMoveArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    path: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    into: Option<String>,
}

#[derive(Args)]
struct NoteCreateArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    path: Option<String>,
}

#[derive(Args)]
struct NoteCreateFolderArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    path: Option<String>,
}

#[derive(Args)]
struct NoteCaptureArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    kind: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
}

#[derive(Args)]
struct NoteStatsArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Subcommand)]
enum SettingsCommand {
    Get,
    Set(SettingsSetArgs),
}

#[derive(Args)]
struct SettingsSetArgs {
    /// One roster name per flag; the spelled flags replace the stored list
    /// whole, and a bare flag stores the empty list. Unspelled leaves the
    /// stored list alone.
    #[arg(long, num_args = 0..)]
    roster: Option<Vec<String>>,
    /// The project to focus; a spelled flag with a name focuses it, and a bare
    /// or empty flag clears back to every project. Unspelled leaves the focus
    /// alone, unless no flag is spelled at all: a bare `settings set` clears
    /// the focus and leaves the roster alone.
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Args)]
struct SprintOpenArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    mode: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
}

#[derive(Args)]
struct SprintTicketArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    ticket: Option<String>,
}

#[derive(Args)]
struct SprintRunArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    note: Option<String>,
}

#[derive(Args)]
struct SprintCloseArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    number: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    note: Option<String>,
}

#[derive(Args)]
struct SprintListArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
}

#[derive(Args)]
struct SprintShowArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    lane: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    number: Option<String>,
}

// MARK: - The process

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if error.kind() == ErrorKind::DisplayVersion {
                print!("{error}");
                return ExitCode::SUCCESS;
            }
            eprintln!("{}", usage::ROOT);
            return ExitCode::from(2);
        }
    };
    if cli.help {
        println!("{}", help_text(&cli));
        return ExitCode::SUCCESS;
    }
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if !failure.message.is_empty() {
                eprintln!("{}", failure.message);
            }
            ExitCode::from(failure.exit)
        }
    }
}

/// One failure, in the words the stream it goes to carries. A domain failure is
/// exit 1; a usage failure is exit 2 with the root text.
struct Failure {
    message: String,
    exit: u8,
}

impl Failure {
    fn domain(error: DomainError) -> Self {
        Self {
            message: domain_text(&error),
            exit: 1,
        }
    }

    fn usage() -> Self {
        Self {
            message: usage::ROOT.to_string(),
            exit: 2,
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit: 1,
        }
    }
}

/// The text a domain failure prints. An unknown id appends the open list in the
/// `list` shape, so the caller recovers in one turn (`StoreEncoding.openList`).
fn domain_text(error: &DomainError) -> String {
    let DomainError::UnknownId { open, .. } = error else {
        return error.message();
    };
    let list = TicketList {
        tickets: open.iter().map(TicketSummary::new).collect(),
    };
    match serde_json::to_string(&list) {
        Ok(json) => format!("{}\n{json}", error.message()),
        Err(_) => error.message(),
    }
}

/// The `--help` text for the deepest verb the run named.
fn help_text(cli: &Cli) -> &'static str {
    let name = match &cli.command {
        None => return usage::ROOT,
        Some(Command::Doctor) => return usage::verb("doctor").unwrap_or(usage::ROOT),
        Some(Command::Rpc) => return usage::RPC,
        Some(Command::Mcp) => return usage::MCP,
        Some(Command::Ticket { command }) => match command {
            None => return usage::group("ticket").unwrap_or(usage::ROOT),
            Some(TicketCommand::Comment { command: None }) => "ticket comment",
            Some(TicketCommand::Comment {
                command: Some(CommentCommand::Add(_)),
            }) => "ticket comment add",
            Some(TicketCommand::Comment {
                command: Some(CommentCommand::List(_)),
            }) => "ticket comment list",
            Some(command) => command.help(),
        },
        Some(Command::Lane { command }) => match command {
            None => return usage::group("lane").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Project { command }) => match command {
            None => return usage::group("project").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Note { command }) => match command {
            None => return usage::group("note").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Sprint { command }) => match command {
            None => return usage::group("sprint").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Workflow {
            command: Some(WorkflowCommand::Run { command }),
        }) => match command {
            None => return usage::group("workflow run").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Workflow { command }) => match command {
            None => return usage::group("workflow").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Settings { command }) => match command {
            None => return usage::group("settings").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
    };
    usage::verb(name).unwrap_or(usage::ROOT)
}

impl TicketCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List(_) => "ticket list",
            Self::Show(_) => "ticket show",
            Self::Add(_) => "ticket add",
            Self::File(_) => "ticket file",
            Self::Set(_) => "ticket set",
            Self::Move(_) => "ticket move",
            Self::Block(_) => "ticket block",
            Self::Unblock(_) => "ticket unblock",
            Self::Complete(_) => "ticket complete",
            Self::Archive(_) => "ticket archive",
            Self::Comment { .. } => "ticket comment",
        }
    }
}

impl LaneCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List => "lane list",
            Self::Add(_) => "lane add",
            Self::Rename(_) => "lane rename",
            Self::Delete(_) => "lane delete",
            Self::Move(_) => "lane move",
        }
    }
}

impl ProjectCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List => "project list",
            Self::Add(_) => "project add",
            Self::Rename(_) => "project rename",
            Self::Delete(_) => "project delete",
        }
    }
}

impl NoteCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Rename(_) => "note rename",
            Self::Capture(_) => "note capture",
            Self::Stats(_) => "note stats",
            Self::Prune => "note prune",
            Self::Links => "note links",
            Self::Write(_) => "note write",
            Self::Move(_) => "note move",
            Self::Create(_) => "note create",
            Self::CreateFolder(_) => "note create-folder",
            Self::Daily => "note daily",
        }
    }
}

impl SettingsCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Get => "settings get",
            Self::Set(_) => "settings set",
        }
    }
}

impl SprintCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Open(_) => "sprint open",
            Self::Ticket(_) => "sprint ticket",
            Self::Run(_) => "sprint run",
            Self::Close(_) => "sprint close",
            Self::List(_) => "sprint list",
            Self::Show(_) => "sprint show",
        }
    }
}

impl WorkflowCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List => "workflow list",
            Self::Show(_) => "workflow show",
            Self::Set(_) => "workflow set",
            Self::Use(_) => "workflow use",
            Self::Delete(_) => "workflow delete",
            Self::Run { .. } => "workflow run",
        }
    }
}

impl WorkflowRunCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Open(_) => "workflow run open",
            Self::Ticket(_) => "workflow run ticket",
            Self::Run(_) => "workflow run run",
            Self::Close(_) => "workflow run close",
            Self::List(_) => "workflow run list",
            Self::Show(_) => "workflow run show",
        }
    }
}

// MARK: - Dispatch

fn run(cli: &Cli) -> Result<(), Failure> {
    let user_home = store::user_home().map_err(|error| Failure::message(error.to_string()))?;
    let environment = env::var(store::ENVIRONMENT).ok();
    let home = Home::resolve(cli.home.as_deref(), environment.as_deref(), &user_home);
    let command = cli.command.as_ref().ok_or_else(Failure::usage)?;

    match command {
        Command::Doctor => doctor(&home),
        Command::Rpc => rpc(&home),
        Command::Mcp => mcp(&home),
        Command::Ticket { command } => ticket(&home, command.as_ref()),
        Command::Lane { command } => lane(&home, command.as_ref()),
        Command::Project { command } => project(&home, command.as_ref()),
        Command::Note { command } => note(&home, command.as_ref()),
        Command::Sprint { command } => sprint(&home, command.as_ref()),
        Command::Workflow { command } => workflow(&home, command.as_ref()),
        Command::Settings { command } => settings_verb(&home, command.as_ref()),
    }
}

fn ticket(home: &Home, command: Option<&TicketCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(
        command,
        TicketCommand::List(_)
            | TicketCommand::Show(_)
            | TicketCommand::Comment {
                command: Some(CommentCommand::List(_))
            }
    ) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let board = Board::new(store);

    match command {
        TicketCommand::List(args) => {
            let assignee = resolve_assignee(args.assignee.as_deref())?;
            let query = TicketQuery::parse(
                args.filter.as_deref(),
                args.text.as_deref(),
                args.status.as_deref(),
                assignee.as_deref(),
                args.lane.as_deref(),
                args.archived,
                args.blocked,
            )
            .map_err(Failure::domain)?;
            if args.status.is_some() {
                for status in &query.statuses {
                    board.validate_status(status).map_err(Failure::domain)?;
                }
            }
            let tickets = board.list(&query).map_err(Failure::domain)?;
            print(&TicketList {
                tickets: tickets.iter().map(TicketSummary::new).collect(),
            })
        }
        TicketCommand::Show(args) => {
            let ticket = board
                .ticket(args.id.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Add(args) => {
            let status = match args.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(Failure::domain)?,
                None => board.landing_status().map_err(Failure::domain)?,
            };
            let assignee = resolve_assignee(args.assignee.as_deref())?;
            let origin = resolve_origin(args.origin.as_deref())?;
            let description = add_description_body(args.description.as_deref())?;
            let ticket = board
                .add(
                    args.title.as_deref().unwrap_or(""),
                    AddTicket {
                        lane: args.lane.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: description.as_deref(),
                        origin: &origin,
                    },
                )
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::File(args) => {
            let text = match args.text.as_deref() {
                None | Some("-") => read_standard_input()?,
                Some(text) => text.to_string(),
            };
            let status = match args.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(Failure::domain)?,
                None => board.landing_status().map_err(Failure::domain)?,
            };
            let assignee = resolve_assignee(args.assignee.as_deref())?;
            let origin = resolve_origin(args.origin.as_deref())?;
            let ticket = board
                .add_from_note(
                    &text,
                    args.from_note.as_deref().unwrap_or(""),
                    AddTicket {
                        lane: args.lane.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: None,
                        origin: &origin,
                    },
                )
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Set(args) => {
            if args.description.is_some() && args.append_description.is_some() {
                return Err(Failure::domain(DomainError::ConflictingDescriptionEdit));
            }
            let status = match args.status.as_deref() {
                Some(raw) => Edit::Set(Status::parse(raw).map_err(Failure::domain)?),
                None => Edit::Unchanged,
            };
            let assignee = match args.assignee.as_deref() {
                Some(raw) => Edit::Set(resolve_assignee(Some(raw))?),
                None => Edit::Unchanged,
            };
            let description = match args.description.as_deref() {
                Some(raw) => Edit::Set(Some(description_body(raw)?)),
                None => Edit::Unchanged,
            };
            let append_description = match args.append_description.as_deref() {
                Some(raw) => Edit::Set(append_description_body(raw)?),
                None => Edit::Unchanged,
            };
            let ticket = board
                .set(
                    args.id.as_deref().unwrap_or(""),
                    SetFields {
                        title: match args.title.as_deref() {
                            Some(title) => Edit::Set(title),
                            None => Edit::Unchanged,
                        },
                        status,
                        assignee: match &assignee {
                            Edit::Set(name) => Edit::Set(name.as_deref()),
                            Edit::Unchanged => Edit::Unchanged,
                        },
                        description: match &description {
                            Edit::Set(body) => Edit::Set(body.as_deref()),
                            Edit::Unchanged => Edit::Unchanged,
                        },
                        append_description: match &append_description {
                            Edit::Set(body) => Edit::Set(body.as_str()),
                            Edit::Unchanged => Edit::Unchanged,
                        },
                        archived: Edit::Unchanged,
                    },
                )
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Move(args) => {
            let at = match args.index.as_deref() {
                None => usize::MAX,
                Some(raw) => match raw.parse::<i64>() {
                    Ok(index) => index.max(0) as usize,
                    Err(_) => {
                        return Err(Failure::domain(DomainError::InvalidIndex(raw.to_string())));
                    }
                },
            };
            let ticket = board
                .move_ticket(args.id.as_deref().unwrap_or(""), args.lane.as_deref(), at)
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Block(args) => {
            let ticket = board
                .block(
                    args.id.as_deref().unwrap_or(""),
                    args.on.as_deref().unwrap_or(""),
                )
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Unblock(args) => {
            let ticket = board
                .unblock(args.id.as_deref().unwrap_or(""), args.on.as_deref())
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Complete(args) => {
            let ticket = board
                .complete(args.id.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&TicketDetail::new(&ticket))
        }
        TicketCommand::Archive(args) => match args.id.as_deref() {
            Some(id) => {
                let ticket = board.archive(id).map_err(Failure::domain)?;
                print(&TicketDetail::new(&ticket))
            }
            None => {
                let archived = board.archive_done().map_err(Failure::domain)?;
                print(&ArchivedView { archived })
            }
        },
        TicketCommand::Comment { command } => {
            let command = command.as_ref().ok_or_else(Failure::usage)?;
            match command {
                CommentCommand::Add(args) => {
                    let body = match args.body.as_deref() {
                        Some("-") => read_standard_input()?,
                        Some(body) => body.to_string(),
                        None => String::new(),
                    };
                    let comment = board
                        .append_comment(
                            args.id.as_deref().unwrap_or(""),
                            args.author.as_deref().unwrap_or(""),
                            &body,
                        )
                        .map_err(Failure::domain)?;
                    print(&CommentResult {
                        comment: CommentView::new(&comment),
                        note: None,
                    })
                }
                CommentCommand::List(args) => {
                    let comments = board
                        .comments(args.id.as_deref().unwrap_or(""))
                        .map_err(Failure::domain)?;
                    print(&CommentList {
                        comments: comments.iter().map(CommentView::new).collect(),
                    })
                }
            }
        }
    }
}

fn lane(home: &Home, command: Option<&LaneCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, LaneCommand::List) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let board = Board::new(store);

    match command {
        LaneCommand::List => print(&LaneList {
            lanes: board.lanes().map_err(Failure::domain)?,
            note: None,
        }),
        LaneCommand::Add(args) => {
            let lanes = board
                .add_lane(
                    args.name.as_deref().unwrap_or(""),
                    args.abbr.as_deref(),
                    args.project.as_deref(),
                )
                .map_err(Failure::domain)?;
            print(&LaneList { lanes, note: None })
        }
        LaneCommand::Rename(args) => {
            let lanes = board
                .rename_lane(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                    args.abbr.as_deref(),
                    args.project.as_deref(),
                )
                .map_err(Failure::domain)?;
            print(&LaneList { lanes, note: None })
        }
        LaneCommand::Delete(args) => {
            let deletion = board
                .delete_lane(args.name.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&LaneDeletionView::new(&deletion, None))
        }
        LaneCommand::Move(args) => {
            let at = match args.index.as_deref() {
                None => usize::MAX,
                Some(raw) => match raw.parse::<i64>() {
                    Ok(index) => index.max(0) as usize,
                    Err(_) => {
                        return Err(Failure::domain(DomainError::InvalidIndex(raw.to_string())));
                    }
                },
            };
            let lanes = board
                .move_lane(args.name.as_deref().unwrap_or(""), at)
                .map_err(Failure::domain)?;
            print(&LaneList { lanes, note: None })
        }
    }
}

fn project(home: &Home, command: Option<&ProjectCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, ProjectCommand::List) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let board = Board::new(store);

    match command {
        ProjectCommand::List => {
            let projects = board.projects().map_err(Failure::domain)?;
            print(&ProjectList::new(&projects, None))
        }
        ProjectCommand::Add(args) => {
            let projects = board
                .add_project(args.name.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&ProjectList::new(&projects, None))
        }
        ProjectCommand::Rename(args) => {
            let projects = board
                .rename_project(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                )
                .map_err(Failure::domain)?;
            print(&ProjectList::new(&projects, None))
        }
        ProjectCommand::Delete(args) => {
            let projects = board
                .delete_project(args.name.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&ProjectList::new(&projects, None))
        }
    }
}

/// The store's settings: the roster the pickers offer and the shared focus
/// the faces draw (TAN-258). `get` is the read the app makes over `rpc`;
/// `set` is its write, beside `settings.get` in the method table, so agents
/// read and write the same values the faces do. Each `set` flag spells one
/// axis: an unspelled axis is left alone, except a bare `settings set`, which
/// clears the focus and leaves the roster alone.
fn settings_verb(home: &Home, command: Option<&SettingsCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, SettingsCommand::Get) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let settings = Settings::new(&store);
    let board = Board::new(store);

    match command {
        SettingsCommand::Get => {
            let projects = project_names(&board)?;
            print(&SettingsReply {
                roster: settings.roster().map_err(Failure::domain)?,
                active_project: settings
                    .active_project(&projects)
                    .map_err(Failure::domain)?,
            })
        }
        SettingsCommand::Set(args) => {
            let projects = project_names(&board)?;
            let bare = args.roster.is_none() && args.project.is_none();
            let roster = match args.roster.clone() {
                Some(roster) => settings
                    .set_roster(board.actor(), roster)
                    .map_err(Failure::domain)?,
                None => settings.roster().map_err(Failure::domain)?,
            };
            let active_project = match args.project.as_deref() {
                Some(project) => settings
                    .set_active_project(board.actor(), Some(project), &projects)
                    .map_err(Failure::domain)?,
                None if bare => settings
                    .set_active_project(board.actor(), None, &projects)
                    .map_err(Failure::domain)?,
                None => settings
                    .active_project(&projects)
                    .map_err(Failure::domain)?,
            };
            print(&SettingsReply {
                roster,
                active_project,
            })
        }
    }
}

/// The board's project names in render order: the focus resolves against
/// these, not against lane names (UI-33), so a stored lane reads as no focus.
fn project_names(board: &Board) -> Result<Vec<String>, Failure> {
    Ok(board
        .projects()
        .map_err(Failure::domain)?
        .into_iter()
        .map(|project| project.name)
        .collect())
}

/// The workflow verbs: the stage set as data (TAN-328). `list` and `show` read;
/// `set` writes a whole document from stdin, atomically; `use` activates one;
/// `delete` removes a workflow no ticket or sprint is using.
fn workflow(home: &Home, command: Option<&WorkflowCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let read_only = match command {
        WorkflowCommand::List | WorkflowCommand::Show(_) => true,
        WorkflowCommand::Run { command } => matches!(
            command,
            None | Some(WorkflowRunCommand::List(_)) | Some(WorkflowRunCommand::Show(_))
        ),
        _ => false,
    };
    let store = if read_only {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let workflows = Workflows::new(&store);
    let actor = Actor::current();
    let revision = || store.last_revision();

    match command {
        WorkflowCommand::List => {
            let (list, revision) = workflows.list_snapshot().map_err(Failure::domain)?;
            let active = Workflows::new(&store).active().map_err(Failure::domain)?.id;
            print(&WorkflowListReply {
                revision,
                workflows: list,
                active,
            })
        }
        WorkflowCommand::Run { command } => {
            let command = command.as_ref().ok_or_else(Failure::usage)?;
            workflow_run(&store, command)
        }
        WorkflowCommand::Show(args) => {
            let (workflow, revision) = match args.id.as_deref() {
                Some(id) => workflows.get_snapshot(id).map_err(Failure::domain)?,
                None => workflows.active_snapshot().map_err(Failure::domain)?,
            };
            print(&WorkflowReply { revision, workflow })
        }
        WorkflowCommand::Set(args) => {
            let text = read_standard_input()?;
            let parsed: WorkflowSetParams = serde_json::from_str(&text).map_err(|error| {
                Failure::message(format!("The workflow document is not JSON: {error}."))
            })?;
            let draft = WorkflowDraft {
                name: parsed.name.unwrap_or_default(),
                scope: parsed.scope.unwrap_or_else(|| "home".to_string()),
                stages: parsed.stages,
            };
            let target = args.id.as_deref().or(parsed.id.as_deref());
            let workflow = workflows
                .set(&actor, target, draft)
                .map_err(Failure::domain)?;
            print(&WorkflowReply {
                revision: revision(),
                workflow,
            })
        }
        WorkflowCommand::Use(args) => {
            let workflow = workflows
                .use_workflow(&actor, args.id.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&WorkflowReply {
                revision: revision(),
                workflow,
            })
        }
        WorkflowCommand::Delete(args) => {
            let workflows = workflows
                .delete(&actor, args.id.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            let active = Workflows::new(&store).active().map_err(Failure::domain)?.id;
            print(&WorkflowListReply {
                revision: revision(),
                workflows,
                active,
            })
        }
    }
}

/// The workflow run verbs (TAN-332): the workflow's current run and its
/// history, on the same records the `sprint` aliases write.
fn workflow_run(store: &Store, command: &WorkflowRunCommand) -> Result<(), Failure> {
    let sprints = Sprints::new(store);
    let actor = Actor::current();

    match command {
        WorkflowRunCommand::Open(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let mode = sprint_mode(args.mode.as_deref())?;
            let session = session_pointer(args.session.as_deref());
            let result = sprints
                .open_run(
                    &actor,
                    lane,
                    args.workflow.as_deref(),
                    mode,
                    session.as_deref(),
                    None,
                )
                .map_err(Failure::domain)?;
            print(&WorkflowRunResult {
                run: result.value,
                note: result.note,
            })
        }
        WorkflowRunCommand::Ticket(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let ticket = Board::new(store.clone())
                .ticket(args.ticket.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            let number = sprints.open_run_in(lane).map_err(Failure::domain)?.number;
            let result = sprints
                .ticket(&actor, lane, number, &ticket.id.to_string())
                .map_err(Failure::domain)?;
            print(&WorkflowRunResult {
                run: result.value.into(),
                note: result.note,
            })
        }
        WorkflowRunCommand::Run(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let note = sprint_note(args.note.as_deref().unwrap_or(""))?;
            let number = sprints.open_run_in(lane).map_err(Failure::domain)?.number;
            let result = sprints
                .run(&actor, lane, number, &note, None)
                .map_err(Failure::domain)?;
            print(&WorkflowRunResult {
                run: result.value.into(),
                note: result.note,
            })
        }
        WorkflowRunCommand::Close(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let note = match args.note.as_deref() {
                Some(raw) => Some(sprint_note(raw)?),
                None => None,
            };
            let number = match args.number.as_deref() {
                Some(raw) => sprint_number(raw)?,
                None => sprints.close_in(lane).map_err(Failure::domain)?.number,
            };
            let result = sprints
                .close(&actor, lane, number, note.as_deref(), None)
                .map_err(Failure::domain)?;
            print(&WorkflowRunResult {
                run: result.value.into(),
                note: result.note,
            })
        }
        WorkflowRunCommand::List(args) => {
            let status = match args.status.as_deref() {
                Some(raw) => Some(
                    tandem_core::domain::sprint::SprintStatus::parse(raw).ok_or_else(|| {
                        Failure::domain(DomainError::InvalidSprintStatus(raw.to_string()))
                    })?,
                ),
                None => None,
            };
            let runs = sprints
                .list_runs(args.lane.as_deref(), args.workflow.as_deref(), status)
                .map_err(Failure::domain)?;
            print(&WorkflowRunList { runs })
        }
        WorkflowRunCommand::Show(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let run = match args.number.as_deref() {
                Some(raw) => sprints.show_run(lane, sprint_number(raw)?),
                None => sprints.open_run_in(lane),
            }
            .map_err(Failure::domain)?;
            print(&WorkflowRunResult { run, note: None })
        }
    }
}

fn note(home: &Home, command: Option<&NoteCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let vault = bikeshed(home);

    match command {
        NoteCommand::Rename(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let result = vault
                .rename(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                )
                .map_err(Failure::domain)?;
            print(&NotePathView {
                path: result.value,
                note: result.note,
            })
        }
        NoteCommand::Capture(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let kind =
                CaptureKind::parse(args.kind.as_deref().unwrap_or("")).map_err(capture_failure)?;
            let project = args.project.clone().unwrap_or_else(project_default);
            let session = args.session.clone().unwrap_or_else(capture_session_default);
            let body = read_standard_input()?;
            match vault.capture(&project, kind, &body, &session, SystemTime::now()) {
                Ok(result) => print(&NotePathView {
                    path: result.value,
                    note: result.note,
                }),
                Err(error) => Err(capture_failure(error)),
            }
        }
        NoteCommand::Stats(args) => {
            let stats = vault
                .stats(args.project.as_deref())
                .map_err(Failure::domain)?;
            print(&stats.value)
        }
        NoteCommand::Prune => {
            let pruned = vault.prune().map_err(Failure::domain)?;
            print(&pruned.value)
        }
        NoteCommand::Links => {
            let health = vault.health().map_err(Failure::domain)?;
            print(&health.value)
        }
        NoteCommand::Write(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let path = args.path.as_deref().unwrap_or("");
            let text = match args.text.as_deref() {
                None | Some("-") => read_standard_input()?,
                Some(text) => text.to_string(),
            };
            let record = vault.write(path, &text).map_err(Failure::domain)?;
            print(&NoteWriteView::new(&record.value))
        }
        NoteCommand::Move(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let outcome = vault
                .move_into(
                    args.path.as_deref().unwrap_or(""),
                    args.into.as_deref().unwrap_or(""),
                )
                .map_err(Failure::domain)?;
            print(&NoteMoveView::new(&outcome.value))
        }
        NoteCommand::Create(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let result = vault
                .create(args.path.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&NotePathView {
                path: result.value,
                note: result.note,
            })
        }
        NoteCommand::CreateFolder(args) => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let result = vault
                .create_folder(args.path.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&NotePathView {
                path: result.value,
                note: result.note,
            })
        }
        NoteCommand::Daily => {
            home.ensure()
                .map_err(|error| home_failure(home.directory(), error))?;
            let result = vault
                .open_daily_note(Day::today())
                .map_err(Failure::domain)?;
            print(&NotePathView {
                path: result.value,
                note: result.note,
            })
        }
    }
}

/// A refused capture is the caller's own text to fix, not a retryable domain
/// failure: an empty body or a slug with no words exits 2, the usage code.
fn capture_failure(error: DomainError) -> Failure {
    let refused = matches!(
        error,
        DomainError::EmptyCaptureBody | DomainError::EmptyCaptureSlug(_)
    );
    let mut failure = Failure::domain(error);
    if refused {
        failure.exit = 2;
    }
    failure
}

fn sprint(home: &Home, command: Option<&SprintCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, SprintCommand::List(_) | SprintCommand::Show(_)) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let sprints = Sprints::new(&store);
    let actor = Actor::current();

    match command {
        SprintCommand::Open(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let mode = sprint_mode(args.mode.as_deref())?;
            let session = session_pointer(args.session.as_deref());
            let result = sprints
                .open(&actor, lane, mode, session.as_deref(), None)
                .map_err(Failure::domain)?;
            print(&SprintResult {
                sprint: result.value,
                note: result.note,
            })
        }
        SprintCommand::Ticket(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let ticket = Board::new(store.clone())
                .ticket(args.ticket.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            let number = sprints.open_in(lane).map_err(Failure::domain)?.number;
            let result = sprints
                .ticket(&actor, lane, number, &ticket.id.to_string())
                .map_err(Failure::domain)?;
            print(&SprintResult {
                sprint: result.value,
                note: result.note,
            })
        }
        SprintCommand::Run(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let note = sprint_note(args.note.as_deref().unwrap_or(""))?;
            let number = sprints.open_in(lane).map_err(Failure::domain)?.number;
            let result = sprints
                .run(&actor, lane, number, &note, None)
                .map_err(Failure::domain)?;
            print(&SprintResult {
                sprint: result.value,
                note: result.note,
            })
        }
        SprintCommand::Close(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let note = match args.note.as_deref() {
                Some(raw) => Some(sprint_note(raw)?),
                None => None,
            };
            let number = match args.number.as_deref() {
                Some(raw) => sprint_number(raw)?,
                None => sprints.close_in(lane).map_err(Failure::domain)?.number,
            };
            let result = sprints
                .close(&actor, lane, number, note.as_deref(), None)
                .map_err(Failure::domain)?;
            print(&SprintResult {
                sprint: result.value,
                note: result.note,
            })
        }
        SprintCommand::List(args) => {
            let status = match args.status.as_deref() {
                Some(raw) => Some(
                    tandem_core::domain::sprint::SprintStatus::parse(raw).ok_or_else(|| {
                        Failure::domain(DomainError::InvalidSprintStatus(raw.to_string()))
                    })?,
                ),
                None => None,
            };
            let records = sprints
                .list(args.lane.as_deref(), status)
                .map_err(Failure::domain)?;
            print(&SprintList { sprints: records })
        }
        SprintCommand::Show(args) => {
            let record = sprints
                .show(
                    args.lane.as_deref().unwrap_or(""),
                    sprint_number(args.number.as_deref().unwrap_or(""))?,
                )
                .map_err(Failure::domain)?;
            print(&SprintResult {
                sprint: record,
                note: None,
            })
        }
    }
}

// MARK: - Plumbing

fn print<T: serde::Serialize>(value: &T) -> Result<(), Failure> {
    let line = serde_json::to_string(value).map_err(|error| Failure::message(error.to_string()))?;
    println!("{line}");
    Ok(())
}

fn write_store(home: &Home) -> Result<Store, Failure> {
    home.ensure()
        .map_err(|error| home_failure(home.directory(), error))?;
    Ok(Store::new(home.clone()))
}

fn home_failure(path: &Path, error: io::Error) -> Failure {
    Failure::message(format!(
        "Cannot create the Tandem home at {}: {error}. Try again.",
        path.display()
    ))
}

fn bikeshed(home: &Home) -> Bikeshed {
    Bikeshed::new(home.directory().join("bikeshed"))
}

/// Everything on stdin, as UTF-8. The stdin bodies are the verbs' multi-line
/// arguments: a pipe is the only spelling that survives a heredoc, a quote and a
/// shell's own trimming.
fn read_standard_input() -> Result<String, Failure> {
    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|error| Failure::message(format!("Cannot read stdin: {error}. Try again.")))?;
    String::from_utf8(bytes).map_err(|_| Failure::domain(DomainError::StandardInputNotUtf8))
}

/// The working directory's last component: the project a capture or session
/// defaults to (ADR 0016).
fn project_default() -> String {
    env::current_dir()
        .ok()
        .and_then(|directory| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

/// A capture's session default: `$OPENCODE_SESSION_ID`, else a short generated
/// token for hosts that export none (ADR 0016). A note is filed by path, so an
/// invented folder is fine here; a session record's handle is not.
fn capture_session_default() -> String {
    env::var("OPENCODE_SESSION_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()[..8].to_string())
}

/// `--assignee self` is the agent running this command: the core's rule, shared
/// with the wire's `ticket.list` and `ticket.add`, so the same value cannot mean
/// two things. Any other value passes through, including `""`, which selects the
/// unowned tickets.
fn resolve_assignee(raw: Option<&str>) -> Result<Option<String>, Failure> {
    tandem_core::domain::resolve_assignee(raw, &Actor::current()).map_err(Failure::domain)
}

/// `--origin self` is the agent; an absent flag is the human. The core owns the
/// rule, the same one the wire calls.
fn resolve_origin(raw: Option<&str>) -> Result<String, Failure> {
    tandem_core::domain::resolve_origin(raw, &Actor::current()).map_err(Failure::domain)
}

/// `--description` on `ticket set`: the flag's own value, or stdin when it is
/// `-`. A stdin body must carry text; `--description ""` is the deliberate clear.
fn description_body(flag: &str) -> Result<String, Failure> {
    if flag != "-" {
        return Ok(flag.to_string());
    }
    let body = read_standard_input()?;
    if body.trim().is_empty() {
        return Err(Failure::domain(DomainError::EmptyDescriptionBody));
    }
    Ok(body)
}

/// `--append-description` on `ticket set`: the addition's own value, or stdin
/// when it is `-`. A named addition must carry text on either path: an empty
/// pipe is a failed write, not a silent no-op. The refusal is the core's
/// (`DescriptionBody::required`), the same one `ticket add --description` makes.
fn append_description_body(flag: &str) -> Result<String, Failure> {
    if flag != "-" {
        return DescriptionBody::required(flag).map_err(Failure::domain);
    }
    let body = read_standard_input()?;
    DescriptionBody::required(&body).map_err(Failure::domain)
}

/// `--description` on `ticket add`: no flag is no description, and a named flag
/// must carry text. A new ticket has no earlier body to clear. The refusal is
/// the core's (`DescriptionBody::required`), the same one the wire's
/// `ticket.add` makes.
fn add_description_body(flag: Option<&str>) -> Result<Option<String>, Failure> {
    match flag {
        None => Ok(None),
        Some("-") => {
            let body = read_standard_input()?;
            Ok(Some(
                DescriptionBody::required(&body).map_err(Failure::domain)?,
            ))
        }
        Some(body) => Ok(Some(
            DescriptionBody::required(body).map_err(Failure::domain)?,
        )),
    }
}

/// A sprint's session pointer: the caller's value, else
/// `$OPENCODE_SESSION_ID`; a blank value is no pointer.
fn session_pointer(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|session| !session.is_empty())
        .map(str::to_string)
        .or_else(|| {
            env::var("OPENCODE_SESSION_ID")
                .ok()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
        })
}

/// A sprint's mode: absent is `serial`.
fn sprint_mode(raw: Option<&str>) -> Result<SprintMode, Failure> {
    match raw {
        None => Ok(SprintMode::Serial),
        Some(raw) => SprintMode::parse(raw)
            .ok_or_else(|| Failure::domain(DomainError::InvalidSprintMode(raw.to_string()))),
    }
}

/// A sprint note: the flag's value, or stdin when it is `-`. The empty body is
/// the core's own refusal.
fn sprint_note(raw: &str) -> Result<String, Failure> {
    if raw == "-" {
        read_standard_input()
    } else {
        Ok(raw.to_string())
    }
}

fn sprint_number(raw: &str) -> Result<i64, Failure> {
    raw.parse::<i64>()
        .map_err(|_| Failure::domain(DomainError::InvalidSprintNumber(raw.to_string())))
}

// MARK: - rpc

/// The app's door: frames on stdin, frames on stdout, and an exit on a closed
/// stdin. The protocol lives in the core (`tandem_core::rpc`); this arm is only
/// the process that owns the two streams.
fn rpc(home: &Home) -> Result<(), Failure> {
    tandem_core::rpc::serve(
        tandem_core::rpc::StdioStream::new(),
        Store::new(home.clone()),
        Actor::current(),
    )
    .map_err(|error| Failure::message(format!("The rpc stream failed: {error}. Try again.")))
}

// MARK: - mcp

/// The agent host's door: MCP frames on stdin, MCP frames on stdout, and an
/// exit on a closed stdin. The protocol lives in the core (`tandem_core::mcp`);
/// this arm is only the process that owns the two streams. The home resolves
/// the way `rpc` does, and the actor is the pass's own (`TANDEM_AGENT`).
fn mcp(home: &Home) -> Result<(), Failure> {
    tandem_core::mcp::serve(
        tandem_core::rpc::StdioStream::new(),
        Store::new(home.clone()),
        Actor::current(),
        environment(),
    )
    .map_err(|error| Failure::message(format!("The mcp stream failed: {error}. Try again.")))
}

// MARK: - doctor

/// `doctor` (ADR 0014): a diagnostic report is not a domain call, so it does not go
/// through `run`. It always prints the report JSON to stdout, the report is the
/// success value even when a check fails, and exits 0 when every
/// failing-capable check passed, 1 otherwise, echoing each failing check's next
/// step to stderr. A read-only verb: it never creates the home it reports.
fn doctor(home: &Home) -> Result<(), Failure> {
    let report = diagnostics::run(home);
    print(&report)?;
    if report.ok {
        return Ok(());
    }
    let failures: Vec<String> = report
        .checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| format!("doctor: {} \u{2014} {}", check.name, check.detail))
        .collect();
    Err(Failure {
        message: failures.join("\n"),
        exit: 1,
    })
}

// MARK: - the run's environment

/// The process's environment as the MCP door reads it: `TANDEM_HOME`,
/// `TANDEM_AGENT` and the harness session pointer. A value that is not UTF-8 is
/// not one of those names and is dropped.
fn environment() -> Environment {
    tandem_core::env::process()
}

// MARK: - The wire's parity gate

/// The machine half of parity item 3 (`docs/port.md`): every method the wire
/// speaks names a CLI verb, and the grammar answers it. A mutation message
/// without a verb is a UI-only write, which the contract forbids. A read may name
/// no verb: it writes nothing, and `usage.report` is the one such method.
#[cfg(test)]
mod wire_parity {
    use super::*;

    #[test]
    fn every_wire_method_names_a_cli_verb() {
        for (tag, verb) in tandem_core::wire::rpc::METHODS {
            if verb.is_empty() {
                continue;
            }
            let argv = std::iter::once("tandem").chain(verb.split(' '));
            Cli::try_parse_from(argv).unwrap_or_else(|error| {
                panic!(
                    "the wire's `{tag}` names the CLI verb `{verb}`, which the grammar does not \
                     answer: {error}"
                )
            });
        }
    }
}
