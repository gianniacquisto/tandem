//! `tandem rpc`: the app's door to the core, one JSON frame per line.
//!
//! The process owns three things and nothing else (`docs/port.md`, Transport and
//! the UI contract):
//!
//! - **The stream** ([`Stream`]): stdio today, HTTPS later. The trait is the
//!   seam, so a remote adapter replaces the transport and never the message
//!   set. One implementation ships, and the reader and writer halves of stdio
//!   are separate handles, so an event can be written while a frame is read.
//! - **The dispatch** ([`handle`]): one frame in, one frame out, through the
//!   same domain calls the CLI's verbs make. A malformed frame is an error
//!   frame and never a panic: stdin is untrusted input, and the child may hold
//!   the store's lock on the next request.
//! - **The watch**: a thread stats the store's files (`tandem.db` and its
//!   `-wal`) every 40 ms on one read-only connection it keeps open, and reads
//!   the revision only when the files move, pushing [`Event::Changed`] when
//!   any writer moves it, this process's own writes included. Watching the
//!   files (rather than querying the revision) is what keeps an idle child
//!   quiet on every platform with no new dependency; the 40 ms cadence is the
//!   latency ceiling and a 5 s fallback covers a filesystem that reports
//!   nothing. Reusing the connection is what keeps an idle child from opening
//!   one a tick. An event is a hint to re-read: it may name
//!   an intermediate revision (a migration is a commit of its own) and the
//!   listing's revision is the authoritative one.
//!
//! The lifecycle is stdin's: when the parent closes the pipe, the read returns
//! end of file, the loop ends, the watch stops and the process exits. The app
//! spawns the child and dies with it; nothing outlives the pipe, and no lock is
//! held between requests, because every request opens the store for its own
//! transaction.

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::bikeshed::{Bikeshed, CaptureKind};
use crate::domain::harness;
use crate::domain::session::{
    Ending, HarnessSession, HarnessTransport, SessionOutcome, SessionStatus, Sessions, parse_skills,
};
use crate::domain::sprint::{SprintMode, SprintStatus, Sprints};
use crate::domain::usage::{
    Calendar, UsageTicket, UsageWindow, filter_sessions_by_window, usage_report,
};
use crate::domain::workflow::{WorkflowDraft, Workflows};
use crate::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, Filter, Settings, Status, TicketQuery,
    resolve_assignee, resolve_origin,
};
use crate::setup::Environment;
use crate::store::{Actor, Store};
use crate::wire::rpc::{
    ArchivedReply, CommentListReply, CommentReply, DoctorReply, Event, HostRoot, LaneDeletionReply,
    LaneListReply, MarkdownReply, Method, NoteAssetReply, NoteBacklinksReply, NoteHealthReply,
    NoteMoveReply, NoteReadReply, NoteRecoveriesReply, NoteTreeReply, NoteWriteReply, Outcome,
    ProjectEntry, ProjectListReply, Reply, Request, Response, SessionDetailReply, SessionListReply,
    SessionReply, SessionTranscriptReply, SettingsReply, SetupReply, SprintListReply, SprintReply,
    TicketListReply, TicketReply, UsageReportReply, WireError, WorkflowListReply, WorkflowReply,
};
use crate::wire::{
    BacklinkView, CommentView, MarkdownBlockView, NoteAssetView, NoteContentView, NoteMoveView,
    NoteNodeView, NotePathView, NoteWriteView, RecoveryView, TicketDetail, TicketSummary,
    UsageReportView, slashed_path,
};
use crate::{diagnostics, setup};

/// The most one frame may carry. A description is text a human wrote; a
/// megabyte is far past any of them, and the cap keeps an untrusted writer from
/// growing this process's heap one line at a time. A frame of exactly this many
/// bytes is legal; the newline that ends it is the stream's, not the frame's.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

/// How often the watch stats the store's files, on the one connection it
/// keeps open. Short enough that the face updates before a human notices.
/// A tick stats twice and queries nothing unless the files moved, so an idle
/// child costs no query; the revision is read only on a move or the fallback.
const WATCH_INTERVAL: Duration = Duration::from_millis(40);

/// How often the watch re-reads the revision even when the files look still.
/// A filesystem that reports no mtime or size change must not silence the
/// stream: the fallback bounds the staleness at five seconds while the fast
/// path stays quiet.
const WATCH_FALLBACK: Duration = Duration::from_secs(5);

/// The longest wait after a store the watch cannot attach to: refused,
/// unreadable, or readable with no revision row yet. A legacy or broken home
/// must not be reopened 25 times a second: the watch doubles its wait on every
/// such attempt up to this cap, and resets on a successful attach.
const WATCH_BACKOFF_MAX: Duration = Duration::from_secs(5);

/// One step of the watch's backoff: twice the wait, capped.
fn backoff(wait: Duration) -> Duration {
    (wait * 2).min(WATCH_BACKOFF_MAX)
}

// MARK: - The stream

/// One transport for the protocol's frames: a line in, a line out.
///
/// The trait exists so the transport is an adapter and not a rewrite
/// (`docs/port.md`, Transport): stdio is the one implementation today, and an
/// HTTPS adapter later carries the same [`Request`]s and [`Response`]s. Both
/// methods take `&self` because the reader and the writer must run at once: a
/// change event may be written while the next request is read.
pub trait Stream: Send + Sync + 'static {
    /// The next frame, or `None` at end of stream. A frame that is not UTF-8
    /// or is past [`MAX_FRAME`] answers [`io::ErrorKind::InvalidData`]; the
    /// stream itself is still usable.
    fn read_frame(&self) -> io::Result<Option<String>>;

    /// Write one frame and flush it: a line that has not reached the pipe is a
    /// message the app never saw.
    fn write_frame(&self, frame: &str) -> io::Result<()>;
}

/// The stdio adapter: stdin for frames in, stdout for frames out.
///
/// `stderr` is the process's own and never carries a frame, so a diagnostic
/// cannot be mistaken for a response.
pub struct StdioStream {
    input: io::Stdin,
    output: Mutex<io::Stdout>,
}

impl StdioStream {
    pub fn new() -> Self {
        Self {
            input: io::stdin(),
            output: Mutex::new(io::stdout()),
        }
    }
}

impl Default for StdioStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for StdioStream {
    fn read_frame(&self) -> io::Result<Option<String>> {
        let mut input = self.input.lock();
        let mut buffer = Vec::new();
        {
            // The limit is two bytes past the cap: enough for a legal frame's
            // content, a CR and the newline that ends it. A frame of exactly
            // `MAX_FRAME` bytes therefore arrives whole, newline included,
            // instead of being refused for the byte its terminator took.
            let mut limited = (&mut input).take(MAX_FRAME as u64 + 2);
            limited.read_until(b'\n', &mut buffer)?;
        }
        if buffer.is_empty() {
            return Ok(None);
        }
        let terminated = buffer.ends_with(b"\n");
        if !terminated && buffer.len() == MAX_FRAME + 2 {
            // The limit was reached before the line ended, so the rest of this
            // frame is still on the stream: drain to its own newline, and only
            // to that one, so the next read starts at a frame boundary and the
            // client's next request survives. A short unterminated buffer is a
            // last frame at end of stream, not an overflow.
            let mut discard = Vec::new();
            loop {
                discard.clear();
                if input.read_until(b'\n', &mut discard)? == 0 || discard.ends_with(b"\n") {
                    break;
                }
            }
        }
        if terminated {
            buffer.pop();
            if buffer.ends_with(b"\r") {
                buffer.pop();
            }
        }
        if buffer.len() > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the frame is longer than {MAX_FRAME} bytes"),
            ));
        }
        String::from_utf8(buffer)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "the frame is not UTF-8"))
    }

    fn write_frame(&self, frame: &str) -> io::Result<()> {
        let mut output = self
            .output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        output.write_all(frame.as_bytes())?;
        output.write_all(b"\n")?;
        output.flush()
    }
}

// MARK: - The process

