//! The tandem binary: the agent face and the board's HTTP server.
//!
//! One binary: the verbs agents call, the MCP
//! endpoint and the board the human reads are the same executable, so they
//! cannot drift. This rung carries the whole grammar the skills speak
//! `ticket`, `label`, `note`, `doctor`'s pinned report and `serve`'s
//! one HTTP door (ADR 0047). Set `TANDEM_SERVER` and the verbs run against a
//! remote `serve` instead of the local store.
//!
//! What lives here is what belongs to a process rather than to the domain:
//! argv, the environment, the two streams and the exit code. Every verb maps to
//! one domain call and prints the wire type that call answers with; no boundary
//! rule is re-implemented here, so the two binaries cannot disagree about what a
//! verb means, only about how a flag was spelled.

mod remote;
mod usage;

use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use tandem_core::archive;
use tandem_core::diagnostics;
use tandem_core::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, SetFields, Status, TicketQuery,
};
use tandem_core::env::Environment;
use tandem_core::notes::{BoardNames, BoardProject, Notes};
use tandem_core::store::{self, Actor, Home, Store};
use tandem_core::wire::{
    ArchivedView, CommentList, CommentResult, CommentView, LabelDeletionView, LabelList,
    NoteMoveView, NotePathView, NoteWriteView, ProjectList, TicketDetail, TicketList,
    TicketSummary,
};
use tracing_subscriber::EnvFilter;

// MARK: - The grammar

/// The root: `--home` on every verb, and the group the run names.
///
/// `--help` is the CLI's own flag, not clap's: the texts in [`usage`] are the
/// golden-pinned text, and a generator's layout would be a second grammar to keep.
#[derive(Parser)]
#[command(
    name = "tandem",
    version = tandem_core::release_version(),
    about = "Tandem: the board, the notes and the records",
    disable_help_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// The home this run keeps its state in (default: $TANDEM_HOME, else ~/.tandem).
    #[arg(long, global = true, value_name = "PATH")]
    home: Option<PathBuf>,

    /// Print the usage text for this verb.
    #[arg(short = 'h', long = "help", global = true, action = clap::ArgAction::SetTrue)]
    help: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args)]
struct ServeArgs {
    /// The address to bind (default 127.0.0.1).
    #[arg(long, default_value = "127.0.0.1", value_name = "IP")]
    host: String,
    /// The port to bind; 0 asks the OS for a free one.
    #[arg(long, default_value_t = 7700, value_name = "PORT")]
    port: u16,
    /// Open the board in the system browser after binding.
    #[arg(long)]
    open: bool,
    /// Serve a non-loopback address with no authentication (a network you trust).
    #[arg(long)]
    allow_unauthenticated: bool,
    /// The one public https origin a TLS proxy serves this board at.
    #[arg(long, value_name = "ORIGIN")]
    public_origin: Option<String>,
}

#[derive(Args)]
struct ExportArgs {
    /// Where to write the archive; `-` streams to stdout.
    #[arg(long, value_name = "PATH")]
    out: Option<String>,
}

#[derive(Args)]
struct ImportArgs {
    /// The archive to restore; `-` reads stdin.
    path: Option<String>,
    /// A ceiling on the unpacked total, in bytes (default: none).
    #[arg(long, value_name = "N")]
    max_bytes: Option<u64>,
}

#[derive(Subcommand)]
enum Command {
    /// The board's ticket surface.
    Ticket {
        #[command(subcommand)]
        command: Option<TicketCommand>,
    },
    /// The label lifecycle.
    Label {
        #[command(subcommand)]
        command: Option<LabelCommand>,
    },
    /// The project lifecycle: a label's parent.
    Project {
        #[command(subcommand)]
        command: Option<ProjectCommand>,
    },
    /// The notes' agent face.
    Note {
        #[command(subcommand)]
        command: Option<NoteCommand>,
    },
    /// Report whether this machine's store and notes are sound (ADR 0014).
    Doctor,
    /// Write the whole home as one archive (ADR 0050).
    Export(ExportArgs),
    /// Restore an archive into an empty home (ADR 0050).
    Import(ImportArgs),
    /// Serve the board and the MCP endpoint over HTTP (ADR 0047).
    Serve(ServeArgs),
}

