//! `tandem serve`: the one binary's loopback HTTP face (ADR 0046).
//!
//! The board is rendered on the server: askama compiles the templates into the
//! core, htmx swaps the `#board-root` partial, and a vendored `tandem.js`
//! carries the one act htmx cannot, a card drop. Each request resolves one
//! [`Context`] through the router's state, so every route reads and writes
//! through the same store and actor the CLI uses, and a face that serves more
//! than one caller resolves the caller there instead. Every mutating request
//! crosses one write guard layer, so a route cannot opt out by forgetting it.
//! A [`watch`] thread forwards every store revision to the `GET /events`
//! stream, and the shell re-fetches the board on each one.
//!
//! Two seams hold the contract: [`router`] is the in-process HTTP seam the unit
//! tests drive, and the `serve` verb in `rust/cli` is the process seam.

use std::collections::HashMap;
use std::convert::Infallible;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use askama::Template;
use axum::extract::rejection::FormRejection;
use axum::extract::{DefaultBodyLimit, Form, FromRequestParts, Path, Query, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::domain::markdown::{MarkdownBlock, TicketMarkdown};
use crate::domain::text::TextMatch;
use crate::domain::{
    AddTicket, AssigneeAxis, Board, Edit, LaneAxis, SetFields, Status, Ticket, TicketQuery,
};
use crate::env::Environment;
use crate::store::{Actor, RevisionWatch, Store};

mod board;
mod events;
mod markdown;
mod notes;
mod sheet;

/// How often the watch stats the store's files. Short enough that a face
/// updates before a human notices, and cheap because a tick stats twice and
/// queries nothing unless a file moved.
pub const WATCH_INTERVAL: Duration = Duration::from_millis(40);

/// How often the watch re-reads the revision even when the files look still: a
/// filesystem that reports no mtime or size change must not silence the stream.
pub const WATCH_FALLBACK: Duration = Duration::from_secs(5);

/// The longest wait after a store the watch cannot attach to: refused,
/// unreadable, or readable with no revision row yet.
pub const WATCH_BACKOFF_MAX: Duration = Duration::from_secs(5);

/// One step of the watch's backoff: twice the wait, capped.
fn backoff(wait: Duration) -> Duration {
    (wait * 2).min(WATCH_BACKOFF_MAX)
}

/// The one policy every HTML response carries: the board is its own scripts and
/// styles, the change stream is same-origin, and nothing may frame it.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
     connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; \
     frame-src 'none'; frame-ancestors 'self'";

/// The notice a stale revision draws: the write was not applied, so the human
/// reviews the board that moved instead of overwriting it.
const CONFLICT_NOTICE: &str =
    "The board changed while you were writing. Review the new state and try again.";

/// One serve run's context: the store every route reads, the actor every write
/// records, and the environment the dispatch reads. Injected as the router's
/// default state, so a test names a throwaway home without a process.
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub store: Store,
    pub actor: Actor,
    pub environment: Environment,
}

impl SessionContext {
    fn board(&self) -> Board {
        Board::with_actor(self.store.clone(), self.actor.clone())
    }

    fn revision(&self) -> i64 {
        self.store.revision().unwrap_or(0)
    }

    /// The vault beside the store's home: one rule, the same one `diagnostics`
    /// and the CLI use, so the server reads the bikeshed the human edits.
    fn bikeshed(&self) -> Bikeshed {
        Bikeshed::new(self.store.home().directory().join("bikeshed"))
    }
}

/// How the router resolves one request's context.
///
/// Local mode is the process's one [`SessionContext`], so resolution is the
/// clone. A face that serves more than one caller resolves the caller and the
/// workspace it may read here, from the request parts, without a handler
/// changing: this trait is the one door the context comes through.
// The rejection is the response the request gets, the shape every axum
// extractor answers a refusal in; a box would only move the error a layer out.
#[allow(clippy::result_large_err)]
pub trait ResolveContext {
    fn resolve(&self, parts: &Parts) -> Result<Context, Response>;
}

#[allow(clippy::result_large_err)]
impl ResolveContext for Arc<SessionContext> {
    fn resolve(&self, _parts: &Parts) -> Result<Context, Response> {
        Ok(Context(Arc::clone(self)))
    }
}

/// One request's context. Every handler takes this, never the router's state,
/// so the context is resolved per request and nothing depends on a
/// process-wide one.
#[derive(Debug, Clone)]
pub struct Context(Arc<SessionContext>);

impl Deref for Context {
    type Target = SessionContext;

    fn deref(&self) -> &SessionContext {
        &self.0
    }
}

impl<S> FromRequestParts<S> for Context
where
    S: ResolveContext + Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        state.resolve(parts)
    }
}