/// Serve frames from `stream` until it ends.
///
/// End of stream is success: a closed stdin is the app going away, which is the
/// normal way this child dies. An I/O failure on the stream is the only error;
/// everything a frame can get wrong is answered as a frame.
pub fn serve<S: Stream>(stream: S, store: Store, actor: Actor) -> io::Result<()> {
    let stream = Arc::new(stream);
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let stream = Arc::clone(&stream);
        let store = store.clone();
        let stop = Arc::clone(&stop);
        thread::spawn(move || watch(&*stream, &store, &stop))
    };

    let outcome = loop {
        match stream.read_frame() {
            Ok(None) => break Ok(()),
            Ok(Some(frame)) => {
                let answer = handle(&frame, &store, &actor);
                if let Err(error) = stream.write_frame(&answer) {
                    break Err(error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                let answer = Response {
                    id: None,
                    outcome: Outcome::Error(WireError::parse(error.to_string())),
                };
                let frame = serde_json::to_string(&answer).unwrap_or_else(|_| {
                    r#"{"id":null,"error":{"code":"parse","message":"the frame is not readable"}}"#
                        .to_string()
                });
                if let Err(error) = stream.write_frame(&frame) {
                    break Err(error);
                }
            }
            Err(error) => break Err(error),
        }
    };

    stop.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    outcome
}

/// The watch: stat the store's files on one read-only connection and push a
/// change event whenever the revision moves, until `stop`.
fn watch<S: Stream>(stream: &S, store: &Store, stop: &AtomicBool) {
    // One read-only connection, reused: an idle child stats twice a tick and
    // queries nothing, not an open and a close. The watch waits for the store
    // to exist and for its one-time WAL switch (`Store::watch`). The 40 ms
    // cadence is the latency ceiling; the 5 s fallback covers a filesystem
    // that reports no mtime or size change.
    let mut watch = store.watch().ok().flatten();
    let mut last = watch
        .as_ref()
        .and_then(|watch| watch.revision().ok())
        .unwrap_or(0);
    let mut files = store.watch_files();
    let mut last_read = Instant::now();
    let mut wait = WATCH_INTERVAL;
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(wait);
        let mut force_read = false;
        if watch.is_none() {
            match store.watch() {
                Ok(Some(found)) => {
                    watch = Some(found);
                    wait = WATCH_INTERVAL;
                    force_read = true;
                }
                // No store yet: the first write has not run, and asking again
                // costs a stat and no connection, so keep the normal cadence.
                // A store that exists but carries no revision row is a pre-v4
                // file, opened on every attempt: it backs off like a refusal
                // until the first write migrates it.
                Ok(None) => {
                    wait = if store.exists() {
                        backoff(wait)
                    } else {
                        WATCH_INTERVAL
                    };
                    continue;
                }
                // A refused store (a legacy file, an unreadable one)
                // must not spin: back off until it changes.
                Err(_) => {
                    wait = backoff(wait);
                    continue;
                }
            }
        }
        let Some(current) = watch.as_ref() else {
            continue;
        };
        let now_files = store.watch_files();
        let files_moved = now_files != files;
        let fallback_due = last_read.elapsed() >= WATCH_FALLBACK;
        if !force_read && !files_moved && !fallback_due {
            continue;
        }
        let Ok(revision) = current.revision() else {
            // The connection is unusable (the store was replaced or
            // removed): drop it and reopen with the backoff.
            watch = None;
            wait = backoff(wait);
            continue;
        };
        last_read = Instant::now();
        if revision == last {
            // A tick can land mid-commit: the WAL grows before COMMIT moves
            // the revision, so a file move with no revision move is a write
            // in flight, not noise. Keep the old baseline so the next tick
            // retries the read once the commit has landed; advancing it here
            // would lose the event until the fallback. A rolled-back write
            // can leave the WAL grown with no revision move, in which case
            // the ticks keep rereading until the next commit re-baselines.
            continue;
        }
        files = now_files;
        last = revision;
        let frame = match serde_json::to_string(&Event::changed(revision)) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        if stream.write_frame(&frame).is_err() {
            return;
        }
    }
}

// MARK: - One frame

/// Answer one frame: a request becomes a result or an error, and a frame that
/// is not a request becomes a parse error.
///
/// This is the whole protocol minus the transport, so the suite can drive it
/// without a process and the pipe test can drive it through one.
pub fn handle(frame: &str, store: &Store, actor: &Actor) -> String {
    handle_in(frame, store, actor, &process_environment())
}

/// [`handle`] with the environment injected, so a test names a throwaway home
/// and a fixture bundle instead of the process's own (`setup` and `doctor` read
/// `HOME`, `XDG_CONFIG_HOME` and `TANDEM_AGENT_BUNDLE`).
pub fn handle_in(frame: &str, store: &Store, actor: &Actor, environment: &Environment) -> String {
    let response = match serde_json::from_str::<Request>(frame) {
        Ok(request) => {
            let id = request.id;
            match execute(request.method, store, actor, environment) {
                Ok(reply) => Response {
                    id: Some(id),
                    outcome: Outcome::Result(reply),
                },
                Err(error) => Response {
                    id: Some(id),
                    outcome: Outcome::Error(error),
                },
            }
        }
        Err(error) => Response {
            id: frame_id(frame),
            outcome: Outcome::Error(classify(frame, &error)),
        },
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"id":null,"error":{"code":"parse","message":"the answer is not writable"}}"#.to_string()
    })
}

/// The id a frame that failed to parse still carried, when it carried one.
fn frame_id(frame: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(frame)
        .ok()
        .and_then(|value| value.get("id").and_then(serde_json::Value::as_u64))
}

/// Why a frame was refused: an unknown method is a method error, everything
/// else is a parse error. The distinction is what lets a client tell a version
/// skew from a malformed call.
fn classify(frame: &str, error: &serde_json::Error) -> WireError {
    let value = serde_json::from_str::<serde_json::Value>(frame).ok();
    let method = value
        .as_ref()
        .and_then(|value| value.get("method"))
        .and_then(serde_json::Value::as_str);
    match method {
        Some(name)
            if !crate::wire::rpc::METHODS
                .iter()
                .any(|(tag, _)| *tag == name) =>
        {
            WireError::method(format!("the wire does not answer the method `{name}`"))
        }
        Some(name) => WireError::parse(format!(
            "the frame's params for `{name}` are not readable: {error}"
        )),
        None => WireError::parse(format!("the frame is not a request: {error}")),
    }
}

// MARK: - Dispatch

