//! The tandem binary: the agent face, and later the `rpc` the app holds.
//!
//! One binary (`docs/port.md`, Target shape): the verbs agents call and the
//! core the app trusts are the same executable, so they cannot drift. This rung
//! carries the whole grammar the skills speak (PRT-9): `ticket`, `lane`, `note`,
//! `session`, `sprint`, and `doctor`'s pinned report. `setup` and the seven other
//! doctor checks land with PRT-10, and `rpc` with PRT-11.
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
use std::time::{SystemTime, UNIX_EPOCH};

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use tandem_core::bikeshed::{Bikeshed, CaptureKind};
use tandem_core::diagnostics;
use tandem_core::domain::harness;
use tandem_core::domain::session::{
    HarnessSession, HarnessTransport, SessionOutcome, SessionTokens, Sessions, parse_skills,
};
use tandem_core::domain::sprint::{SprintMode, Sprints};
use tandem_core::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, SetFields, Settings, Status, TicketQuery,
};
use tandem_core::setup::{self, Environment};
use tandem_core::store::{self, Actor, Home, Store};
use tandem_core::wire::rpc::SettingsReply;
use tandem_core::wire::{
    ArchivedView, CommentList, CommentResult, CommentView, LaneDeletionView, LaneList,
    NoteMoveView, NotePathView, NoteWriteView, SessionDetailView, SessionList, SessionResult,
    SprintList, SprintResult, TicketDetail, TicketList, TicketSummary,
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
    /// The bikeshed's agent face.
    Note {
        #[command(subcommand)]
        command: Option<NoteCommand>,
    },
    /// The session record.
    Session {
        #[command(subcommand)]
        command: Option<SessionCommand>,
    },
    /// The sprint record.
    Sprint {
        #[command(subcommand)]
        command: Option<SprintCommand>,
    },
    /// The agent face: install, uninstall and inspect it (ADR 0034).
    Setup {
        #[command(subcommand)]
        command: Option<SetupCommand>,
    },
    /// The store's settings, starting with the assignee roster.
    Settings {
        #[command(subcommand)]
        command: Option<SettingsCommand>,
    },
    /// Report whether this machine's Tandem setup is sound (ADR 0014).
    Doctor,
    /// Speak the app's JSON protocol on stdin and stdout (PRT-11).
    Rpc,
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
    Answer(AnswerArgs),
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
}

#[derive(Subcommand)]
enum NoteCommand {
    Rename(NoteRenameArgs),
    Capture(NoteCaptureArgs),
    Stats(NoteStatsArgs),
    Links,
    Write(NoteWriteArgs),
    Move(NoteMoveArgs),
}

#[derive(Subcommand)]
enum SessionCommand {
    Start(SessionStartArgs),
    End(SessionEndArgs),
    Link(SessionLinkArgs),
    List(SessionListArgs),
    Show(SessionShowArgs),
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
struct AnswerArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    id: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    assignee: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    body: Option<String>,
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
}

#[derive(Args)]
struct LaneRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    abbr: Option<String>,
}

#[derive(Args)]
struct LaneDeleteArgs {
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

#[derive(Args)]
struct SessionStartArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    agent: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    harness_session: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    harness_transport: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    brief: Option<String>,
}

#[derive(Args)]
struct SessionEndArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    outcome: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    summary: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tokens_input: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tokens_output: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tokens_reasoning: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tokens_cache_read: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    tokens_cache_write: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    cost: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    model: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    skills: Option<String>,
    #[arg(long)]
    usage_from_harness: bool,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    harness_events: Option<String>,
}

#[derive(Args)]
struct SessionLinkArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    ticket: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
}

#[derive(Args)]
struct SessionListArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    status: Option<String>,
}

#[derive(Args)]
struct SessionShowArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    session: Option<String>,
}

#[derive(Subcommand)]
enum SetupCommand {
    Install(SetupInstallArgs),
    Uninstall(SetupUninstallArgs),
    Status,
}

#[derive(Subcommand)]
enum SettingsCommand {
    Set(SettingsSetArgs),
}

