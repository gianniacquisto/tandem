//! `tandem serve`: the one binary's loopback HTTP face (ADR 0046).
//!
//! The board is rendered on the server: askama compiles the templates into the
//! core, htmx swaps the `#board-root` partial, and a vendored `tandem.js`
//! carries the one act htmx cannot, a card drop. One [`SessionContext`] is the
//! router's state, so every route reads and writes through the same store and
//! actor the CLI uses. A [`watch`] thread forwards every store revision to the
//! `GET /events` stream, and the shell re-fetches the board on each one.
//!
//! Two seams hold the contract: [`router`] is the in-process HTTP seam the unit
//! tests drive, and the `serve` verb in `rust/cli` is the process seam.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use askama::Template;
use axum::extract::rejection::FormRejection;
use axum::extract::{DefaultBodyLimit, Form, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::domain::text::TextMatch;
use crate::domain::{
    AddTicket, AssigneeAxis, Board, Edit, LaneAxis, SetFields, Status, Ticket, TicketQuery,
};
use crate::env::Environment;
use crate::store::{Actor, RevisionWatch, Store};

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
}

/// The in-process HTTP seam: the board routes over `context`.
pub fn router(context: SessionContext) -> Router {
    let (changes, _) = broadcast::channel(64);
    router_with(context, changes)
}

/// The router over an existing change channel, so the `serve` verb runs the
/// watcher on the same broadcast every connected stream listens to.
pub fn router_with(context: SessionContext, changes: broadcast::Sender<i64>) -> Router {
    Router::new()
        .route("/", get(board_page))
        .route("/board", get(board_page))
        .route("/events", get(events))
        .route("/static/{*path}", get(static_asset))
        .route("/tickets", post(add_ticket))
        .route("/tickets/{id}/status", post(move_ticket))
        .layer(DefaultBodyLimit::max(crate::rpc::MAX_FRAME))
        .layer(Extension(changes))
        .with_state(Arc::new(context))
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

// MARK: - Board reads

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

    /// The URL this view is fetched from, so a conflict's dismiss and the SSE
    /// re-fetch keep the filters the human set.
    fn url(&self) -> String {
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
        match serde_urlencoded::to_string(&pairs) {
            Ok(query) => format!("/board?{query}"),
            Err(_) => "/board".to_string(),
        }
    }
}

fn field<'a>(params: &'a HashMap<String, String>, key: &str) -> &'a str {
    params.get(key).map(String::as_str).unwrap_or("")
}

/// The board's visible set: every non-archived ticket, narrowed by the project
/// focus, the lane, the assignee and the title search.
fn visible<'a>(
    tickets: &'a [Ticket],
    projects: &[crate::domain::ProjectWithLanes],
    filters: &Filters,
) -> Option<Vec<&'a Ticket>> {
    let focus: Option<Vec<String>> = if filters.show_all || filters.project.is_empty() {
        None
    } else {
        projects
            .iter()
            .find(|project| project.name == filters.project)
            .map(|project| project.lanes.clone())
    };
    let lane = filters.lane_axis();
    let assignee = filters.assignee_axis();
    let needle = filters.text.trim();
    Some(
        tickets
            .iter()
            .filter(|ticket| {
                if ticket.archived {
                    return false;
                }
                if let Some(lanes) = &focus
                    && !ticket
                        .lane
                        .as_ref()
                        .is_some_and(|lane| lanes.contains(lane))
                {
                    return false;
                }
                match &lane {
                    LaneAxis::Any => {}
                    LaneAxis::Unnamed => {
                        if ticket.lane.is_some() {
                            return false;
                        }
                    }
                    LaneAxis::Named(name) => {
                        if ticket.lane.as_deref() != Some(name.as_str()) {
                            return false;
                        }
                    }
                }
                match &assignee {
                    AssigneeAxis::Any => {}
                    AssigneeAxis::Unassigned => {
                        if ticket.assignee.is_some() {
                            return false;
                        }
                    }
                    AssigneeAxis::Named(name) => {
                        if ticket.assignee.as_deref() != Some(name.as_str()) {
                            return false;
                        }
                    }
                }
                needle.is_empty() || TextMatch::contains(needle, &ticket.title)
            })
            .collect(),
    )
}