#[derive(Subcommand)]
enum TicketCommand {
    List(ListArgs),
    Show(ShowArgs),
    Add(AddArgs),
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
enum LabelCommand {
    List,
    Add(LabelAddArgs),
    Rename(LabelRenameArgs),
    Delete(LabelDeleteArgs),
    Move(LabelMoveArgs),
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
    Stats(NoteStatsArgs),
    Links,
    Write(NoteWriteArgs),
    Move(NoteMoveArgs),
    Create(NoteCreateArgs),
    CreateFolder(NoteCreateFolderArgs),
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
    project: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    label: Option<String>,
    /// The archive as its own view, or joining a named filter.
    #[arg(long)]
    archived: bool,
    /// Only tickets waiting on an open blocker.
    #[arg(long)]
    blocked: bool,
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
    project: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    label: Option<String>,
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
    label: Option<String>,
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
struct LabelAddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Args)]
struct LabelRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
}

#[derive(Args)]
struct LabelDeleteArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
}

#[derive(Args)]
struct LabelMoveArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    index: Option<String>,
}

#[derive(Args)]
struct ProjectAddArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    name: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    abbr: Option<String>,
}

#[derive(Args)]
struct ProjectRenameArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    from: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    to: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    abbr: Option<String>,
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
    /// The hash `note read` returned (`absent` for a new note); the write is
    /// refused if the note changed since.
    #[arg(long = "expected-hash")]
    expected_hash: Option<String>,
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
struct NoteStatsArgs {
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    project: Option<String>,
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
#[derive(Debug)]
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
        Some(Command::Export(_)) => return usage::verb("export").unwrap_or(usage::ROOT),
        Some(Command::Import(_)) => return usage::verb("import").unwrap_or(usage::ROOT),
        Some(Command::Serve(_)) => return usage::SERVE,
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
        Some(Command::Label { command }) => match command {
            None => return usage::group("label").unwrap_or(usage::ROOT),
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
    };
    usage::verb(name).unwrap_or(usage::ROOT)
}

impl TicketCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List(_) => "ticket list",
            Self::Show(_) => "ticket show",
            Self::Add(_) => "ticket add",
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

impl LabelCommand {
    fn help(&self) -> &'static str {
        match self {
            Self::List => "label list",
            Self::Add(_) => "label add",
            Self::Rename(_) => "label rename",
            Self::Delete(_) => "label delete",
            Self::Move(_) => "label move",
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
            Self::Stats(_) => "note stats",
            Self::Links => "note links",
            Self::Write(_) => "note write",
            Self::Move(_) => "note move",
            Self::Create(_) => "note create",
            Self::CreateFolder(_) => "note create-folder",
        }
    }
}

// MARK: - Dispatch