#[derive(Args)]
struct SettingsSetArgs {
    /// One roster name per flag; the whole list replaces the stored one.
    /// No flag stores the empty list.
    #[arg(long)]
    roster: Vec<String>,
}

#[derive(Args)]
struct SetupInstallArgs {
    /// A host name, or a comma-separated list (default: opencode).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    host: Option<String>,
    /// Replace a link that points elsewhere, a checkout included.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct SetupUninstallArgs {
    /// A host name, or a comma-separated list (default: every host).
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    host: Option<String>,
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

    /// Exit with a code and no message: the verb already printed its report,
    /// and the report is the diagnosis.
    fn silent(exit: u8) -> Self {
        Self {
            message: String::new(),
            exit,
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
        Some(Command::Note { command }) => match command {
            None => return usage::group("note").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Session { command }) => match command {
            None => return usage::group("session").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Sprint { command }) => match command {
            None => return usage::group("sprint").unwrap_or(usage::ROOT),
            Some(command) => command.help(),
        },
        Some(Command::Setup { command }) => match command {
            None => return usage::group("setup").unwrap_or(usage::ROOT),
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
            Self::Answer(_) => "ticket answer",
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
        }
    }
}

impl NoteCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Rename(_) => "note rename",
            Self::Capture(_) => "note capture",
            Self::Stats(_) => "note stats",
            Self::Links => "note links",
            Self::Write(_) => "note write",
            Self::Move(_) => "note move",
        }
    }
}

impl SessionCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Start(_) => "session start",
            Self::End(_) => "session end",
            Self::Link(_) => "session link",
            Self::List(_) => "session list",
            Self::Show(_) => "session show",
        }
    }
}

impl SetupCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::Install(_) => "setup install",
            Self::Uninstall(_) => "setup uninstall",
            Self::Status => "setup status",
        }
    }
}