// MARK: - The templates

#[derive(Template)]
#[template(path = "shell.html")]
struct PageView {
    /// The board template's own output, already HTML: the shell embeds it
    /// rather than escaping it a second time.
    board: askama::filters::Safe<String>,
}

#[derive(Template)]
#[template(path = "board.html")]
struct BoardView {
    revision: i64,
    self_url: String,
    dismiss_url: String,
    notice: Option<String>,
    form: FiltersView,
    project_options: Vec<ProjectOption>,
    lane_options: Vec<LaneOption>,
    assignee_options: Vec<AssigneeOption>,
    show_all: bool,
    empty: Option<EmptyView>,
    columns: Vec<ColumnView>,
}

struct FiltersView {
    project: String,
    show_all: bool,
    lane: String,
    assignee: String,
    text: String,
}

struct ProjectOption {
    value: String,
    label: String,
    selected: bool,
}

struct LaneOption {
    value: String,
    label: String,
    selected: bool,
}

struct AssigneeOption {
    value: String,
    label: String,
    selected: bool,
}

struct EmptyView {
    title: String,
    subtitle: String,
    hero: bool,
}

struct ColumnView {
    status: &'static str,
    label: &'static str,
    count: usize,
    cards: Vec<CardView>,
}

struct CardView {
    id: String,
    title: String,
    reference: Option<String>,
    lane: Option<LaneView>,
    assignee: Option<AssigneeView>,
    blocked: Option<String>,
    selected: bool,
}

struct LaneView {
    name: String,
    tone: &'static str,
}

struct AssigneeView {
    label: String,
    human: bool,
}

/// The palette tokens a lane chip may draw, in the renderer's fixed order.
const LANE_TONES: [&str; 7] = ["moss", "ember", "accent", "sun", "skin", "hill", "sky"];

/// The tone a lane's chip draws: the lane name hashed over the palette tokens,
/// exactly as `app/renderer/src/shell/board.ts` does, so a lane keeps its colour
/// and no colour is stored.
fn lane_tone(name: &str) -> &'static str {
    let mut hash: u32 = 0;
    for character in name.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(character as u32);
    }
    LANE_TONES[(hash % LANE_TONES.len() as u32) as usize]
}

/// The block's one-line mark: `Blocked, TAN-42` when exactly one open blocker
/// has a ref to name, `Blocked` when it has none or there are several.
fn block_label(ticket: &Ticket) -> Option<String> {
    if !ticket.blocked() {
        return None;
    }
    let open: Vec<_> = ticket
        .blocked_by
        .iter()
        .filter(|block| block.status != Status::Done)
        .collect();
    if open.len() != 1 {
        return Some("Blocked".to_string());
    }
    match open[0].reference.as_deref() {
        Some(reference) => Some(format!("Blocked, {reference}")),
        None => Some("Blocked".to_string()),
    }
}

fn assignee_view(assignee: Option<&str>) -> Option<AssigneeView> {
    assignee.map(|name| AssigneeView {
        label: if name == "human" {
            "human".to_string()
        } else {
            format!("@{name}")
        },
        human: name == "human",
    })
}

/// The one message the board draws when there is nothing to draw.
fn empty_view(
    tickets: &[Ticket],
    visible: usize,
    filters: &Filters,
    projects: &[crate::domain::ProjectWithLanes],
) -> Option<EmptyView> {
    if tickets.is_empty() {
        return Some(EmptyView {
            title: "Nothing on the board".to_string(),
            subtitle: "Add a card from a column's foot, or from the CLI.".to_string(),
            hero: true,
        });
    }
    if visible != 0 {
        return None;
    }
    if !filters.text.trim().is_empty() {
        return Some(EmptyView {
            title: "No matches".to_string(),
            subtitle: "No ticket title matches your search.".to_string(),
            hero: false,
        });
    }
    if !filters.lane.is_empty() || !filters.assignee.is_empty() {
        return Some(EmptyView {
            title: "No tickets match these filters".to_string(),
            subtitle: "Clear a filter to see more.".to_string(),
            hero: false,
        });
    }
    if !filters.project.is_empty()
        && projects
            .iter()
            .any(|project| project.name == filters.project)
    {
        return Some(EmptyView {
            title: format!("No tickets in {}", filters.project),
            subtitle: "Add a card from a column's foot, or show every project.".to_string(),
            hero: false,
        });
    }
    Some(EmptyView {
        title: "The board is empty".to_string(),
        subtitle: "Add a card from a column's foot.".to_string(),
        hero: false,
    })
}