/// The in-process HTTP seam: the board routes over `context`.
pub fn router(context: SessionContext) -> Router {
    let (changes, _) = broadcast::channel(64);
    router_with(context, changes)
}

/// The router over an existing change channel, so the `serve` verb runs the
/// watcher on the same broadcast every connected stream listens to.
pub fn router_with(context: SessionContext, changes: broadcast::Sender<i64>) -> Router {
    layered(routes(), changes).with_state(Arc::new(context))
}

/// Every route the loopback face serves, before its layers and its state. One
/// table: the layers below cover everything added to it, so a route added here
/// is guarded by construction and cannot be served without the write guard.
fn routes() -> Router<Arc<SessionContext>> {
    Router::new()
        .route("/", get(board::board_page))
        .route("/board", get(board::board_page))
        .route("/events", get(events::events))
        .route("/static/{*path}", get(events::static_asset))
        .route("/tickets", post(board::add_ticket))
        .route(
            "/tickets/{id}",
            get(sheet::ticket_page).post(sheet::save_ticket),
        )
        .route("/tickets/{id}/status", post(board::move_ticket))
        .route("/tickets/{id}/comments", post(sheet::add_comment))
        .route("/tickets/{id}/lane", post(sheet::move_to_lane))
        .route("/lanes", post(board::add_lane))
        .route("/lanes/{name}/rename", post(board::rename_lane))
        .route("/notes", get(notes::notes_page))
        .route("/notes/view", get(notes::note_view))
        .route("/notes/edit", get(notes::note_edit))
        .route("/notes/preview", post(notes::note_preview))
        .route("/notes/save", post(notes::note_save))
        .route("/notes/create", post(notes::note_create))
        .route("/notes/create-folder", post(notes::note_create_folder))
        .route("/notes/move", post(notes::note_move))
}

/// The layers every route carries: the write guard, the body ceiling and the
/// change channel the events stream reads.
fn layered(
    router: Router<Arc<SessionContext>>,
    changes: broadcast::Sender<i64>,
) -> Router<Arc<SessionContext>> {
    router
        .layer(middleware::from_fn(write_guard))
        .layer(DefaultBodyLimit::max(crate::rpc::MAX_FRAME))
        .layer(Extension(changes))
}

/// The write guard as a layer: every request that is not a read crosses it
/// before a handler runs, so a route cannot opt out by forgetting the call.
async fn write_guard(request: Request, next: Next) -> Response {
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && let Some(denied) = guard(request.headers())
    {
        return denied;
    }
    next.run(request).await
}

/// The store watch: one read-only connection, a 40 ms stat and a 5 s fallback,
/// and a broadcast per revision that moves.
pub fn watch(
    store: Store,
    changes: broadcast::Sender<i64>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    // One read-only connection, reused: an idle server stats twice a tick and
    // queries nothing, not an open and a close. The watch waits for the store
    // to exist and for its one-time WAL switch (`Store::watch`).
    thread::spawn(move || {
        let mut watch: Option<RevisionWatch> = store.watch().ok().flatten();
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
                    // No store yet: keep the normal cadence. A store that
                    // exists but carries no revision row is a pre-v4 file,
                    // opened on every attempt, so it backs off like a refusal
                    // until the first write migrates it.
                    Ok(None) => {
                        wait = if store.exists() {
                            backoff(wait)
                        } else {
                            WATCH_INTERVAL
                        };
                        continue;
                    }
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
                // A tick can land mid-commit: the WAL grows before COMMIT
                // moves the revision, so a file move with no revision move is
                // a write in flight, not noise. Keep the old baseline so the
                // next tick retries once the commit has landed.
                continue;
            }
            files = now_files;
            last = revision;
            // No receiver is not an error: the next reader subscribes afresh
            // and the SSE route sends it the current revision on connect.
            let _ = changes.send(revision);
        }
    })
}
// MARK: - The board's filters

/// One board request's filters, parsed from a GET query or a POST form. The
/// raw strings are kept for the hidden inputs a write form carries back.
struct Filters {
    project: String,
    show_all: bool,
    /// `""` is any lane, `" "` the unnamed default area, anything else a name.
    lane: String,
    /// `""` is any assignee, `" "` the unassigned, anything else a name.
    assignee: String,
    text: String,
    selected: Option<String>,
}

impl Filters {
    fn parse(params: &HashMap<String, String>) -> Self {
        let project = params
            .get("project")
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        let show_all = params
            .get("show_all")
            .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false"));
        let lane = params.get("lane").cloned().unwrap_or_default();
        let assignee = params.get("assignee").cloned().unwrap_or_default();
        let text = params.get("text").cloned().unwrap_or_default();
        let selected = params
            .get("selected")
            .filter(|value| !value.is_empty())
            .cloned();
        Self {
            project,
            show_all,
            lane,
            assignee,
            text,
            selected,
        }
    }