/// One method through the domain. Every arm is the CLI verb's call, with the
/// same defaults: the wire adds no rule of its own.
///
/// `environment` is the process's, injected so a test can name a throwaway
/// home and a fixture bundle: the `setup` and `doctor` verbs are the machine's
/// reads, and the app gets them over this door rather than opening the
/// filesystem itself.
fn execute(
    method: Method,
    store: &Store,
    actor: &Actor,
    environment: &Environment,
) -> Result<Reply, WireError> {
    let board = Board::with_actor(store.clone(), actor.clone());
    match method {
        Method::TicketList(params) => {
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let query = TicketQuery::parse(
                params.filter.as_deref(),
                params.text.as_deref(),
                params.status.as_deref(),
                assignee.as_deref(),
                params.lane.as_deref(),
                params.archived,
                params.blocked,
            )
            .map_err(domain)?;
            if params.status.is_some() {
                for status in &query.statuses {
                    board.validate_status(status).map_err(domain)?;
                }
            }
            let (tickets, revision) = board.list_snapshot(&query).map_err(domain)?;
            Ok(Reply::TicketList(TicketListReply {
                revision,
                tickets: tickets.iter().map(TicketSummary::new).collect(),
            }))
        }
        Method::TicketShow(params) => {
            let (ticket, revision) = board.ticket_snapshot(named(&params.id)).map_err(domain)?;
            Ok(Reply::Ticket(TicketReply {
                revision,
                ticket: TicketDetail::new(&ticket),
            }))
        }
        Method::TicketAdd(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(domain)?,
                None => board.landing_status().map_err(domain)?,
            };
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let origin = resolve_origin(params.origin.as_deref(), actor).map_err(domain)?;
            let description = match params.description.as_deref() {
                None => None,
                Some(raw) => Some(DescriptionBody::required(raw).map_err(domain)?),
            };
            let ticket = board
                .add(
                    params.title.as_deref().unwrap_or(""),
                    AddTicket {
                        lane: params.lane.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: description.as_deref(),
                        origin: &origin,
                    },
                )
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketAddFromNote(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(domain)?,
                None => board.landing_status().map_err(domain)?,
            };
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let origin = resolve_origin(params.origin.as_deref(), actor).map_err(domain)?;
            let ticket = board
                .add_from_note(
                    params.text.as_deref().unwrap_or(""),
                    named(&params.note),
                    AddTicket {
                        lane: params.lane.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: None,
                        origin: &origin,
                    },
                )
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketSet(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Edit::Set(Status::parse(raw).map_err(domain)?),
                None => Edit::Unchanged,
            };
            let resolved_assignee =
                resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let assignee = match params.assignee.as_deref() {
                Some(_) => Edit::Set(resolved_assignee.as_deref()),
                None => Edit::Unchanged,
            };
            let description = match params.description.as_deref() {
                Some(raw) => Edit::Set(Some(raw)),
                None => Edit::Unchanged,
            };
            let ticket = board
                .set(
                    named(&params.id),
                    crate::domain::SetFields {
                        title: match params.title.as_deref() {
                            Some(title) => Edit::Set(title),
                            None => Edit::Unchanged,
                        },
                        status,
                        assignee,
                        description,
                        append_description: Edit::Unchanged,
                        archived: Edit::Unchanged,
                    },
                )
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketMove(params) => {
            let at = match params.index {
                None => usize::MAX,
                Some(index) => index.max(0) as usize,
            };
            let ticket = board
                .move_ticket(named(&params.id), params.lane.as_deref(), at)
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketBlock(params) => {
            let ticket = board
                .block(named(&params.id), params.on.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketUnblock(params) => {
            let ticket = board
                .unblock(named(&params.id), params.on.as_deref())
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketComplete(params) => {
            let ticket = board.complete(named(&params.id)).map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketArchive(params) => match params.id.as_deref() {
            Some(id) => {
                let ticket = board.archive(id).map_err(domain)?;
                Ok(ticket_receipt(&ticket, store))
            }
            None => {
                let archived = board.archive_done().map_err(domain)?;
                Ok(Reply::Archived(ArchivedReply {
                    revision: store.last_revision(),
                    archived,
                }))
            }
        },
        Method::TicketCommentAdd(params) => {
            let comment = board
                .append_comment(
                    named(&params.id),
                    params.author.as_deref().unwrap_or(""),
                    params.body.as_deref().unwrap_or(""),
                )
                .map_err(domain)?;
            Ok(Reply::Comment(CommentReply {
                revision: store.last_revision(),
                comment: CommentView::new(&comment),
            }))
        }
        Method::TicketCommentList(params) => {
            let (comments, revision) =
                board.comments_snapshot(named(&params.id)).map_err(domain)?;
            Ok(Reply::CommentList(CommentListReply {
                revision,
                comments: comments.iter().map(CommentView::new).collect(),
            }))
        }
        Method::UsageReport(params) => {
            // The derivation is the one `usage_report` froze (TAN-208): the
            // dispatch only feeds it the records and the board's labels, the way
            // the Swift window fed it the model it already held. The window
            // narrows the sessions first, so one narrowed report feeds the
            // header, the tables and the graph; absent is All time for old
            // callers, and the derivation itself stays pure and clock-free.
            let window = params.window.unwrap_or(UsageWindow::AllTime);
            let sessions = Sessions::new(store).list(None, None).map_err(domain)?;
            let today = local_today();
            let narrowed = filter_sessions_by_window(&sessions, &window, &today, &Calendar::Local);
            let tickets: Vec<UsageTicket> = Board::new(store.clone())
                .list_filter(Filter::All)
                .map_err(domain)?
                .into_iter()
                .map(|ticket| UsageTicket {
                    id: ticket.id.to_string(),
                    reference: ticket.reference.clone(),
                    title: ticket.title.clone(),
                })
                .collect();
            // The reader's calendar: a day is the day the human lived through.
            let report = usage_report(&narrowed, &tickets, &Calendar::Local);
            Ok(Reply::UsageReport(UsageReportReply {
                report: UsageReportView::new(&report),
            }))
        }
        Method::SettingsGet(_) => {
            // The roster is a `settings` row, and the picking is the domain's
            // (`domain::Settings`): a default for a home that never stored one,
            // the stored list over it, and an explicitly empty list as empty.
            // The focus answers on the same read, resolved against the declared
            // lanes, so a stale focus is no focus. The read is rows and no
            // lock: the store opens, answers and closes inside `Store::setting`.
            let settings = Settings::new(store);
            let roster = settings.roster().map_err(domain)?;
            let lanes = board.lanes().map_err(domain)?;
            let active_project = settings.active_project(&lanes).map_err(domain)?;
            Ok(Reply::Settings(SettingsReply {
                roster,
                active_project,
            }))
        }
        Method::SettingsSet(params) => {
            // One write, two axes: each is applied only when its key is
            // present, so a roster Save leaves the focus alone and a focus
            // pick leaves the roster alone. The focus resolves first, so a
            // refused lane writes nothing, not even the spelled roster. A
            // frame that spells neither key is the legacy bare write: it
            // clears the focus and leaves the roster alone, because on the
            // wire an explicit null reads as absent, and clearing the focus
            // must never empty the roster. The receipt is what the store
            // holds after both axes, so the face redraws from this reply,
            // never from its request's echo, and the revision moves, so the
            // faces re-read on the change event.
            let settings = Settings::new(store);
            let lanes = board.lanes().map_err(domain)?;
            let active_project = match params.project.as_deref() {
                Some(project) => settings
                    .set_active_project(actor, Some(project), &lanes)
                    .map_err(domain)?,
                None if params.roster.is_none() => settings
                    .set_active_project(actor, None, &lanes)
                    .map_err(domain)?,
                None => settings.active_project(&lanes).map_err(domain)?,
            };
            let roster = match params.roster {
                Some(roster) => settings.set_roster(actor, roster).map_err(domain)?,
                None => settings.roster().map_err(domain)?,
            };
            Ok(Reply::Settings(SettingsReply {
                roster,
                active_project,
            }))
        }
        Method::MarkdownBlocks(params) => {
            // The parsing is the domain's (`TicketMarkdown`), so the sheet draws
            // the same blocks the Swift sheet drew from the same dialect. The
            // message touches no store: it is a pure projection of the text the
            // face already holds, and it answers without a lock.
            Ok(Reply::Markdown(MarkdownReply {
                blocks: MarkdownBlockView::blocks(&crate::domain::TicketMarkdown::blocks(
                    params.text.as_deref(),
                )),
            }))
        }
        Method::SessionList(params) => {
            // The records, newest first: the CLI/MCP listing by project, and
            // the sheet's Sessions section by ticket (TAN-264). A ticket the
            // caller names narrows to the records that carried it; a blank
            // ticket or project is no filter, as an absent filter is none on
            // `ticket.list`. A status the store does not know is the domain's
            // refusal, the same one `session list` makes.
            let status = match params.status.as_deref() {
                Some(raw) if !raw.trim().is_empty() => {
                    Some(SessionStatus::parse(raw.trim()).ok_or_else(|| {
                        WireError::domain(format!(
                            "session status must be one of running, ended (got \"{raw}\")."
                        ))
                    })?)
                }
                _ => None,
            };
            let project = params
                .project
                .as_deref()
                .map(str::trim)
                .filter(|project| !project.is_empty());
            let ticket = params
                .ticket
                .as_deref()
                .map(str::trim)
                .filter(|ticket| !ticket.is_empty());
            let records = match ticket {
                Some(ticket) => {
                    let carried = Sessions::new(store).for_ticket(ticket).map_err(domain)?;
                    carried
                        .into_iter()
                        .filter(|session| project.is_none_or(|name| session.project == name))
                        .filter(|session| status.is_none_or(|state| session.status == state))
                        .collect()
                }
                None => Sessions::new(store).list(project, status).map_err(domain)?,
            };
            Ok(Reply::SessionList(SessionListReply {
                revision: store.last_revision(),
                sessions: records,
            }))
        }
        Method::SessionTranscript(params) => session_transcript(
            store,
            environment,
            named(&params.id),
            params.cursor,
            params.limit,
        ),
        Method::LaneList(_) => {
            let (lanes, revision) = board.lanes_snapshot().map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply { revision, lanes }))
        }
        Method::LaneAdd(params) => {
            let lanes = board
                .add_lane(
                    params.name.as_deref().unwrap_or(""),
                    params.abbr.as_deref(),
                    params.project.as_deref(),
                )
                .map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply {
                revision: store.last_revision(),
                lanes,
            }))
        }
        Method::LaneRename(params) => {
            let lanes = board
                .rename_lane(
                    params.from.as_deref().unwrap_or(""),
                    params.to.as_deref().unwrap_or(""),
                    params.abbr.as_deref(),
                    params.project.as_deref(),
                )
                .map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply {
                revision: store.last_revision(),
                lanes,
            }))
        }
        Method::LaneDelete(params) => {
            let deletion = board
                .delete_lane(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::LaneDeletion(LaneDeletionReply {
                revision: store.last_revision(),
                lanes: deletion.lanes,
                moved: deletion.moved,
            }))
        }
        Method::LaneMove(params) => {
            let at = match params.index {
                Some(index) if index >= 0 => index as usize,
                Some(_) => 0,
                None => usize::MAX,
            };
            let lanes = board
                .move_lane(params.name.as_deref().unwrap_or(""), at)
                .map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply {
                revision: store.last_revision(),
                lanes,
            }))
        }
        Method::ProjectList(_) => {
            let (projects, revision) = board.projects_snapshot().map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision,
                projects: projects
                    .into_iter()
                    .map(|entry| ProjectEntry {
                        name: entry.name,
                        lanes: entry.lanes,
                    })
                    .collect(),
            }))
        }
        Method::ProjectAdd(params) => {
            let projects = board
                .add_project(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: projects
                    .into_iter()
                    .map(|entry| ProjectEntry {
                        name: entry.name,
                        lanes: entry.lanes,
                    })
                    .collect(),
            }))
        }
        Method::ProjectRename(params) => {
            let projects = board
                .rename_project(
                    params.from.as_deref().unwrap_or(""),
                    params.to.as_deref().unwrap_or(""),
                )
                .map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: projects
                    .into_iter()
                    .map(|entry| ProjectEntry {
                        name: entry.name,
                        lanes: entry.lanes,
                    })
                    .collect(),
            }))
        }
        Method::ProjectDelete(params) => {
            let projects = board
                .delete_project(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: projects
                    .into_iter()
                    .map(|entry| ProjectEntry {
                        name: entry.name,
                        lanes: entry.lanes,
                    })
                    .collect(),
            }))
        }
        Method::WorkflowList(_) => {
            let workflows = Workflows::new(store);
            let (list, revision) = workflows.list_snapshot().map_err(domain)?;
            let active = Workflows::new(store).active().map_err(domain)?.id;
            Ok(Reply::WorkflowList(WorkflowListReply {
                revision,
                workflows: list,
                active,
            }))
        }
        Method::WorkflowShow(params) => {
            let workflows = Workflows::new(store);
            let (workflow, revision) = match params
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                Some(id) => workflows.get_snapshot(id).map_err(domain)?,
                None => workflows.active_snapshot().map_err(domain)?,
            };
            Ok(Reply::Workflow(WorkflowReply { revision, workflow }))
        }
        Method::WorkflowSet(params) => {
            let draft = WorkflowDraft {
                name: params.name.clone().unwrap_or_default(),
                scope: params.scope.clone().unwrap_or_else(|| "home".to_string()),
                stages: params.stages.clone(),
            };
            let workflow = Workflows::new(store)
                .set(actor, params.id.as_deref(), draft)
                .map_err(domain)?;
            Ok(Reply::Workflow(WorkflowReply {
                revision: store.last_revision(),
                workflow,
            }))
        }
        Method::WorkflowUse(params) => {
            let workflow = Workflows::new(store)
                .use_workflow(actor, params.id.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::Workflow(WorkflowReply {
                revision: store.last_revision(),
                workflow,
            }))
        }
        Method::WorkflowDelete(params) => {
            let workflows = Workflows::new(store)
                .delete(actor, params.id.as_deref().unwrap_or(""))
                .map_err(domain)?;
            let active = Workflows::new(store).active().map_err(domain)?.id;
            Ok(Reply::WorkflowList(WorkflowListReply {
                revision: store.last_revision(),
                workflows,
                active,
            }))
        }
        Method::SprintList(params) => {
            // The status words are the verb's own: an unknown one is refused
            // in the CLI's words, so both doors say one thing.
            let status = match params.status.as_deref() {
                Some(raw) => Some(
                    SprintStatus::parse(raw)
                        .ok_or_else(|| domain(DomainError::InvalidSprintStatus(raw.to_string())))?,
                ),
                None => None,
            };
            let (sprints, revision) = Sprints::new(store)
                .list_snapshot(params.lane.as_deref(), status)
                .map_err(domain)?;
            Ok(Reply::SprintList(SprintListReply { revision, sprints }))
        }
        Method::SprintShow(params) => {
            let (sprint, revision) = Sprints::new(store)
                .open_in_snapshot(named(&params.lane))
                .map_err(domain)?;
            Ok(Reply::Sprint(SprintReply {
                revision,
                sprint,
                note: None,
            }))
        }
        Method::SprintOpen(params) => {
            // The mode defaults and validates as the CLI's `sprint open`
            // does: absent is serial, and an unknown word is the CLI's own
            // refusal. A lane with an open sprint answers it unchanged, with
            // the note the CLI prints on a second open.
            let mode = match params.mode.as_deref() {
                None => SprintMode::Serial,
                Some(raw) => SprintMode::parse(raw)
                    .ok_or_else(|| domain(DomainError::InvalidSprintMode(raw.to_string())))?,
            };
            // The opener resolves as the verb resolves it: the caller's value
            // (an unknown one is the verb's refusal), else the harness session
            // at open time, which the record stores as its own id. An absent
            // session the environment cannot resolve stays absent, so the
            // app's null-session open keeps recording null.
            let sessions = Sessions::new(store);
            let session = match params.session.as_deref() {
                Some(_) => Some(resolve_session(
                    &sessions,
                    params.session.as_deref(),
                    environment,
                )?),
                None => resolve_session(&sessions, None, environment).ok(),
            };
            let opened = Sprints::new(store)
                .open(actor, named(&params.lane), mode, session.as_deref(), None)
                .map_err(domain)?;
            Ok(Reply::Sprint(SprintReply {
                revision: store.last_revision(),
                sprint: opened.value,
                note: opened.note,
            }))
        }
        Method::SetupStatus(_) => {
            let home = store.home().directory();
            let executable = std::env::current_exe().unwrap_or_default();
            let is_debug_build = cfg!(debug_assertions);
            let bundle = setup::bundle(environment, &executable, is_debug_build);
            let report = setup::status_with(
                home,
                environment,
                bundle.as_deref(),
                &executable,
                is_debug_build,
            );
            Ok(Reply::Setup(setup_reply(store, environment, report)))
        }
        Method::SetupInstall(params) => {
            let hosts = setup::hosts(params.host.as_deref())
                .map_err(|error| WireError::setup(error.to_string()))?;
            let home = store.home().directory();
            let executable = std::env::current_exe().unwrap_or_default();
            let is_debug_build = cfg!(debug_assertions);
            let bundle = setup::bundle(environment, &executable, is_debug_build);
            let report = setup::install(home, environment, bundle.as_deref(), params.force, &hosts)
                .map_err(|error| WireError::setup(error.to_string()))?;
            Ok(Reply::Setup(setup_reply(store, environment, report)))
        }
        Method::DoctorReport(_) => Ok(Reply::Doctor(DoctorReply {
            report: diagnostics::run(store.home(), environment),
        })),
        Method::NoteTree(params) => {
            let bikeshed = Bikeshed::with_reveal(bikeshed_directory(store), params.hidden);
            Ok(Reply::NoteTree(NoteTreeReply {
                nodes: bikeshed.tree().iter().map(NoteNodeView::new).collect(),
            }))
        }
        Method::NoteRead(params) => {
            let path = named(&params.path);
            let content = open_bikeshed(store).read(path).map_err(domain)?.value;
            Ok(Reply::NoteRead(NoteReadReply {
                note: NoteContentView::new(path, &content),
            }))
        }
        Method::NoteAsset(params) => {
            let path = named(&params.path);
            let asset = open_bikeshed(store).asset(path).map_err(domain)?.value;
            Ok(Reply::NoteAsset(NoteAssetReply {
                asset: NoteAssetView::new(&asset),
            }))
        }
        Method::NoteWrite(params) => {
            let path = named(&params.path);
            // An absent text is not an empty note: the client sent no document,
            // and writing "" would clear a human's note. `text: ""` is the
            // explicit clear; a missing field is the refusal.
            let Some(text) = params.text.as_deref() else {
                return Err(WireError::domain("the write carries no text"));
            };
            let record = open_bikeshed(store)
                .write(path, text)
                .map_err(domain)?
                .value;
            Ok(Reply::NoteWrite(NoteWriteReply {
                write: NoteWriteView::new(&record),
            }))
        }
        Method::NoteMove(params) => {
            let outcome = open_bikeshed(store)
                .move_into(named(&params.path), named(&params.into))
                .map_err(domain)?
                .value;
            Ok(Reply::NoteMove(NoteMoveReply {
                moved: NoteMoveView::new(&outcome),
            }))
        }
        Method::NoteCreate(params) => {
            let created = open_bikeshed(store)
                .create(named(&params.path))
                .map_err(domain)?
                .value;
            Ok(Reply::NotePath(NotePathView {
                path: created,
                note: None,
            }))
        }
        Method::NoteCreateFolder(params) => {
            let created = open_bikeshed(store)
                .create_folder(named(&params.path))
                .map_err(domain)?
                .value;
            Ok(Reply::NotePath(NotePathView {
                path: created,
                note: None,
            }))
        }
        Method::NoteDaily(_) => {
            let result = open_bikeshed(store)
                .open_daily_note(crate::bikeshed::Day::today())
                .map_err(domain)?;
            Ok(Reply::NotePath(NotePathView {
                path: result.value,
                note: result.note,
            }))
        }
        Method::NoteHealth(_) => Ok(Reply::NoteHealth(NoteHealthReply {
            health: open_bikeshed(store).health().map_err(domain)?.value,
        })),
        Method::NoteBacklinks(params) => {
            let path = named(&params.path);
            let rows = open_bikeshed(store).backlinks(path).map_err(domain)?.value;
            Ok(Reply::NoteBacklinks(NoteBacklinksReply {
                path: path.to_string(),
                backlinks: rows.iter().map(BacklinkView::new).collect(),
            }))
        }
        Method::NoteRecoveries(_) => Ok(Reply::NoteRecoveries(NoteRecoveriesReply {
            recoveries: open_bikeshed(store)
                .recoveries()
                .iter()
                .map(RecoveryView::new)
                .collect(),
        })),
        Method::SessionStart(params) => {
            let id = match params.session.as_deref() {
                Some(raw) => raw.to_string(),
                None => uuid::Uuid::new_v4().to_string(),
            };
            let project = match params.project.as_deref() {
                Some(project) => project.to_string(),
                None => project_default(),
            };
            let agent = session_agent(params.agent.as_deref(), environment);
            let brief = params.brief.as_deref().unwrap_or("");
            let harness = session_harness(&params, environment)?;
            let sessions = Sessions::new(store);
            let result = match harness {
                Some(harness) => sessions.start_with_harness(
                    actor,
                    &id,
                    &project,
                    agent.as_deref(),
                    brief,
                    harness,
                    None,
                ),
                None => sessions.start(actor, &id, &project, agent.as_deref(), brief, None),
            }
            .map_err(domain)?;
            Ok(Reply::Session(SessionReply {
                revision: store.last_revision(),
                session: result.value,
                note: result.note,
            }))
        }
        Method::SessionLink(params) => {
            let sessions = Sessions::new(store);
            let id = resolve_session(&sessions, params.session.as_deref(), environment)?;
            let ticket = board.ticket(named(&params.ticket)).map_err(domain)?;
            let result = sessions
                .link(actor, &id, &ticket.id.to_string())
                .map_err(domain)?;
            Ok(Reply::Session(SessionReply {
                revision: store.last_revision(),
                session: result.value,
                note: result.note,
            }))
        }
        Method::SessionEnd(params) => {
            let sessions = Sessions::new(store);
            let id = resolve_session(&sessions, params.session.as_deref(), environment)?;
            let outcome = match params.outcome.as_deref() {
                None => SessionOutcome::Unknown,
                Some(raw) => SessionOutcome::parse(raw)
                    .ok_or_else(|| domain(DomainError::InvalidSessionOutcome(raw.to_string())))?,
            };
            let summary = params.summary.as_deref().map(str::to_string);
            let mut tokens = end_tokens(&params)?;
            let mut cost = end_cost(params.cost)?;
            let mut model = end_model(params.model.as_deref());
            let mut skills = match params.skills.as_deref() {
                Some(raw) => parse_skills(raw).map_err(domain)?,
                None => None,
            };
            let mut turns = None;
            let mut calls = None;
            let mut note: Option<String> = None;
            if params.usage_from_harness.unwrap_or(true) {
                let record = sessions.show(&id).map_err(domain)?;
                match harness_usage(store, &record, params.harness_url.as_deref(), environment) {
                    Ok(reported) => {
                        tokens = reported.tokens().or(tokens);
                        cost = reported.cost.or(cost);
                        model = reported.model.or(model);
                        skills = reported.skills.or(skills);
                        turns = reported.turns.or(turns);
                        calls = reported.calls.or(calls);
                    }
                    Err(error) if is_absent_harness(&error) => {
                        note = Some(format!("{}. Stamped without usage.", error.message()));
                    }
                    Err(error) => return Err(domain(error)),
                }
            }
            let stamp = sessions
                .end(
                    actor,
                    &id,
                    Ending {
                        outcome,
                        summary: summary.as_deref(),
                        tokens,
                        cost,
                        model: model.as_deref(),
                        skills,
                        turns,
                        calls,
                        at: None,
                    },
                )
                .map_err(domain)?;
            if stamp.stamped && stamp.session.carries_usage() {
                post_usage_comments(store, &stamp.session, actor).map_err(domain)?;
            }
            let stamp_note = stamp.note.or(note);
            Ok(Reply::Session(SessionReply {
                revision: store.last_revision(),
                session: stamp.session,
                note: stamp_note,
            }))
        }
        Method::SessionShow(params) => {
            let sessions = Sessions::new(store);
            let id = resolve_session(&sessions, params.session.as_deref(), environment)?;
            let record = sessions.show(&id).map_err(domain)?;
            let notes = open_bikeshed(store)
                .session_notes(&id)
                .map_err(domain)?
                .value;
            Ok(Reply::SessionDetail(SessionDetailReply {
                revision: store.last_revision(),
                session: record,
                notes,
                note: None,
            }))
        }
        Method::SprintTicket(params) => {
            let lane = named(&params.lane);
            let ticket = board.ticket(named(&params.ticket)).map_err(domain)?;
            let number = Sprints::new(store).open_in(lane).map_err(domain)?.number;
            let result = Sprints::new(store)
                .ticket(actor, lane, number, &ticket.id.to_string())
                .map_err(domain)?;
            Ok(Reply::Sprint(SprintReply {
                revision: store.last_revision(),
                sprint: result.value,
                note: result.note,
            }))
        }
        Method::SprintRun(params) => {
            let lane = named(&params.lane);
            let note = params.note.as_deref().unwrap_or("");
            let number = Sprints::new(store).open_in(lane).map_err(domain)?.number;
            let result = Sprints::new(store)
                .run(actor, lane, number, note, None)
                .map_err(domain)?;
            Ok(Reply::Sprint(SprintReply {
                revision: store.last_revision(),
                sprint: result.value,
                note: result.note,
            }))
        }
        Method::SprintClose(params) => {
            let lane = named(&params.lane);
            let number = match params.number {
                Some(number) => number,
                None => Sprints::new(store).close_in(lane).map_err(domain)?.number,
            };
            let note = params.note.as_deref().map(str::to_string);
            let result = Sprints::new(store)
                .close(actor, lane, number, note.as_deref(), None)
                .map_err(domain)?;
            Ok(Reply::Sprint(SprintReply {
                revision: store.last_revision(),
                sprint: result.value,
                note: result.note,
            }))
        }
        Method::NoteCapture(params) => {
            store.home().ensure().map_err(|error| {
                WireError::store(format!(
                    "Cannot create the Tandem home at {}: {error}. Try again.",
                    store.home().directory().display()
                ))
            })?;
            let kind = CaptureKind::parse(params.kind.as_deref().unwrap_or("")).map_err(domain)?;
            let project = match params.project.as_deref() {
                Some(project) => project.to_string(),
                None => project_default(),
            };
            let session = match params.session.as_deref() {
                Some(session) => session.to_string(),
                None => capture_session_default(environment),
            };
            let body = params.body.as_deref().unwrap_or("");
            let vault = Bikeshed::new(store.home().directory().join("bikeshed"));
            let result = vault
                .capture(&project, kind, body, &session, std::time::SystemTime::now())
                .map_err(domain)?;
            Ok(Reply::NotePath(NotePathView {
                path: result.value,
                note: result.note,
            }))
        }
    }
}