fn run(cli: &Cli) -> Result<(), Failure> {
    // `TANDEM_SERVER` sends the verbs to a running `tandem serve` instead of
    // the local store (ADR 0047). Unset or blank keeps the direct path.
    if let Ok(server) = env::var(remote::SERVER_VARIABLE)
        && !server.trim().is_empty()
        && let Some(command) = cli.command.as_ref()
    {
        return remote::run(&server, command);
    }
    let user_home = store::user_home().map_err(|error| Failure::message(error.to_string()))?;
    let environment = env::var(store::ENVIRONMENT).ok();
    let home = Home::resolve(cli.home.as_deref(), environment.as_deref(), &user_home);
    let command = cli.command.as_ref().ok_or_else(Failure::usage)?;

    match command {
        Command::Doctor => doctor(&home),
        Command::Export(args) => export(&home, args),
        Command::Import(args) => import(&home, args),
        Command::Serve(args) => serve(&home, args),
        Command::Ticket { command } => ticket(&home, command.as_ref()),
        Command::Label { command } => label(&home, command.as_ref()),
        Command::Project { command } => project(&home, command.as_ref()),
        Command::Note { command } => note(&home, command.as_ref()),
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
                args.project.as_deref(),
                args.label.as_deref(),
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
                        project: args.project.as_deref(),
                        label: args.label.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: description.as_deref(),
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
                .move_ticket(args.id.as_deref().unwrap_or(""), args.label.as_deref(), at)
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
                    let actor = Actor::current();
                    let comment = board
                        .append_comment(
                            args.id.as_deref().unwrap_or(""),
                            tandem_core::domain::resolve_comment_author(
                                args.author.as_deref(),
                                &actor,
                            ),
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

fn label(home: &Home, command: Option<&LabelCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let store = if matches!(command, LabelCommand::List) {
        Store::new(home.clone())
    } else {
        write_store(home)?
    };
    let board = Board::new(store);

    match command {
        LabelCommand::List => print(&LabelList {
            labels: board.labels().map_err(Failure::domain)?,
            note: None,
        }),
        LabelCommand::Add(args) => {
            let labels = board
                .add_label(args.name.as_deref().unwrap_or(""), args.project.as_deref())
                .map_err(Failure::domain)?;
            print(&LabelList { labels, note: None })
        }
        LabelCommand::Rename(args) => {
            let labels = board
                .rename_label(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                    args.project.as_deref(),
                )
                .map_err(Failure::domain)?;
            print(&LabelList { labels, note: None })
        }
        LabelCommand::Delete(args) => {
            let deletion = board
                .delete_label(args.name.as_deref().unwrap_or(""))
                .map_err(Failure::domain)?;
            print(&LabelDeletionView::new(&deletion, None))
        }
        LabelCommand::Move(args) => {
            let at = match args.index.as_deref() {
                None => usize::MAX,
                Some(raw) => match raw.parse::<i64>() {
                    Ok(index) => index.max(0) as usize,
                    Err(_) => {
                        return Err(Failure::domain(DomainError::InvalidIndex(raw.to_string())));
                    }
                },
            };
            let labels = board
                .move_label(args.name.as_deref().unwrap_or(""), at)
                .map_err(Failure::domain)?;
            print(&LabelList { labels, note: None })
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
                .add_project(args.name.as_deref().unwrap_or(""), args.abbr.as_deref())
                .map_err(Failure::domain)?;
            print(&ProjectList::new(&projects, None))
        }
        ProjectCommand::Rename(args) => {
            let projects = board
                .rename_project(
                    args.from.as_deref().unwrap_or(""),
                    args.to.as_deref().unwrap_or(""),
                    args.abbr.as_deref(),
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

fn note(home: &Home, command: Option<&NoteCommand>) -> Result<(), Failure> {
    let command = command.ok_or_else(Failure::usage)?;
    let vault = notes(home)?;

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
        NoteCommand::Stats(args) => {
            let stats = vault
                .stats(args.project.as_deref())
                .map_err(Failure::domain)?;
            eprintln!(
                "note stats: the `bikeshed` field is deprecated and goes in the next release; \
                 read `notes`."
            );
            print(&stats.value)
        }
        NoteCommand::Links => {
            let projects = Board::new(Store::new(home.clone()))
                .projects()
                .map_err(Failure::domain)?;
            let names = BoardNames {
                projects: projects
                    .into_iter()
                    .map(|entry| BoardProject {
                        name: entry.name,
                        labels: entry.labels,
                    })
                    .collect(),
            };
            let health = vault.health(&names).map_err(Failure::domain)?;
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
            let record = vault
                .write(path, &text, args.expected_hash.as_deref())
                .map_err(Failure::domain)?;
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

/// The notes a home keeps, after the one-time move of an older `bikeshed/`
/// folder (ADR 0055): the CLI's note verbs open through here.
fn notes(home: &Home) -> Result<Notes, Failure> {
    tandem_core::notes::migrate::migrate_home(home).map_err(Failure::domain)?;
    Ok(Notes::new(home.notes_directory()))
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

// MARK: - serve

/// The environment variable that sets `tandem serve`'s log filter, in RUST_LOG's
/// directive syntax. Read once, here: the core never reads it.
const LOG_VARIABLE: &str = "TANDEM_LOG";

/// Installs the process's log: plain lines on `writer`, filtered by
/// `TANDEM_LOG` (RUST_LOG syntax) with `default` when it is unset or invalid.
/// `tandem serve` writes stdout at `info`; `tandem mcp` writes stderr at `warn`
/// and never colours, because stdout there carries frames. Returns whether the
/// variable is set to a filter that does not parse, so the caller can say so
/// once. The value is never echoed.
fn install_log<W>(default: &str, writer: W, ansi: bool) -> bool
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    let (filter, invalid) = match env::var(LOG_VARIABLE) {
        Ok(value) if !value.trim().is_empty() => match EnvFilter::try_new(&value) {
            Ok(filter) => (filter, false),
            Err(_) => (EnvFilter::new(default), true),
        },
        Err(env::VarError::NotUnicode(_)) => (EnvFilter::new(default), true),
        _ => (EnvFilter::new(default), false),
    };
    // `try_init` fails only when a subscriber is already installed, which
    // leaves that one in charge: the first install wins.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(ansi)
        .try_init();
    invalid
}

/// The environment variable that carries the bearer token, for `tandem serve`
/// off loopback and for the remote CLI. Never a flag value, never stored.
const TOKEN_VARIABLE: &str = "TANDEM_TOKEN";

/// The environment variables that carry the operator key and the next one
/// (CUT-40). Never flag values, never stored.
const OPERATOR_KEY_VARIABLE: &str = "TANDEM_OPERATOR_KEY";
const OPERATOR_KEY_NEXT_VARIABLE: &str = "TANDEM_OPERATOR_KEY_NEXT";

/// The operator key and its rotation partner from the environment: each at least
/// [`MIN_TOKEN_LENGTH`] characters, never the owner token, and the next only
/// beside a current one. An empty variable is unset. A refusal names the
/// variable and the rule, never a value.
fn resolve_operator_keys(
    key: Option<String>,
    next: Option<String>,
    owner_token: Option<&str>,
) -> Result<Option<(String, Option<String>)>, Failure> {
    let refuse = |message: String| Failure {
        message: format!("tandem serve: {message}"),
        exit: 2,
    };
    let key = key.filter(|value| !value.is_empty());
    let next = next.filter(|value| !value.is_empty());
    for (name, value) in [
        (OPERATOR_KEY_VARIABLE, &key),
        (OPERATOR_KEY_NEXT_VARIABLE, &next),
    ] {
        let Some(value) = value else { continue };
        if value.chars().count() < MIN_TOKEN_LENGTH {
            return Err(refuse(format!(
                "{name} must be at least {MIN_TOKEN_LENGTH} characters."
            )));
        }
        if owner_token == Some(value.as_str()) {
            return Err(refuse(format!(
                "{name} must not be {TOKEN_VARIABLE}: the operator key is its own credential."
            )));
        }
    }
    match (key, next) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(refuse(format!(
            "{OPERATOR_KEY_NEXT_VARIABLE} is set without {OPERATOR_KEY_VARIABLE}."
        ))),
        (Some(key), next) => Ok(Some((key, next))),
    }
}

/// The environment variable that carries the public origin when no flag does.
const PUBLIC_ORIGIN_VARIABLE: &str = "TANDEM_PUBLIC_ORIGIN";

/// The public origin to serve behind, normalized: the flag wins over the
/// variable, an empty variable is unset, an empty flag is an error, and a
/// malformed variable is refused even when the flag is also set.
fn resolve_public_origin(
    flag: Option<&str>,
    variable: Option<String>,
) -> Result<Option<String>, Failure> {
    let refuse = |source: &str, reason: &str| Failure {
        message: format!(
            "tandem serve: {source} {reason} It must be https://host[:port], with no path, \
             query or userinfo."
        ),
        exit: 2,
    };
    let from_variable = match variable.filter(|value| !value.is_empty()) {
        Some(value) => Some(
            tandem_core::serve::normalize_public_origin(&value)
                .map_err(|reason| refuse(PUBLIC_ORIGIN_VARIABLE, reason))?,
        ),
        None => None,
    };
    match flag {
        Some(value) => tandem_core::serve::normalize_public_origin(value)
            .map(Some)
            .map_err(|reason| refuse("--public-origin", reason)),
        None => Ok(from_variable),
    }
}

/// The shortest token a non-loopback `tandem serve` accepts.
const MIN_TOKEN_LENGTH: usize = 32;

/// The one door (ADR 0047): a non-loopback address is refused before binding
/// unless `TANDEM_TOKEN` is set or `--allow-unauthenticated` is passed, the listener prints the address it actually
/// bound (so `--port 0` is usable), and SIGINT or SIGTERM ends the run.
fn serve(home: &Home, args: &ServeArgs) -> Result<(), Failure> {
    let host: std::net::IpAddr = args.host.parse().map_err(|_| Failure {
        message: format!(
            "tandem serve: --host must be an IP address, got \"{}\".",
            args.host
        ),
        exit: 2,
    })?;
    let public_origin = resolve_public_origin(
        args.public_origin.as_deref(),
        env::var(PUBLIC_ORIGIN_VARIABLE).ok(),
    )?;
    if public_origin.is_some() && host.is_loopback() {
        return Err(Failure {
            message: format!(
                "tandem serve: --public-origin and {PUBLIC_ORIGIN_VARIABLE} need a non-loopback \
                 --host and {TOKEN_VARIABLE}: {host} is a loopback address."
            ),
            exit: 2,
        });
    }
    // Off loopback the run needs a stated choice: a token in the environment
    // (never a flag, which would show in `ps`) or the explicit opt-out of
    // authentication. A token that is set must be a strong one.
    let token = if host.is_loopback() {
        None
    } else {
        match env::var(TOKEN_VARIABLE)
            .ok()
            .filter(|token| !token.is_empty())
        {
            Some(token) if token.chars().count() >= MIN_TOKEN_LENGTH => Some(token),
            Some(_) => {
                return Err(Failure {
                    message: format!(
                        "tandem serve: {TOKEN_VARIABLE} must be at least {MIN_TOKEN_LENGTH} \
                         characters."
                    ),
                    exit: 2,
                });
            }
            None if args.allow_unauthenticated => None,
            None => {
                return Err(Failure {
                    message: format!(
                        "tandem serve: {host} is not a loopback address. Set {TOKEN_VARIABLE} \
                         (at least {MIN_TOKEN_LENGTH} characters) and put a TLS terminator in \
                         front, or pass --allow-unauthenticated to serve a network you trust \
                         with no authentication."
                    ),
                    exit: 2,
                });
            }
        }
    };
    let operator_keys = resolve_operator_keys(
        env::var(OPERATOR_KEY_VARIABLE).ok(),
        env::var(OPERATOR_KEY_NEXT_VARIABLE).ok(),
        env::var(TOKEN_VARIABLE).ok().as_deref(),
    )?;
    // Open mode checks no credential, so it would silently ignore the members of
    // a team workspace and every role. A home that holds members refuses to
    // start there (ADR 0052); one that cannot be read refuses too.
    if token.is_none() && !host.is_loopback() {
        match Store::new(home.clone()).team_mode() {
            Ok(false) => {}
            Ok(true) => {
                return Err(Failure {
                    message: format!(
                        "tandem serve: this home holds members, and --allow-unauthenticated \
                         checks no credential, so it would ignore them. Set {TOKEN_VARIABLE} \
                         (at least {MIN_TOKEN_LENGTH} characters) and put a TLS terminator in \
                         front instead."
                    ),
                    exit: 2,
                });
            }
            Err(error) => {
                return Err(Failure {
                    message: format!(
                        "tandem serve: the home's members could not be read ({error}), so \
                         --allow-unauthenticated is refused."
                    ),
                    exit: 2,
                });
            }
        }
    }
    // Installed after the host checks, before any event can fire: the listening
    // line below is still the first stdout line, because nothing logs before it.
    let invalid_filter = install_log("info", io::stdout, io::stdout().is_terminal());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            Failure::message(format!(
                "tandem serve: the runtime could not start: {error}."
            ))
        })?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((host, args.port))
            .await
            .map_err(|error| {
                Failure::message(format!(
                    "tandem serve: could not bind {host}:{}: {error}.",
                    args.port
                ))
            })?;
        let address = listener.local_addr().map_err(|error| {
            Failure::message(format!(
                "tandem serve: the bound address is unreadable: {error}."
            ))
        })?;
        println!("tandem serve: listening on http://{address}");
        if let Some(origin) = &public_origin {
            println!("tandem serve: public origin {origin}");
        }
        let _ = io::stdout().flush();
        if invalid_filter {
            tracing::warn!("{LOG_VARIABLE} is not a valid log filter; logging at info");
        }
        if args.open {
            open_browser(&format!("http://{address}"));
        }
        let mut access = match &token {
            Some(token) => tandem_core::serve::Access::token(address.port(), token),
            None if host.is_loopback() => tandem_core::serve::Access::loopback(address.port()),
            None => tandem_core::serve::Access::open(address.port()),
        };
        if let Some((key, next)) = &operator_keys {
            access = access.with_operator_keys(key, next.as_deref());
        }
        if let Some(origin) = &public_origin {
            access = access.with_public_origin(origin).map_err(|reason| {
                Failure::message(format!("tandem serve: the public origin {reason}"))
            })?;
        }
        // An older home moves its notes folder once, before the first request;
        // a clash refuses to start rather than serve an empty notes.
        tandem_core::notes::migrate::migrate_home(home).map_err(Failure::domain)?;
        let context = tandem_core::serve::SessionContext {
            store: Store::new(home.clone()),
            actor: Actor::current(),
            environment: environment(),
            access,
            export_check: tandem_core::serve::ExportCheck::standard(),
        };
        tracing::info!(
            address = %address,
            home = ?home.directory(),
            actor = ?context.actor.as_str(),
            version = %tandem_core::mcp::server_version(),
            origin = public_origin.as_deref().unwrap_or("none"),
            "tandem serve started"
        );
        let (changes, _) = tokio::sync::broadcast::channel(64);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher =
            tandem_core::serve::watch(context.store.clone(), changes.clone(), stop.clone());
        let app = tandem_core::serve::router_with(context, changes);
        let outcome = tokio::select! {
            result = axum::serve(listener, app) => result.map_err(|error| {
                tracing::error!(error = ?error, "the server stopped");
                Failure::message(format!("tandem serve: the server stopped: {error}."))
            }),
            _ = shutdown_signal() => {
                tracing::info!("tandem serve stopping: signal received");
                Ok(())
            }
        };
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        // The watcher notices the stop on its next tick and exits; the process
        // is ending, so the handle is dropped rather than joined.
        drop(watcher);
        outcome
    })
}

/// SIGINT, or SIGTERM on unix. The serve future is dropped on either, which
/// closes the listener and the open streams.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// `--open`: the system browser, after the listener has a real port. A browser
/// that cannot start is not a server failure; the address is already printed.
fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "cmd"
    } else {
        "xdg-open"
    };
    let mut command = std::process::Command::new(program);
    if cfg!(windows) {
        command.args(["/C", "start", ""]);
    }
    let _ = command.arg(url).spawn();
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
    if report.checks.iter().any(|check| check.name == "bikeshed") {
        eprintln!(
            "doctor: the `bikeshed` check is deprecated and goes in the next release; read \
             `notes`."
        );
    }
    if report.ok {
        return Ok(());
    }
    let failures: Vec<String> = report
        .checks
        .iter()
        // The deprecated alias repeats the `notes` row; say each failure once.
        .filter(|check| !check.ok && check.name != "bikeshed")
        .map(|check| format!("doctor: {} \u{2014} {}", check.name, check.detail))
        .collect();
    Err(Failure {
        message: failures.join("\n"),
        exit: 1,
    })
}

// MARK: - Export and import

/// `export --out <path|->`: the home as one archive (ADR 0050). It opens the
/// home directly like every local verb, takes no write gate, and runs beside a
/// live `tandem serve`. The record line goes to stderr: this verb installs no
/// log subscriber.
fn export(home: &Home, args: &ExportArgs) -> Result<(), Failure> {
    let out = args.out.as_deref().ok_or_else(Failure::usage)?;
    // An older home's notes move first, so the archive carries them.
    tandem_core::notes::migrate::migrate_home(home).map_err(Failure::domain)?;
    let principal = archive::Principal {
        access: archive::Access::Local,
        actor: Actor::current(),
    };
    if out == "-" {
        if io::stdout().is_terminal() {
            return Err(Failure::message(
                "Cannot export to a terminal: the archive is binary. Pass --out <path>, or pipe \
                 standard output into a file or `tandem import -`.",
            ));
        }
        let line = start_export(&principal)?;
        eprintln!("{line}");
        let writer = io::BufWriter::new(io::stdout().lock());
        let summary = archive::export(home, writer)
            .map_err(|error| match error {
                // A write to the pipe may have failed after bytes went out.
                archive::ArchiveError::Io(message) => archive::ArchiveError::Stream(message),
                other => other,
            })
            .map_err(archive_failure)?;
        eprintln!(
            "Exported {} bytes to standard output. Next: restore it with `tandem import -` \
             against a new, empty --home.",
            summary.bytes
        );
        return Ok(());
    }
    let path = Path::new(out);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            Failure::message(format!(
                "Cannot export to {}: it already exists. Export never overwrites; choose a new \
                 path.",
                path.display()
            ))
        } else {
            Failure::message(format!(
                "Cannot create {}: {error}. Check the path and try again.",
                path.display()
            ))
        }
    })?;
    let outcome = start_export(&principal).and_then(|line| {
        eprintln!("{line}");
        archive::export(home, io::BufWriter::new(file)).map_err(archive_failure)
    });
    match outcome {
        Ok(summary) => {
            println!("{} ({} bytes)", path.display(), summary.bytes);
            println!(
                "Next: restore it with `tandem import {}` against a new, empty --home.",
                path.display()
            );
            Ok(())
        }
        Err(failure) => {
            let _ = std::fs::remove_file(path);
            Err(failure)
        }
    }
}