impl SettingsCommand {
    fn help(&self) -> &'static str {
        match self {
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

// MARK: - Dispatch

fn run(cli: &Cli) -> Result<(), Failure> {
    let user_home = store::user_home().map_err(|error| Failure::message(error.to_string()))?;
    let environment = env::var(store::ENVIRONMENT).ok();
    let home = Home::resolve(cli.home.as_deref(), environment.as_deref(), &user_home);
    let command = cli.command.as_ref().ok_or_else(Failure::usage)?;

    match command {
        Command::Doctor => doctor(&home),
        Command::Rpc => rpc(&home),
        Command::Ticket { command } => ticket(&home, command.as_ref()),
        Command::Lane { command } => lane(&home, command.as_ref()),
        Command::Note { command } => note(&home, command.as_ref()),
        Command::Session { command } => session(&home, command.as_ref()),
        Command::Sprint { command } => sprint(&home, command.as_ref()),
        Command::Setup { command } => setup_command(&home, command.as_ref()),
        Command::Settings { command } => settings(&home, command.as_ref()),
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
                None => Status::Backlog,
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
                None => Status::Backlog,
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
        TicketCommand::Answer(args) => {
            let body = match args.body.as_deref() {
                Some("-") => read_standard_input()?,
                Some(body) => body.to_string(),
                None => String::new(),
            };
            let assignee = resolve_assignee(args.assignee.as_deref())?;
            let ticket = board
                .answer(
                    args.id.as_deref().unwrap_or(""),
                    Status::parse(args.status.as_deref().unwrap_or("")).map_err(Failure::domain)?,
                    assignee.as_deref(),
                    &body,
                    unix_now(),
                )
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
                .add_lane(args.name.as_deref().unwrap_or(""), args.abbr.as_deref())
                .map_err(Failure::domain)?;
            print(&LaneList { lanes, note: None })
        }
        LaneCommand::Rename(args) => {
            let lanes = board
                .rename_lane(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                    args.abbr.as_deref(),
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

fn session(home: &Home, command: Option<&SessionCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, SessionCommand::List(_) | SessionCommand::Show(_)) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let sessions = Sessions::new(&store);
    let actor = Actor::current();

    match command {
        SessionCommand::Start(args) => {
            let id = started_id(args.session.as_deref());
            let project = args.project.clone().unwrap_or_else(project_default);
            let agent = session_agent(args.agent.clone());
            let brief = session_brief(args.brief.as_deref())?;
            let harness = harness_session(
                args.harness_session.as_deref(),
                args.harness_transport.as_deref(),
            )?;
            let result = match harness {
                Some(harness) => sessions.start_with_harness(
                    &actor,
                    &id,
                    &project,
                    agent.as_deref(),
                    &brief,
                    harness,
                    None,
                ),
                None => sessions.start(&actor, &id, &project, agent.as_deref(), &brief, None),
            }
            .map_err(Failure::domain)?;
            print(&SessionResult {
                session: result.value,
                note: result.note,
            })
        }
        SessionCommand::End(args) => {
            let id = session_handle(&sessions, args.session.as_deref())?;
            let outcome = session_outcome(args.outcome.as_deref())?;
            let summary = session_summary(args.summary.as_deref())?;
            let mut tokens = usage_tokens(args)?;
            let mut cost = usage_cost(args.cost.as_deref())?;
            let mut model = usage_model(args.model.as_deref());
            let mut skills = match args.skills.as_deref() {
                Some(raw) => parse_skills(raw).map_err(Failure::domain)?,
                None => None,
            };
            if args.usage_from_harness {
                let record = sessions.show(&id).map_err(Failure::domain)?;
                let reported = harness_usage(&record, args.harness_events.as_deref())?;
                tokens = reported.tokens().or(tokens);
                cost = reported.cost.or(cost);
                model = reported.model.or(model);
                skills = reported.skills.or(skills);
            }
            let stamp = sessions
                .end(
                    &actor,
                    &id,
                    tandem_core::domain::session::Ending {
                        outcome,
                        summary: summary.as_deref(),
                        tokens,
                        cost,
                        model: model.as_deref(),
                        skills,
                        at: None,
                    },
                )
                .map_err(Failure::domain)?;
            if stamp.stamped && stamp.session.carries_usage() {
                post_usage_comments(&store, &stamp.session)?;
            }
            print(&SessionResult {
                session: stamp.session,
                note: stamp.note,
            })
        }
        SessionCommand::Link(args) => {
            let id = session_handle(&sessions, args.session.as_deref())?;
            let ticket = Board::new(store.clone())
                .ticket(args.ticket.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            let result = sessions
                .link(&actor, &id, &ticket.id.to_string())
                .map_err(Failure::domain)?;
            print(&SessionResult {
                session: result.value,
                note: result.note,
            })
        }
        SessionCommand::List(args) => {
            let status = match args.status.as_deref() {
                Some(raw) => Some(
                    tandem_core::domain::session::SessionStatus::parse(raw).ok_or_else(|| {
                        Failure::message(format!(
                            "session status must be one of running, ended (got \"{raw}\")."
                        ))
                    })?,
                ),
                None => None,
            };
            let records = sessions
                .list(args.project.as_deref(), status)
                .map_err(Failure::domain)?;
            print(&SessionList { sessions: records })
        }
        SessionCommand::Show(args) => {
            let id = session_handle(&sessions, args.session.as_deref())?;
            let record = sessions.show(&id).map_err(Failure::domain)?;
            let notes = bikeshed(home)
                .session_notes(&id)
                .map_err(Failure::domain)?
                .value;
            print(&SessionDetailView {
                session: record,
                notes,
                note: None,
            })
        }
    }
}

fn sprint(home: &Home, command: Option<&SprintCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, SprintCommand::List(_) | SprintCommand::Show(_)) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let sprints = Sprints::new(&store);
    let sessions = Sessions::new(&store);
    let actor = Actor::current();

    match command {
        SprintCommand::Open(args) => {
            let lane = args.lane.as_deref().unwrap_or("");
            let mode = sprint_mode(args.mode.as_deref())?;
            let id = session_handle(&sessions, args.session.as_deref())?;
            let result = sprints
                .open(&actor, lane, mode, Some(&id), None)
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

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

/// The harness session a record points at: `--harness-session`, else
/// `$OPENCODE_SESSION_ID`; no transport flag is `database`, the external
/// opencode this Mac dogfoods (PRT-29).
fn harness_session(
    raw: Option<&str>,
    transport: Option<&str>,
) -> Result<Option<HarnessSession>, Failure> {
    let from_environment = || {
        env::var("OPENCODE_SESSION_ID")
            .map(|id| id.trim().to_string())
            .unwrap_or_default()
    };
    let id = match raw {
        Some(raw) => raw.trim().to_string(),
        None => from_environment(),
    };
    if id.is_empty() {
        if raw.is_some() {
            return Err(Failure::domain(DomainError::EmptyHarnessSessionId));
        }
        if transport.is_some() {
            return Err(Failure::message(
                "--harness-transport names no harness session. Pass --harness-session <id> with it.",
            ));
        }
        return Ok(None);
    }
    let transport = match transport {
        Some(raw) => HarnessTransport::parse(raw.trim()).ok_or_else(|| {
            Failure::domain(DomainError::InvalidHarnessTransport(raw.to_string()))
        })?,
        None => HarnessTransport::Database,
    };
    Ok(Some(HarnessSession { id, transport }))
}

/// The usage a record's harness session reported, read through the transport
/// the record names: the event stream `--harness-events` points at, or the
/// external opencode database.
fn harness_usage(
    record: &tandem_core::domain::session::Session,
    events: Option<&str>,
) -> Result<harness::HarnessUsage, Failure> {
    let id = record.harness_session_id.clone().ok_or_else(|| {
        Failure::domain(DomainError::NoHarnessSession {
            id: record.id.clone(),
        })
    })?;
    match record.harness_transport {
        Some(HarnessTransport::EventStream) => {
            let stream = events
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| Failure::domain(DomainError::NoHarnessEventsPath))?;
            harness::read_events(Path::new(stream), &id).map_err(Failure::domain)
        }
        _ => harness::read(&opencode_database(), &id).map_err(Failure::domain),
    }
}

/// The external opencode store: `$OPENCODE_DB`, else the path its own default
/// names. Tandem reads it read-only and never writes it.
fn opencode_database() -> PathBuf {
    if let Some(path) = env::var_os("OPENCODE_DB") {
        return PathBuf::from(path);
    }
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}

/// A session record's handle when the verb opens one: `--session`, else a
/// minted id. Tandem owns the record (PRT-29); the harness session is a column
/// beside it, never the key, so a second pass in one harness session opens a
/// record of its own instead of finding the first one "already recorded".
fn started_id(raw: Option<&str>) -> String {
    match raw {
        Some(raw) => raw.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    }
}

/// A session record's handle when the verb reads or stamps one: `--session`,
/// else `$OPENCODE_SESSION_ID`, resolved to the record (an exact id first, then
/// the harness pointer the older caller speaks).
fn session_handle(sessions: &Sessions, raw: Option<&str>) -> Result<String, Failure> {
    let handle = match raw {
        Some(raw) => raw.to_string(),
        None => match env::var("OPENCODE_SESSION_ID") {
            Ok(id) if !id.trim().is_empty() => id,
            _ => return Err(Failure::domain(DomainError::SessionIdUnset)),
        },
    };
    sessions.resolve(&handle).map_err(Failure::domain)
}

/// A session's agent: `--agent`, else `$TANDEM_AGENT`; nobody when the
/// environment names no agent. The core trims the name.
fn session_agent(raw: Option<String>) -> Option<String> {
    raw.or_else(|| env::var("TANDEM_AGENT").ok())
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

/// A session's brief: the flag's value, or stdin when it is `-`. A value naming
/// an existing regular file is refused: the flag never read the file, so the
/// path would be filed as the brief.
fn session_brief(raw: Option<&str>) -> Result<String, Failure> {
    let raw = raw.unwrap_or("");
    if raw != "-" {
        let expanded = expand_tilde(raw);
        if expanded.is_file() {
            return Err(Failure::domain(DomainError::BriefNamesAFile(
                raw.to_string(),
            )));
        }
        return Ok(raw.to_string());
    }
    let body = read_standard_input()?;
    if body.trim().is_empty() {
        return Err(Failure::domain(DomainError::EmptySessionBrief));
    }
    Ok(body)
}

/// `~` is the shell's to expand, and no shell runs between the caller and the
/// flag.
fn expand_tilde(raw: &str) -> PathBuf {
    if (raw == "~" || raw.starts_with("~/"))
        && let Some(home) = env::var_os("HOME")
    {
        let rest = raw.trim_start_matches('~').trim_start_matches('/');
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(raw)
}

/// A session's summary: absent is none, and a `--summary -` body with no text is
/// a failed write, not a deliberate silence.
fn session_summary(raw: Option<&str>) -> Result<Option<String>, Failure> {
    match raw {
        None => Ok(None),
        Some("-") => {
            let body = read_standard_input()?;
            if body.trim().is_empty() {
                return Err(Failure::domain(DomainError::EmptySessionSummary));
            }
            Ok(Some(body))
        }
        Some(text) => Ok(Some(text.to_string())),
    }
}

/// How a session ended: absent is `unknown`, the honest answer.
fn session_outcome(raw: Option<&str>) -> Result<SessionOutcome, Failure> {
    match raw {
        None => Ok(SessionOutcome::Unknown),
        Some(raw) => SessionOutcome::parse(raw).ok_or_else(|| {
            Failure::message(format!(
                "outcome must be one of succeeded, failed, interrupted, unknown (got \"{raw}\")."
            ))
        }),
    }
}

/// The token counts the usage flags report; absent is absent on the record, and
/// a value that is not a non-negative whole number names the flag.
fn usage_tokens(args: &SessionEndArgs) -> Result<Option<SessionTokens>, Failure> {
    fn count(flag: &str, raw: Option<&str>) -> Result<Option<i64>, Failure> {
        match raw {
            None => Ok(None),
            Some(raw) => raw
                .parse::<i64>()
                .ok()
                .filter(|value| *value >= 0)
                .map(Some)
                .ok_or_else(|| {
                    Failure::domain(DomainError::InvalidUsageFlag {
                        flag: flag.to_string(),
                        value: raw.to_string(),
                    })
                }),
        }
    }
    let input = count("tokens-input", args.tokens_input.as_deref())?;
    let output = count("tokens-output", args.tokens_output.as_deref())?;
    let reasoning = count("tokens-reasoning", args.tokens_reasoning.as_deref())?;
    let cache_read = count("tokens-cache-read", args.tokens_cache_read.as_deref())?;
    let cache_write = count("tokens-cache-write", args.tokens_cache_write.as_deref())?;
    if input.is_none()
        && output.is_none()
        && reasoning.is_none()
        && cache_read.is_none()
        && cache_write.is_none()
    {
        return Ok(None);
    }
    Ok(Some(SessionTokens {
        input,
        output,
        reasoning,
        cache_read,
        cache_write,
    }))
}

/// The cost `--cost` reports, in dollars; a value that is not a non-negative
/// number names the flag.
fn usage_cost(raw: Option<&str>) -> Result<Option<f64>, Failure> {
    match raw {
        None => Ok(None),
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(Some)
            .ok_or_else(|| {
                Failure::domain(DomainError::InvalidUsageFlag {
                    flag: "cost".to_string(),
                    value: raw.to_string(),
                })
            }),
    }
}

/// `--model`: absent is absent; an empty value is no model.
fn usage_model(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// The one `usage` comment a stamped session posts on every ticket it carried.
fn post_usage_comments(
    store: &Store,
    session: &tandem_core::domain::session::Session,
) -> Result<(), Failure> {
    if session.tickets.is_empty() {
        return Ok(());
    }
    let board = Board::new(store.clone());
    let author = session.agent.clone().unwrap_or_else(|| "usage".to_string());
    let body = usage_comment_body(session);
    for ticket in &session.tickets {
        board
            .append_comment(ticket, &author, &body)
            .map_err(Failure::domain)?;
    }
    Ok(())
}

/// `Usage` plus the session id and model, then one line per reported piece.
fn usage_comment_body(session: &tandem_core::domain::session::Session) -> String {
    let mut lines = Vec::new();
    let mut header = format!("Usage \u{2014} {}", session.id);
    if let Some(model) = &session.model {
        header.push_str(" \u{00b7} ");
        header.push_str(model);
    }
    lines.push(header);
    let mut metric = |name: &str, value: Option<i64>| {
        if let Some(value) = value {
            lines.push(format!("{name} {}", grouped(value)));
        }
    };
    if let Some(tokens) = &session.tokens {
        metric("input", tokens.input);
        metric("output", tokens.output);
        metric("cache read", tokens.cache_read);
        metric("cache write", tokens.cache_write);
        metric("reasoning", tokens.reasoning);
    }
    if let Some(cost) = session.cost {
        lines.push(format!("cost ${cost:.2}"));
    }
    lines.join("\n")
}

/// Thousands grouped with commas, en_US style, so a bare count reads.
fn grouped(value: i64) -> String {
    let text = value.to_string();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", text.as_str()),
    };
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(character);
    }
    format!("{sign}{out}")
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

// MARK: - settings

/// `settings set`: the roster's whole replacement, the same write the Settings
/// face's Save makes over the wire. The flags are the list: one `--roster`
/// per name, and no flag stores the empty list. Cleaning is the domain's, so
/// the printed receipt is the cleaned list the store holds.
fn settings(home: &Home, command: Option<&SettingsCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = write_store(home)?;
    let actor = Actor::current();

    match command {
        SettingsCommand::Set(args) => {
            let roster = Settings::new(&store)
                .set_roster(&actor, args.roster.clone())
                .map_err(Failure::domain)?;
            print(&SettingsReply { roster })
        }
    }
}

// MARK: - doctor

/// `doctor` (ADR 0014): a setup report is not a domain call, so it does not go
/// through `run`. It always prints the report JSON to stdout, the report is the
/// success value even when a check fails, and exits 0 when every
/// failing-capable check passed, 1 otherwise, echoing each failing check's next
/// step to stderr. A read-only verb: it never creates the home it reports.
fn doctor(home: &Home) -> Result<(), Failure> {
    let report = diagnostics::run(home, &environment());
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

// MARK: - setup

/// `setup install|uninstall|status` (ADR 0034): one JSON report on stdout
/// whatever the exit code, because the report is the diagnosis. `install` and
/// `uninstall` exit 1 when a warning is an error; `status` exits 0 always. A
/// thrown failure keeps the other verbs' contract: message on stderr, exit 1,
/// no JSON on stdout.
fn setup_command(home: &Home, command: Option<&SetupCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let environment = environment();
    match command {
        SetupCommand::Install(args) => {
            let hosts = setup::hosts(args.host.as_deref()).map_err(setup_failure)?;
            let bundle = setup::bundled_face(&environment);
            let report = setup::install(
                home.directory(),
                &environment,
                bundle.as_deref(),
                args.force,
                &hosts,
            )
            .map_err(setup_failure)?;
            print(&report)?;
            if report.state.has_error() {
                return Err(Failure::silent(1));
            }
            Ok(())
        }
        SetupCommand::Uninstall(args) => {
            let hosts = setup::uninstall_hosts(args.host.as_deref()).map_err(setup_failure)?;
            let report = setup::uninstall(home.directory(), &environment, hosts.as_deref())
                .map_err(setup_failure)?;
            print(&report)?;
            if report.state.has_error() {
                return Err(Failure::silent(1));
            }
            Ok(())
        }
        SetupCommand::Status => {
            let report = setup::status(home.directory(), &environment);
            print(&report)
        }
    }
}

/// A setup failure is a message an agent or human can act on, exit 1, and no
/// JSON: the report is only for a run that got far enough to build one.
fn setup_failure(error: impl std::fmt::Display) -> Failure {
    Failure::message(error.to_string())
}

/// The process's environment as the setup and doctor code reads it: `HOME`,
/// `XDG_CONFIG_HOME`, `TANDEM_AGENT_BUNDLE` and the four host flags. A value
/// that is not UTF-8 is not one of those names and is dropped.
fn environment() -> Environment {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
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