/// The board the current store and filters draw, or the read failure that kept
/// it from being drawn.
fn render_board(
    context: &SessionContext,
    filters: &Filters,
    notice: Option<String>,
) -> Result<BoardView, String> {
    let board = context.board();
    let (tickets, revision) = board
        .list_snapshot(&TicketQuery::default())
        .map_err(|error| error.message())?;
    let (projects, _) = board.projects_snapshot().map_err(|error| error.message())?;
    let (lanes, _) = board.lanes_snapshot().map_err(|error| error.message())?;
    let shown = visible(&tickets, &projects, filters);
    let shown = shown.unwrap_or_default();

    let project_options = {
        let mut options = vec![ProjectOption {
            value: String::new(),
            label: "All projects".to_string(),
            selected: filters.project.is_empty()
                || !projects
                    .iter()
                    .any(|project| project.name == filters.project),
        }];
        for project in &projects {
            options.push(ProjectOption {
                value: project.name.clone(),
                label: project.name.clone(),
                selected: project.name == filters.project,
            });
        }
        options
    };

    let lane_options = {
        let mut options = vec![
            LaneOption {
                value: String::new(),
                label: "All lanes".to_string(),
                selected: filters.lane.is_empty(),
            },
            LaneOption {
                value: " ".to_string(),
                label: "No lane".to_string(),
                selected: filters.lane == " ",
            },
        ];
        for lane in &lanes {
            options.push(LaneOption {
                value: lane.clone(),
                label: lane.clone(),
                selected: filters.lane == *lane,
            });
        }
        options
    };

    let assignee_options = {
        let mut names: Vec<String> = Vec::new();
        for ticket in &tickets {
            let Some(name) = &ticket.assignee else {
                continue;
            };
            let name = name.trim().to_string();
            if name.is_empty() || names.contains(&name) {
                continue;
            }
            names.push(name);
        }
        let mut options = vec![
            AssigneeOption {
                value: String::new(),
                label: "Anyone".to_string(),
                selected: filters.assignee.is_empty(),
            },
            AssigneeOption {
                value: " ".to_string(),
                label: "Unassigned".to_string(),
                selected: filters.assignee == " ",
            },
        ];
        for name in names {
            options.push(AssigneeOption {
                value: name.clone(),
                label: if name == "human" {
                    "human".to_string()
                } else {
                    format!("@{name}")
                },
                selected: filters.assignee == name,
            });
        }
        if !filters.assignee.is_empty()
            && filters.assignee != " "
            && !options
                .iter()
                .any(|option| option.value == filters.assignee)
        {
            options.push(AssigneeOption {
                value: filters.assignee.clone(),
                label: format!("@{}", filters.assignee),
                selected: true,
            });
        }
        options
    };

    let columns = Status::WORKFLOW
        .iter()
        .map(|status| ColumnView {
            status: status.as_str(),
            label: status.display_name(),
            count: shown
                .iter()
                .filter(|ticket| ticket.status == *status)
                .count(),
            cards: shown
                .iter()
                .filter(|ticket| ticket.status == *status)
                .map(|ticket| CardView {
                    id: ticket.id.to_string(),
                    title: ticket.title.clone(),
                    reference: ticket.reference.clone(),
                    lane: ticket.lane.as_deref().map(|name| LaneView {
                        name: name.to_string(),
                        tone: lane_tone(name),
                    }),
                    assignee: assignee_view(ticket.assignee.as_deref()),
                    blocked: block_label(ticket),
                    selected: filters.selected.as_deref() == Some(ticket.id.to_string().as_str()),
                })
                .collect(),
        })
        .collect();

    let empty = empty_view(&tickets, shown.len(), filters, &projects);
    let url = filters.url();
    Ok(BoardView {
        revision,
        self_url: url.clone(),
        dismiss_url: url,
        notice,
        form: FiltersView {
            project: filters.project.clone(),
            show_all: filters.show_all,
            lane: filters.lane.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        project_options,
        lane_options,
        assignee_options,
        show_all: filters.show_all,
        empty,
        columns,
    })
}

// MARK: - Routes

fn is_partial(headers: &HeaderMap) -> bool {
    headers.get("hx-request").is_some()
}

fn render_response(
    context: &SessionContext,
    filters: &Filters,
    notice: Option<String>,
    headers: &HeaderMap,
    status: StatusCode,
) -> Response {
    let view = match render_board(context, filters, notice) {
        Ok(view) => view,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let board = match view.render() {
        Ok(board) => board,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    if is_partial(headers) {
        return html_response(board, status);
    }
    match (PageView {
        board: askama::filters::Safe(board),
    })
    .render()
    {
        Ok(html) => html_response(html, status),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the shell could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

fn html_response(html: String, status: StatusCode) -> Response {
    let mut response = (status, Html(html)).into_response();
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    response
}

async fn board_page(
    State(context): State<Arc<SessionContext>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let filters = Filters::parse(&params);
    render_response(&context, &filters, None, &headers, StatusCode::OK)
}

/// A mutating POST's two guards: the form content type and a same-origin
/// `Origin` when one is sent. A browser sends `Origin` on every cross-origin
/// POST, so the check keeps an untrusted page from writing to loopback.
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

async fn add_ticket(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return render_response(
            &context,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            &headers,
            StatusCode::OK,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => return bad_request(&error.message()),
    };
    let lane = filters.add_lane();
    let title = field(&params, "title");
    match context.board().add(
        title,
        AddTicket {
            lane: lane.as_deref(),
            status,
            assignee: None,
            description: None,
            origin: context.actor.as_str(),
        },
    ) {
        Ok(ticket) => {
            let mut filters = filters;
            filters.selected = Some(ticket.id.to_string());
            render_response(&context, &filters, None, &headers, StatusCode::OK)
        }
        Err(error) => bad_request(&error.message()),
    }
}

async fn move_ticket(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return render_response(
            &context,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            &headers,
            StatusCode::OK,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => return bad_request(&error.message()),
    };
    let board = context.board();
    let ticket = match board.ticket(&id) {
        Ok(ticket) => ticket,
        Err(error) => return bad_request(&error.message()),
    };
    // The column the card already stands in has no order to write, so a drop
    // there writes nothing (the renderer's rule).
    if ticket.status != status
        && let Err(error) = board.set(
            &id,
            SetFields {
                status: Edit::Set(status),
                ..SetFields::default()
            },
        )
    {
        return bad_request(&error.message());
    }
    render_response(&context, &filters, None, &headers, StatusCode::OK)
}

async fn events(
    State(context): State<Arc<SessionContext>>,
    Extension(changes): Extension<broadcast::Sender<i64>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    // The stream opens on the revision the board was drawn from, so a client
    // that attaches after a write (a reload, a restored store) still reads the
    // current number before any new change.
    let current = context.revision();
    let initial = tokio_stream::once(Ok::<_, Infallible>(
        SseEvent::default()
            .event("changed")
            .data(current.to_string()),
    ));
    let lagged = Arc::clone(&context);
    let updates = BroadcastStream::new(changes.subscribe()).filter_map(move |item| match item {
        Ok(revision) => Some(Ok(SseEvent::default()
            .event("changed")
            .data(revision.to_string()))),
        // A slow reader lagged past the channel: the honest answer is the
        // store's current revision, which makes it re-fetch the board.
        Err(_) => {
            let revision = lagged.revision();
            Some(Ok(SseEvent::default()
                .event("changed")
                .data(revision.to_string())))
        }
    });
    Sse::new(initial.chain(updates)).keep_alive(KeepAlive::default())
}

async fn static_asset(Path(path): Path<String>) -> Response {
    let asset: Option<(&'static str, &'static [u8])> = match path.as_str() {
        "tokens.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("static/tokens.css"),
        )),
        "app.css" => Some(("text/css; charset=utf-8", include_bytes!("static/app.css"))),
        "htmx.min.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("static/htmx.min.js"),
        )),
        "sse.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("static/sse.js"),
        )),
        "tandem.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("static/tandem.js"),
        )),
        _ => None,
    };
    match asset {
        Some((kind, bytes)) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(kind)),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests;