/// The permission check and the record: the one entry the door calls before it
/// streams a byte.
fn start_export(principal: &archive::Principal) -> Result<String, Failure> {
    archive::start_export(principal, archive::Door::Cli).map_err(archive_failure)
}

/// `import <path|->`: restore an archive into an empty home (ADR 0050). Every
/// refusal exits non-zero and leaves the home as it was.
fn import(home: &Home, args: &ImportArgs) -> Result<(), Failure> {
    let source = args.path.as_deref().ok_or_else(Failure::usage)?;
    let summary = if source == "-" {
        archive::import(home, io::BufReader::new(io::stdin().lock()), args.max_bytes)
    } else {
        let file = std::fs::File::open(source).map_err(|error| {
            Failure::message(format!(
                "Cannot read the archive at {source}: {error}. Check the path and try again."
            ))
        })?;
        archive::import(home, io::BufReader::new(file), args.max_bytes)
    }
    .map_err(archive_failure)?;
    println!(
        "Imported into {}: {} ticket{}, {} note{}.",
        home.directory().display(),
        summary.tickets,
        if summary.tickets == 1 { "" } else { "s" },
        summary.notes,
        if summary.notes == 1 { "" } else { "s" },
    );
    println!(
        "Next: start `tandem serve` on this home, sign in again (no browser session travels) and \
         issue new tokens (no token travels)."
    );
    Ok(())
}

fn archive_failure(error: archive::ArchiveError) -> Failure {
    Failure::message(error.to_string())
}

// MARK: - the run's environment

/// The process's environment as the MCP door reads it: `TANDEM_HOME`,
/// `TANDEM_AGENT` and the harness session pointer. A value that is not UTF-8 is
/// not one of those names and is dropped.
fn environment() -> Environment {
    tandem_core::env::process()
}

// MARK: - The wire's parity gate

/// The machine half of the wire contract: every method the wire
/// speaks names a CLI verb, and the grammar answers it. A mutation message
/// without a verb is a UI-only write, which the contract forbids. A read may name
/// no verb: it writes nothing, and `usage.report` is the one such method.
#[cfg(test)]
mod version_flag {
    use super::*;
    use clap::CommandFactory;

    /// The release build stamps `TANDEM_VERSION`, and the smoke test on every
    /// platform reads it back from `tandem --version`. A dev build falls back to
    /// the crate version, so this pins the wiring rather than one value.
    #[test]
    fn the_version_flag_names_the_release_stamp() {
        assert_eq!(
            Cli::command().get_version(),
            Some(tandem_core::release_version())
        );
    }
}

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
