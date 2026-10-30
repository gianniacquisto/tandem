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
//! - **The watch**: a thread polls the store's revision on one read-only
//!   connection it keeps open, and pushes [`Event::Changed`] when any writer
//!   moves it, this process's own writes included. Polling the revision (rather
//!   than watching a file) is what makes "another process under the lock"
//!   observable on every platform; reusing the connection is what keeps an idle
//!   child from opening one a tick. An event is a hint to re-read: it may name
//!   an intermediate revision (a migration is a commit of its own) and the
//!   listing's revision is the authoritative one.
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

use crate::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, Status, TicketQuery, resolve_assignee,
    resolve_origin,
};
use crate::store::{Actor, Store};
use crate::wire::rpc::{
    ArchivedReply, CommentListReply, CommentReply, Event, LaneDeletionReply, LaneListReply, Method,
    Outcome, Reply, Request, Response, TicketListReply, TicketReply, WireError,
};
use crate::wire::{CommentView, TicketDetail, TicketSummary};

/// The most one frame may carry. A description is text a human wrote; a
/// megabyte is far past any of them, and the cap keeps an untrusted writer from
/// growing this process's heap one line at a time. A frame of exactly this many
/// bytes is legal; the newline that ends it is the stream's, not the frame's.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

/// How often the watch asks the store whether the revision moved, on the one
/// connection it keeps open. Short enough that the face updates before a human
/// notices, and one query a tick is what an idle child costs.
const WATCH_INTERVAL: Duration = Duration::from_millis(40);

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

/// The watch: poll the store's revision on one read-only connection and push a
/// change event whenever it moves, until `stop`.
fn watch<S: Stream>(stream: &S, store: &Store, stop: &AtomicBool) {
    // One read-only connection, reused: an idle child runs one query per tick,
    // not an open and a close. The watch waits for the store to exist and for
    // its one-time WAL switch (`Store::watch`).
    let mut watch = store.watch().ok().flatten();
    let mut last = watch
        .as_ref()
        .and_then(|watch| watch.revision().ok())
        .unwrap_or(0);
    let mut wait = WATCH_INTERVAL;
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(wait);
        if watch.is_none() {
            match store.watch() {
                Ok(Some(found)) => {
                    watch = Some(found);
                    wait = WATCH_INTERVAL;
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
        let Ok(revision) = current.revision() else {
            // The connection is unusable (the store was replaced or
            // removed): drop it and reopen with the backoff.
            watch = None;
            wait = backoff(wait);
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
                None => Status::Backlog,
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
        Method::TicketAnswer(params) => {
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let ticket = board
                .answer(
                    named(&params.id),
                    Status::parse(params.status.as_deref().unwrap_or("")).map_err(domain)?,
                    assignee.as_deref(),
                    params.body.as_deref().unwrap_or(""),
                    crate::store::timestamp::unix_now(),
                )
                .map_err(domain)?;
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
        Method::LaneList(_) => {
            let (lanes, revision) = board.lanes_snapshot().map_err(domain)?;
            Ok(Reply::LaneList(LaneListReply { revision, lanes }))
        }
        Method::LaneAdd(params) => {
            let lanes = board
                .add_lane(params.name.as_deref().unwrap_or(""), params.abbr.as_deref())
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
    }
}

/// A handle the caller named; an absent one is the empty string the CLI passes,
/// so the domain's unknown-id refusal is the same answer on both doors.
fn named(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
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
    /// presents to `Store::watch`: `Ok(None)`, not an error.
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
    /// produces none.
    struct Sink;

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
}
