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
//! - **The watch**: a thread polls the store's revision and pushes
//!   [`Event::Changed`] when any writer moves it, this process's own writes
//!   included. Polling the revision (rather than watching a file) is what makes
//!   "another process under the lock" observable on every platform. An event is
//!   a hint to re-read: it may name an intermediate revision (a migration is a
//!   commit of its own) and the listing's revision is the authoritative one.
//!
//! The lifecycle is stdin's: when the parent closes the pipe, the read returns
//! end of file, the loop ends, the watch stops and the process exits. The app
//! spawns the child and dies with it; nothing outlives the pipe, and no lock is
//! held between requests, because every request opens the store for its own
//! transaction.

use std::io::{self, BufRead, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::domain::{AddTicket, Board, DomainError, Edit, Status, TicketQuery};
use crate::store::{Actor, Store};
use crate::wire::rpc::{
    ArchivedReply, CommentListReply, CommentReply, Event, LaneDeletionReply, LaneListReply, Method,
    Outcome, Reply, Request, Response, TicketListReply, TicketReply, WireError,
};
use crate::wire::{CommentView, TicketDetail, TicketSummary};

/// The most one frame may carry. A description is text a human wrote; a
/// megabyte is far past any of them, and the cap keeps an untrusted writer from
/// growing this process's heap one line at a time.
const MAX_FRAME: usize = 4 * 1024 * 1024;

/// How often the watch asks the store whether the revision moved. Short enough
/// that the face updates before a human notices, long enough that an idle child
/// is idle.
const WATCH_INTERVAL: Duration = Duration::from_millis(40);

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
            let mut limited = (&mut input).take(MAX_FRAME as u64 + 1);
            limited.read_until(b'\n', &mut buffer)?;
        }
        if buffer.len() > MAX_FRAME {
            // Drain the rest of the line so the next read starts at a frame
            // boundary, then refuse this one. The stream stays usable.
            let mut discard = Vec::new();
            loop {
                discard.clear();
                if input.read_until(b'\n', &mut discard)? == 0 || discard.ends_with(b"\n") {
                    break;
                }
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the frame is longer than {MAX_FRAME} bytes"),
            ));
        }
        if buffer.is_empty() {
            return Ok(None);
        }
        if buffer.ends_with(b"\n") {
            buffer.pop();
            if buffer.ends_with(b"\r") {
                buffer.pop();
            }
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
        let mut last = store.revision().unwrap_or(0);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(WATCH_INTERVAL);
                let Ok(revision) = store.revision() else {
                    continue;
                };
                if revision == last {
                    continue;
                }
                last = revision;
                let frame = match serde_json::to_string(&Event::changed(revision)) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                if stream.write_frame(&frame).is_err() {
                    return;
                }
            }
        })
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

// MARK: - One frame