/// The bikeshed a home keeps: `<home>/bikeshed`, the one rule the CLI's note
/// verbs and the app's reads share.
fn bikeshed_directory(store: &Store) -> std::path::PathBuf {
    store.home().directory().join("bikeshed")
}

/// The vault the note methods read and write.
fn open_bikeshed(store: &Store) -> Bikeshed {
    Bikeshed::new(bikeshed_directory(store))
}

/// The Settings surface's one read: the setup verb's report and the roots it is
/// about. The paths are the core's own rule, so the renderer names none.
fn setup_reply(store: &Store, environment: &Environment, report: setup::Report) -> SetupReply {
    let home = store.home().directory();
    let context = setup::Context::new(home, environment);
    SetupReply {
        home: slashed_path(home),
        bikeshed: slashed_path(&home.join("bikeshed")),
        hosts: setup::Host::ALL
            .into_iter()
            .map(|host| HostRoot {
                host: host.as_str().to_string(),
                path: slashed_path(&context.host_root(host)),
            })
            .collect(),
        report,
    }
}

/// The environment `setup` and `doctor` read: `HOME`, `XDG_CONFIG_HOME`,
/// `TANDEM_AGENT_BUNDLE` and the four host flags. A value that is not UTF-8 is
/// not one of those names and is dropped.
fn process_environment() -> Environment {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

/// A handle the caller named; an absent one is the empty string the CLI passes,
/// so the domain's unknown-id refusal is the same answer on both doors.
fn named(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// Today in the reader's local calendar, as a `YYYY-MM-DD` day key: the window
/// the Usage face draws is the days the human lived through, resolved through
/// the engine's own `localtime`. A calendar the engine cannot read falls back
/// to the UTC date, so the report still answers rather than refusing the read.
fn local_today() -> String {
    let now = crate::store::timestamp::now();
    crate::store::local_day(&now).unwrap_or_else(|_| now.get(0..10).unwrap_or(&now).to_string())
}

/// `session.transcript`: one record's harness transcript, or the reason it
/// cannot be read.
///
/// The id is the Tandem record's or the raw harness pointer: `Sessions::resolve`
/// answers the record either way, and an id neither names is the wire's
/// refusal, the same one `session show` makes. Everything past the resolve is a
/// reason beside empty entries, never a refusal: the section draws every row,
/// and a row the harness no longer holds says why. The bytes stay out of
/// `tandem.db`: the record's pointer is the join, and the read opens the
/// harness store read-only, the way the usage read does.
fn session_transcript(
    store: &Store,
    environment: &Environment,
    id: &str,
    cursor: Option<i64>,
    limit: Option<i64>,
) -> Result<Reply, WireError> {
    let sessions = Sessions::new(store);
    let record = sessions
        .resolve(id)
        .and_then(|resolved| sessions.show(&resolved))
        .map_err(domain)?;
    let unavailable = |reason: String| {
        Reply::SessionTranscript(SessionTranscriptReply {
            entries: Vec::new(),
            next_cursor: None,
            unavailable: Some(reason),
        })
    };
    let Some(pointer) = record.harness_session_id.clone() else {
        return Ok(unavailable(format!(
            "Session \"{}\" names no harness session, so there is no transcript to read.",
            record.id
        )));
    };
    let limit = limit.unwrap_or(50).clamp(1, 200) as usize;
    // A record from the embedded harness (`event-stream`) carries its rows in
    // the home's own `harness/opencode.db`; a CLI run (`database`) carries its
    // rows in the external store `OPENCODE_DB` names. A record with a pointer
    // and no known transport reads the external store: it is the only source
    // that needs no path the record does not carry.
    let page = match record
        .harness_transport
        .unwrap_or(HarnessTransport::Database)
    {
        HarnessTransport::Database => harness::transcript(
            &external_harness_database(environment),
            &pointer,
            cursor,
            limit,
        ),
        HarnessTransport::EventStream => harness::transcript(
            &store.home().directory().join("harness").join("opencode.db"),
            &pointer,
            cursor,
            limit,
        ),
    };
    match page {
        Ok(page) => Ok(Reply::SessionTranscript(SessionTranscriptReply {
            entries: page.entries,
            next_cursor: page.next_cursor,
            unavailable: None,
        })),
        Err(error) => Ok(unavailable(error.message())),
    }
}

/// The external opencode store: `OPENCODE_DB`, else the path its own default
/// names. Tandem reads it read-only and never writes it.
fn external_harness_database(environment: &Environment) -> PathBuf {
    if let Some(path) = environment
        .get("OPENCODE_DB")
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
    {
        return PathBuf::from(path);
    }
    let home = environment
        .get("HOME")
        .cloned()
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_default();
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}

/// A session record's handle: the caller's value, else `$OPENCODE_SESSION_ID`,
/// resolved to the record, as the CLI's verbs resolve it. An absent handle is
/// the CLI's own refusal, never a mint.
fn resolve_session(
    sessions: &Sessions,
    raw: Option<&str>,
    environment: &Environment,
) -> Result<String, WireError> {
    let handle = match raw {
        Some(raw) => raw.to_string(),
        None => match environment.get("OPENCODE_SESSION_ID") {
            Some(id) if !id.trim().is_empty() => id.clone(),
            _ => return Err(domain(DomainError::SessionIdUnset)),
        },
    };
    sessions.resolve(&handle).map_err(domain)
}

/// The harness session a record points at: the caller's value, else
/// `$OPENCODE_SESSION_ID`; no transport beside it is `database`, the external
/// opencode, as the verb defaults it.
fn session_harness(
    params: &crate::wire::rpc::SessionStartParams,
    environment: &Environment,
) -> Result<Option<HarnessSession>, WireError> {
    let id = match params.harness_session.as_deref() {
        Some(raw) => raw.trim().to_string(),
        None => environment
            .get("OPENCODE_SESSION_ID")
            .map(|id| id.trim().to_string())
            .unwrap_or_default(),
    };
    if id.is_empty() {
        if params.harness_session.is_some() {
            return Err(domain(DomainError::EmptyHarnessSessionId));
        }
        if params.harness_transport.is_some() {
            return Err(domain(DomainError::EmptyHarnessSessionId));
        }
        return Ok(None);
    }
    let transport = match params.harness_transport.as_deref() {
        Some(raw) => HarnessTransport::parse(raw.trim())
            .ok_or_else(|| domain(DomainError::InvalidHarnessTransport(raw.to_string())))?,
        None => HarnessTransport::Database,
    };
    Ok(Some(HarnessSession { id, transport }))
}

/// A session's agent: the caller's value, else `$TANDEM_AGENT`; nobody when the
/// environment names no agent. The core trims the name.
fn session_agent(raw: Option<&str>, environment: &Environment) -> Option<String> {
    raw.map(str::to_string).or_else(|| {
        environment
            .get(crate::store::ACTOR_ENVIRONMENT)
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

/// The token counts the end call reports; absent is absent on the record. A
/// negative value names its flag, as the verb's flags do.
fn end_tokens(
    params: &crate::wire::rpc::SessionEndParams,
) -> Result<Option<crate::domain::session::SessionTokens>, WireError> {
    fn count(flag: &str, value: Option<i64>) -> Result<Option<i64>, WireError> {
        match value {
            None => Ok(None),
            Some(value) if value >= 0 => Ok(Some(value)),
            Some(value) => Err(domain(DomainError::InvalidUsageFlag {
                flag: flag.to_string(),
                value: value.to_string(),
            })),
        }
    }
    let input = count("tokens-input", params.tokens_input)?;
    let output = count("tokens-output", params.tokens_output)?;
    let reasoning = count("tokens-reasoning", params.tokens_reasoning)?;
    let cache_read = count("tokens-cache-read", params.tokens_cache_read)?;
    let cache_write = count("tokens-cache-write", params.tokens_cache_write)?;
    if input.is_none()
        && output.is_none()
        && reasoning.is_none()
        && cache_read.is_none()
        && cache_write.is_none()
    {
        return Ok(None);
    }
    Ok(Some(crate::domain::session::SessionTokens {
        input,
        output,
        reasoning,
        cache_read,
        cache_write,
    }))
}

/// The cost the end call reports; a negative value names the flag.
fn end_cost(cost: Option<f64>) -> Result<Option<f64>, WireError> {
    match cost {
        None => Ok(None),
        Some(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
        Some(value) => Err(domain(DomainError::InvalidUsageFlag {
            flag: "cost".to_string(),
            value: value.to_string(),
        })),
    }
}

/// `--model`: absent is absent; an empty value is no model.
fn end_model(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// The usage a record's harness session reported, read over the harness
/// loopback API (TAN-276), as the CLI's `session end` reads it. The server is
/// the call's `harness_url`, else `TANDEM_HARNESS_URL`, else the rendezvous
/// file for the record's transport; an explicit URL always wins whatever the
/// transport. The password is `TANDEM_HARNESS_PASSWORD` else the rendezvous
/// file, never a flag.
fn harness_usage(
    store: &Store,
    record: &crate::domain::session::Session,
    url_flag: Option<&str>,
    environment: &Environment,
) -> Result<crate::domain::harness::HarnessUsage, DomainError> {
    use crate::domain::harness;
    let id = record
        .harness_session_id
        .clone()
        .ok_or_else(|| DomainError::NoHarnessSession {
            id: record.id.clone(),
        })?;
    let (base, password) = harness_server(store, record.harness_transport, url_flag, environment)?;
    harness::read_api(&base, &password, &id)
}

/// The loopback server and its credential: an explicit URL wins, else the
/// rendezvous file for the transport. The transport is provenance (which
/// harness the pass ran in), so it only selects which rendezvous file is read
/// when no URL is given.
fn harness_server(
    store: &Store,
    transport: Option<HarnessTransport>,
    url_flag: Option<&str>,
    environment: &Environment,
) -> Result<(String, String), DomainError> {
    let explicit = url_flag
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| {
            environment
                .get("TANDEM_HARNESS_URL")
                .map(|url| url.trim().to_string())
                .filter(|url| !url.is_empty())
        });
    let rendezvous = rendezvous_path(store, transport, environment);
    let password_from_file = || rendezvous_password(&rendezvous).unwrap_or_default();
    if let Some(base) = explicit {
        let password = environment
            .get("TANDEM_HARNESS_PASSWORD")
            .cloned()
            .unwrap_or_else(password_from_file);
        return Ok((base, password));
    }
    let (base, file_password) = rendezvous_server(&rendezvous)?;
    let password = environment
        .get("TANDEM_HARNESS_PASSWORD")
        .cloned()
        .unwrap_or(file_password);
    Ok((base, password))
}

/// Which rendezvous file names the server when no URL is given: the embedded
/// harness's `state/opencode/service.json` under the Tandem home for
/// `event-stream` records, else the external state dir the binary mirrors
/// (`XDG_STATE_HOME/opencode/service.json`, else
/// `~/.local/state/opencode/service.json`).
fn rendezvous_path(
    store: &Store,
    transport: Option<HarnessTransport>,
    environment: &Environment,
) -> PathBuf {
    if transport == Some(HarnessTransport::EventStream) {
        return store
            .home()
            .directory()
            .join("harness")
            .join("state")
            .join("opencode")
            .join("service.json");
    }
    if let Some(state) = environment
        .get("XDG_STATE_HOME")
        .map(|state| state.trim())
        .filter(|state| !state.is_empty())
    {
        return PathBuf::from(state).join("opencode").join("service.json");
    }
    let home_dir = environment
        .get("HOME")
        .map(|home| home.trim())
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_default();
    home_dir
        .join(".local")
        .join("state")
        .join("opencode")
        .join("service.json")
}

/// The server the rendezvous file names, with its password. Extra keys are
/// ignored; the file is service-written, mode 0600.
fn rendezvous_server(path: &Path) -> Result<(String, String), DomainError> {
    let text = std::fs::read_to_string(path).map_err(|_| DomainError::NoHarnessServer {
        url: path.display().to_string(),
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| DomainError::NoHarnessServer {
            url: path.display().to_string(),
        })?;
    let base = value
        .get("url")
        .and_then(|url| url.as_str())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainError::NoHarnessServer {
            url: path.display().to_string(),
        })?;
    let password = value
        .get("password")
        .and_then(|password| password.as_str())
        .unwrap_or_default()
        .to_string();
    Ok((base, password))
}

/// The password the rendezvous file holds, or empty when it holds none.
fn rendezvous_password(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(
        value
            .get("password")
            .and_then(|password| password.as_str())
            .unwrap_or_default()
            .to_string(),
    )
}

/// Whether a harness failure is an absence the stamp survives: a record with
/// no pointer, no server to read through, or a session the harness never
/// recorded. A wrong password is still a failure, because the numbers may
/// exist and be unreachable.
fn is_absent_harness(error: &DomainError) -> bool {
    matches!(
        error,
        DomainError::NoHarnessSession { .. }
            | DomainError::NoHarnessDatabase { .. }
            | DomainError::NoHarnessEvents { .. }
            | DomainError::NoHarnessServer { .. }
            | DomainError::UnknownHarnessSession { .. }
    )
}

/// The one `usage` comment a stamped session posts on every ticket it carried,
/// in the words the CLI posts: a header naming the session id and the model,
/// then one line per reported metric.
fn post_usage_comments(
    store: &Store,
    session: &crate::domain::session::Session,
    actor: &Actor,
) -> Result<(), DomainError> {
    if session.tickets.is_empty() {
        return Ok(());
    }
    let board = Board::with_actor(store.clone(), actor.clone());
    let author = session.agent.clone().unwrap_or_else(|| "usage".to_string());
    let body = crate::domain::session::usage_comment_body(session);
    for ticket in &session.tickets {
        board.append_comment(ticket, &author, &body)?;
    }
    Ok(())
}

/// The working directory's last component: the project a capture or session
/// defaults to.
fn project_default() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|directory| {
            directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

/// A capture's session default: `$OPENCODE_SESSION_ID`, else a short generated
/// token.
fn capture_session_default(environment: &Environment) -> String {
    environment
        .get("OPENCODE_SESSION_ID")
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()[..8].to_string())
}

/// A ticket receipt: the ticket a write answered and the revision that write
/// produced, read inside its transaction (`Store::last_revision`). A concurrent
/// writer cannot pair the ticket with a later number.
fn ticket_receipt(ticket: &crate::domain::Ticket, store: &Store) -> Reply {
    Reply::Ticket(TicketReply {
        revision: store.last_revision(),
        ticket: TicketDetail::new(ticket),
    })
}

/// A domain refusal, in the words the CLI prints on stderr.
fn domain(error: DomainError) -> WireError {
    WireError::domain(error.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_store_backs_off_to_a_cap() {
        let mut wait = WATCH_INTERVAL;
        for _ in 0..16 {
            wait = backoff(wait);
        }
        assert_eq!(wait, WATCH_BACKOFF_MAX);
        assert!(WATCH_BACKOFF_MAX > WATCH_INTERVAL);
    }

    /// A home whose store carries no revision row, which is what a pre-v4 file
    /// presents to `Store::watch`: `Ok(None)`, not an error. Unix only with the
    /// case that uses it.
    #[cfg(unix)]
    fn store_without_a_revision(name: &str) -> (std::path::PathBuf, Store) {
        use crate::store::{Home, NewSetting};

        let root =
            std::env::temp_dir().join(format!("tandem-rpc-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
        let store = Store::new(home);
        store
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: "k",
                    value: "v",
                })
            })
            .expect("the first write creates the schema");
        let connection = rusqlite::Connection::open(root.join("home").join("tandem.db"))
            .expect("the store opens");
        connection
            .execute_batch("DROP TABLE store_meta")
            .expect("the revision table is droppable");
        (root, store)
    }

    /// The stream the watch writes its events to; a store with no revision
    /// produces none. Unix only with the case that uses it.
    #[cfg(unix)]
    struct Sink;

    #[cfg(unix)]
    impl Stream for Sink {
        fn read_frame(&self) -> io::Result<Option<String>> {
            Ok(None)
        }

        fn write_frame(&self, _frame: &str) -> io::Result<()> {
            Ok(())
        }
    }

    // Unix only because the counter it reads is (`Store::watch_opens`), the
    // same gate the restored-store case carries.
    #[cfg(unix)]
    #[test]
    fn a_store_with_no_revision_backs_off_instead_of_reopening() {
        let (root, store) = store_without_a_revision("no-revision");
        let store = Arc::new(store);
        let stop = Arc::new(AtomicBool::new(false));
        let opens_before = store.watch_opens();
        let handle = {
            let store = Arc::clone(&store);
            let stop = Arc::clone(&stop);
            thread::spawn(move || watch(&Sink, &store, &stop))
        };
        thread::sleep(Duration::from_secs(1));
        stop.store(true, Ordering::Relaxed);
        handle.join().expect("the watch stops");
        let opens = store.watch_opens() - opens_before;
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            opens <= 10,
            "a pre-v4 store is opened {opens} times in a second, not at the 40 ms cadence"
        );
    }

    /// A home with one write, so the watch can attach and read a revision.
    fn store_with_a_revision(name: &str) -> (std::path::PathBuf, Store) {
        use crate::store::{Home, NewSetting};

        let root =
            std::env::temp_dir().join(format!("tandem-rpc-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
        let store = Store::new(home);
        store
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: "k",
                    value: "v",
                })
            })
            .expect("the first write creates the schema");
        (root, store)
    }

    /// The stream the latency case reads its events from.
    struct Recorder {
        frames: Mutex<Vec<String>>,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                frames: Mutex::new(Vec::new()),
            }
        }

        fn changed_revisions(&self) -> Vec<i64> {
            let frames = self
                .frames
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            frames
                .iter()
                .filter_map(|frame| serde_json::from_str::<serde_json::Value>(frame).ok())
                .filter(|value| {
                    value.get("event").and_then(|event| event.as_str()) == Some("changed")
                })
                .filter_map(|value| value.get("revision").and_then(|revision| revision.as_i64()))
                .collect()
        }
    }

    impl Stream for Recorder {
        fn read_frame(&self) -> io::Result<Option<String>> {
            Ok(None)
        }

        fn write_frame(&self, frame: &str) -> io::Result<()> {
            let mut frames = self
                .frames
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            frames.push(frame.to_string());
            Ok(())
        }
    }

    // An idle watch must not query the revision on every tick: the files do
    // not move, so the held connection stays quiet and the 40 ms cadence
    // costs two stats, not a `SELECT`.
    #[test]
    fn an_idle_watch_issues_no_revision_read() {
        let (root, store) = store_with_a_revision("idle");
        let store = Arc::new(store);
        let stop = Arc::new(AtomicBool::new(false));
        let reads_before = store.watch_revision_reads();
        let recorder = Arc::new(Recorder::new());
        let handle = {
            let store = Arc::clone(&store);
            let stop = Arc::clone(&stop);
            let recorder = Arc::clone(&recorder);
            thread::spawn(move || watch(&*recorder, &store, &stop))
        };
        thread::sleep(Duration::from_millis(600));
        stop.store(true, Ordering::Relaxed);
        handle.join().expect("the watch stops");
        let reads = store.watch_revision_reads() - reads_before;
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            reads <= 2,
            "an idle watch read the revision {reads} times in 600 ms, not once at attach"
        );
        assert!(
            recorder.changed_revisions().is_empty(),
            "an idle watch emits no change event"
        );
    }

    // A write from another store (the second terminal's door) arrives on the
    // stream quickly: the 40 ms ceiling is the bound, the write waits for the
    // attach so it cannot land before the baseline, and the case retries up
    // to two seconds so scheduling jitter cannot flake it.
    #[test]
    fn a_write_from_another_store_reports_changed_quickly() {
        use crate::store::NewSetting;

        let (root, store) = store_with_a_revision("latency");
        let store = Arc::new(store);
        let stop = Arc::new(AtomicBool::new(false));
        let recorder = Arc::new(Recorder::new());
        let reads_before = store.watch_revision_reads();
        let handle = {
            let store = Arc::clone(&store);
            let stop = Arc::clone(&stop);
            let recorder = Arc::clone(&recorder);
            thread::spawn(move || watch(&*recorder, &store, &stop))
        };
        // The write must land after the watch's baseline: a write that lands
        // before it leaves `last` already at the new revision and no event
        // ever fires. The attach's first revision read proves the thread is
        // up; one more tick settles the baseline stat, so a late start on a
        // loaded runner cannot flake the case the way a fixed sleep can.
        let attached = std::time::Instant::now() + Duration::from_secs(10);
        while store.watch_revision_reads() == reads_before && std::time::Instant::now() < attached {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            store.watch_revision_reads() > reads_before,
            "the watch never attached"
        );
        thread::sleep(WATCH_INTERVAL * 2);
        let before = store.revision().expect("the revision reads");
        let files_before = store.watch_files();
        let started = std::time::Instant::now();
        store
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: "k2",
                    value: "v2",
                })
            })
            .expect("the other writer writes");
        let moved = store.revision().expect("the revision moved");
        assert!(moved > before, "the write moved the revision");
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            seen = recorder.changed_revisions();
            if seen.contains(&moved) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let elapsed = started.elapsed();
        stop.store(true, Ordering::Relaxed);
        handle.join().expect("the watch stops");
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            seen.contains(&moved),
            "a write to {moved} never arrived within 2 s (saw {seen:?}): files {files_before:?} -> {:?}, revision {:?}, reads {}",
            store.watch_files(),
            store.revision().ok(),
            store.watch_revision_reads(),
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "a write took {elapsed:?} to arrive, past the 40 ms ceiling plus jitter"
        );
    }
}