    /// The board filters a sheet's write form carries back. They are named with
    /// a `board_` prefix because the same form carries the ticket's own fields:
    /// its `assignee` and `lane` mean the ticket's, and reading them as the
    /// board's would redraw the board behind narrowed to this very ticket.
    fn parse_board_fields(params: &HashMap<String, String>) -> Self {
        let board = params
            .iter()
            .filter_map(|(key, value)| {
                key.strip_prefix("board_")
                    .map(|key| (key.to_string(), value.clone()))
            })
            .collect();
        Self::parse(&board)
    }

    fn lane_axis(&self) -> LaneAxis {
        match self.lane.as_str() {
            "" => LaneAxis::Any,
            " " => LaneAxis::Unnamed,
            name => LaneAxis::Named(name.to_string()),
        }
    }

    fn assignee_axis(&self) -> AssigneeAxis {
        match self.assignee.as_str() {
            "" => AssigneeAxis::Any,
            " " => AssigneeAxis::Unassigned,
            name => AssigneeAxis::Named(name.to_string()),
        }
    }

    /// The add form files into the lane filter's lane, or the default area when
    /// the filter is off (the React board's rule).
    fn add_lane(&self) -> Option<String> {
        match self.lane_axis() {
            LaneAxis::Named(name) => Some(name),
            _ => None,
        }
    }

    /// The filters as a query string, so a link that opens a sheet or lands
    /// back on the board carries them.
    fn query(&self) -> String {
        let mut pairs: Vec<(&str, String)> = vec![
            ("project", self.project.clone()),
            (
                "show_all",
                if self.show_all {
                    "1".into()
                } else {
                    String::new()
                },
            ),
            ("lane", self.lane.clone()),
            ("assignee", self.assignee.clone()),
            ("text", self.text.clone()),
        ];
        if let Some(selected) = &self.selected {
            pairs.push(("selected", selected.clone()));
        }
        serde_urlencoded::to_string(&pairs).unwrap_or_default()
    }

    /// The URL this view is fetched from, so a conflict's dismiss and the SSE
    /// re-fetch keep the filters the human set.
    fn url(&self) -> String {
        format!("/board?{}", self.query())
    }
}

impl Default for Filters {
    fn default() -> Self {
        Self::parse(&HashMap::new())
    }
}

fn field<'a>(params: &'a HashMap<String, String>, key: &str) -> &'a str {
    params.get(key).map(String::as_str).unwrap_or("")
}
#[derive(Template)]
#[template(path = "shell.html")]
struct PageView {
    /// The face template's own output, already HTML: the shell embeds it
    /// rather than escaping it a second time.
    body: askama::filters::Safe<String>,
    /// Whether the page opens the change stream. A page that draws the ticket
    /// sheet does not: the stream pauses until the sheet closes, so a store
    /// change cannot re-draw the form under the human's cursor.
    stream: bool,
}
fn is_partial(headers: &HeaderMap) -> bool {
    headers.get("hx-request").is_some()
}

fn html_response(html: String, status: StatusCode) -> Response {
    let mut response = (status, Html(html)).into_response();
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    response
}
/// The write guard's policy: the form content type and a same-origin `Origin`
/// when one is sent. [`write_guard`] applies it to every request that is not a
/// read. A browser sends `Origin` on every cross-origin POST, so the check keeps
/// an untrusted page from writing to loopback.
fn guard(headers: &HeaderMap) -> Option<Response> {
    let form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"));
    if !form {
        return Some(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "the request must be application/x-www-form-urlencoded\n",
            )
                .into_response(),
        );
    }
    if cross_origin(headers) {
        return Some(
            (
                StatusCode::FORBIDDEN,
                "the request came from another origin\n",
            )
                .into_response(),
        );
    }
    None
}
fn cross_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if origin == "null" {
        return true;
    }
    let authority = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    !authority.eq_ignore_ascii_case(host)
}
fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, format!("{message}\n")).into_response()
}
/// A parsed `expected_revision`: the revision the form was drawn from. A
/// missing or unreadable one is a refusal, never a silent overwrite.
fn expected_revision(params: &HashMap<String, String>) -> Result<i64, String> {
    field(params, "expected_revision")
        .trim()
        .parse::<i64>()
        .map_err(|_| "expected_revision is missing or not a number".to_string())
}
fn form_params(
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Option<HashMap<String, String>> {
    match form {
        Ok(Form(params)) => Some(params),
        Err(_) => None,
    }
}
#[cfg(test)]
mod tests;