/// Answer one frame: a request becomes a result or an error, and a frame that
/// is not a request becomes a parse error.
///
/// This is the whole protocol minus the transport, so the suite can drive it
/// without a process and the pipe test can drive it through one.
pub fn handle(frame: &str, store: &Store, actor: &Actor) -> String {
    let response = match serde_json::from_str::<Request>(frame) {
        Ok(request) => {
            let id = request.id;
            match execute(request.method, store, actor) {
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
fn execute(method: Method, store: &Store, actor: &Actor) -> Result<Reply, WireError> {
    let board = Board::with_actor(store.clone(), actor.clone());
    match method {
        Method::TicketList(params) => {
            let query = TicketQuery::parse(
                params.filter.as_deref(),
                params.text.as_deref(),
                params.status.as_deref(),
                params.assignee.as_deref(),
                params.lane.as_deref(),
                params.archived,
                params.blocked,
            )
            .map_err(domain)?;
            let (tickets, revision) = snapshot(&board, &query, store)?;
            Ok(Reply::TicketList(TicketListReply {
                revision,
                tickets: tickets.iter().map(TicketSummary::new).collect(),
            }))
        }
        Method::TicketShow(params) => {
            let ticket = board.ticket(named(&params.id)).map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketAdd(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(domain)?,
                None => Status::Backlog,
            };
            let ticket = board
                .add(
                    params.title.as_deref().unwrap_or(""),
                    AddTicket {
                        lane: params.lane.as_deref(),
                        status,
                        assignee: params.assignee.as_deref(),
                        description: params.description.as_deref(),
                        origin: params.origin.as_deref().unwrap_or("human"),
                    },
                )
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketSet(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Edit::Set(Status::parse(raw).map_err(domain)?),
                None => Edit::Unchanged,
            };
            let assignee = match params.assignee.as_deref() {
                Some(raw) => Edit::Set(Some(raw)),
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
                        archived: Edit::Unchanged,
                    },
                )
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketMove(params) => {
            let at = match params.index {
                None => usize::MAX,
                Some(index) => index.max(0) as usize,
            };
            let ticket = board
                .move_ticket(named(&params.id), params.lane.as_deref(), at)
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketBlock(params) => {
            let ticket = board
                .block(named(&params.id), params.on.as_deref().unwrap_or(""))
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketUnblock(params) => {
            let ticket = board
                .unblock(named(&params.id), params.on.as_deref())
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketComplete(params) => {
            let ticket = board.complete(named(&params.id)).map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketAnswer(params) => {
            let ticket = board
                .answer(
                    named(&params.id),
                    Status::parse(params.status.as_deref().unwrap_or("")).map_err(domain)?,
                    params.assignee.as_deref(),
                    params.body.as_deref().unwrap_or(""),
                    crate::store::timestamp::unix_now(),
                )
                .map_err(domain)?;
            ticket_reply(&ticket, store)
        }
        Method::TicketArchive(params) => match params.id.as_deref() {
            Some(id) => {
                let ticket = board.archive(id).map_err(domain)?;
                ticket_reply(&ticket, store)
            }
            None => {
                let archived = board.archive_done().map_err(domain)?;
                Ok(Reply::Archived(ArchivedReply {
                    revision: revision(store)?,
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
                revision: revision(store)?,
                comment: CommentView::new(&comment),
            }))
        }
        Method::TicketCommentList(params) => {
            let comments = board.comments(named(&params.id)).map_err(domain)?;
            Ok(Reply::CommentList(CommentListReply {
                revision: revision(store)?,
                comments: comments.iter().map(CommentView::new).collect(),
            }))
        }
        Method::LaneList(_) => Ok(Reply::LaneList(LaneListReply {
            revision: revision(store)?,
            lanes: board.lanes().map_err(domain)?,
        })),
        Method::LaneAdd(params) => {
            let lanes = board
                .add_lane(params.name.as_deref().unwrap_or(""), params.abbr.as_deref())
                .map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply {
                revision: revision(store)?,
                lanes,
            }))
        }
        Method::LaneRename(params) => {
            let lanes = board
                .rename_lane(
                    params.from.as_deref().unwrap_or(""),
                    params.to.as_deref().unwrap_or(""),
                    params.abbr.as_deref(),
                )
                .map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply {
                revision: revision(store)?,
                lanes,
            }))
        }
        Method::LaneDelete(params) => {
            let deletion = board
                .delete_lane(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::LaneDeletion(LaneDeletionReply {
                revision: revision(store)?,
                lanes: deletion.lanes,
                moved: deletion.moved,
            }))
        }
    }
}

/// A handle the caller named; an absent one is the empty string the CLI passes,
/// so the domain's unknown-id refusal is the same answer on both doors.
fn named(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// The revision a reply carries, in the store's words when it fails.
fn revision(store: &Store) -> Result<i64, WireError> {
    store
        .revision()
        .map_err(|error| WireError::store(error.to_string()))
}

/// A ticket receipt: the ticket the write answered and the revision after it.
fn ticket_reply(ticket: &crate::domain::Ticket, store: &Store) -> Result<Reply, WireError> {
    Ok(Reply::Ticket(TicketReply {
        revision: revision(store)?,
        ticket: TicketDetail::new(ticket),
    }))
}

/// A listing that is a snapshot of one revision: read the revision, read the
/// board, read the revision again, and accept the pair only when the store did
/// not move in between. A writer that keeps winning is answered with the last
/// read, whose revision the client will also see in the change event.
fn snapshot(
    board: &Board,
    query: &TicketQuery,
    store: &Store,
) -> Result<(Vec<crate::domain::Ticket>, i64), WireError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let before = revision(store)?;
        let tickets = board.list(query).map_err(domain)?;
        let after = revision(store)?;
        if before == after || attempt >= 5 {
            return Ok((tickets, after));
        }
    }
}

/// A domain refusal, in the words the CLI prints on stderr.
fn domain(error: DomainError) -> WireError {
    WireError::domain(error.message())
}
