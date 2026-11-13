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

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};
use crate::domain::markdown::{MarkdownBlock, TicketMarkdown};
use crate::domain::text::TextMatch;
use crate::domain::{
    AddTicket, AssigneeAxis, Board, Edit, LaneAxis, SetFields, Status, Ticket, TicketQuery,
};
use crate::env::Environment;
use crate::store::{Actor, RevisionWatch, Store};

mod markdown;

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
        .route("/tickets/{id}", get(ticket_page).post(save_ticket))
        .route("/tickets/{id}/status", post(move_ticket))
        .route("/tickets/{id}/description", post(append_description))
        .route("/tickets/{id}/comments", post(add_comment))
        .route("/tickets/{id}/lane", post(move_to_lane))
        .route("/lanes", post(add_lane))
        .route("/lanes/{name}/rename", post(rename_lane))
        .route("/notes", get(notes_page))
        .route("/notes/view", get(note_view))
        .route("/notes/edit", get(note_edit))
        .route("/notes/preview", post(note_preview))
        .route("/notes/save", post(note_save))
        .route("/notes/create", post(note_create))
        .route("/notes/create-folder", post(note_create_folder))
        .route("/notes/move", post(note_move))
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
    /// The face template's own output, already HTML: the shell embeds it
    /// rather than escaping it a second time.
    body: askama::filters::Safe<String>,
}

#[derive(Template)]
#[template(path = "board.html")]
struct BoardView {
    revision: i64,
    self_url: String,
    dismiss_url: String,
    /// The filters as a query string, so a card's link to its sheet carries
    /// them and the sheet opens over the board the human was looking at.
    board_query: String,
    notice: Option<String>,
    form: FiltersView,
    project_options: Vec<ProjectOption>,
    lane_options: Vec<LaneOption>,
    assignee_options: Vec<AssigneeOption>,
    show_all: bool,
    empty: Option<EmptyView>,
    columns: Vec<ColumnView>,
    /// The ticket sheet, drawn inside the board root so an htmx swap of the
    /// root carries both. `None` on the plain board.
    sheet: Option<askama::filters::Safe<String>>,
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

    // The columns are the six stages `Status` owns, and that is the whole set a
    // ticket can hold: the workflow tables that once recorded a seventh stage
    // retired in schema v11 (CUT-9), and a stored key outside the six reads back
    // as Backlog (`Status::stored`). No visible ticket is left out of a column.
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
        board_query: filters.query(),
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
        sheet: None,
    })
}

/// The board with a sheet drawn inside its root, so one htmx swap carries both.
fn board_with_sheet(
    context: &SessionContext,
    filters: &Filters,
    sheet: String,
    self_url: String,
    headers: &HeaderMap,
    status: StatusCode,
) -> Response {
    let view = match render_board(context, filters, None) {
        Ok(view) => view,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let mut view = view;
    view.self_url = self_url;
    view.sheet = Some(askama::filters::Safe(sheet));
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
        body: askama::filters::Safe(board),
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
        body: askama::filters::Safe(board),
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

// MARK: - The ticket sheet

/// One status choice in the sheet's metadata row.
struct SheetStatusOption {
    value: &'static str,
    label: &'static str,
    selected: bool,
}

/// One lane the sheet may rename.
struct LaneRow {
    name: String,
    encoded: String,
}

/// One comment, its markdown already drawn.
struct SheetComment {
    author: String,
    time: String,
    html: askama::filters::Safe<String>,
}

/// One dependency edge, drawn as a link to the other ticket.
struct EdgeView {
    id: String,
    reference: String,
    title: String,
    status: &'static str,
}

#[derive(Template)]
#[template(path = "sheet.html")]
struct SheetView {
    revision: i64,
    id: String,
    /// The filters the board behind the sheet was drawn with, as a query
    /// string: every write form carries them, so an answer redraws that board
    /// and the close link lands on it.
    board_query: String,
    /// The same filters as fields, for the hidden inputs every sheet form
    /// carries back.
    filters: FiltersView,
    handle: String,
    origin: String,
    link: Option<String>,
    title: String,
    status: &'static str,
    assignee: String,
    status_options: Vec<SheetStatusOption>,
    lane_options: Vec<LaneOption>,
    lane_rows: Vec<LaneRow>,
    description_empty: bool,
    description_html: askama::filters::Safe<String>,
    description_source: String,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
    notice: Option<String>,
    comments: Vec<SheetComment>,
    blocked_by: Vec<EdgeView>,
    blocks: Vec<EdgeView>,
    blocked_reason: Option<String>,
}

/// The UTC stamp a comment header draws: whole seconds since the epoch, as the
/// date and minute the store wrote, never an invented zone.
fn utc_stamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3_600,
        (rest % 3_600) / 60
    )
}

/// Howard Hinnant's `civil_from_days`: the date a day count from 1970-01-01
/// names, proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn comment_author(author: &str) -> String {
    let author = author.trim();
    if author.is_empty() {
        "anonymous".to_string()
    } else if author == "human" {
        "human".to_string()
    } else {
        format!("@{author}")
    }
}

fn edge_view(edge: &crate::domain::TicketBlock) -> EdgeView {
    EdgeView {
        id: edge.id.to_string(),
        reference: edge
            .reference
            .clone()
            .unwrap_or_else(|| "no ref".to_string()),
        title: edge.title.clone(),
        status: edge.status.display_name(),
    }
}

fn sheet_handle(ticket: &Ticket) -> String {
    if let Some(reference) = &ticket.reference {
        return reference.clone();
    }
    match ticket.number {
        Some(number) => format!("#{number}"),
        None => "#?".to_string(),
    }
}

fn sheet_lane_options(ticket: &Ticket, lanes: &[String]) -> Vec<LaneOption> {
    let current = ticket.lane.as_deref();
    let mut options = vec![LaneOption {
        value: String::new(),
        label: "No lane".to_string(),
        selected: current.is_none(),
    }];
    for lane in lanes {
        options.push(LaneOption {
            value: lane.clone(),
            label: lane.clone(),
            selected: current == Some(lane.as_str()),
        });
    }
    if let Some(current) = current
        && !lanes.iter().any(|lane| lane == current)
    {
        options.push(LaneOption {
            value: current.to_string(),
            label: current.to_string(),
            selected: true,
        });
    }
    options
}

fn render_sheet(
    context: &SessionContext,
    id: &str,
    filters: &Filters,
    notice: Option<String>,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
) -> Result<SheetView, String> {
    let board = context.board();
    let (ticket, revision) = board.ticket_snapshot(id).map_err(|error| error.message())?;
    let (comments, _) = board
        .comments_snapshot(id)
        .map_err(|error| error.message())?;
    let (lanes, _) = board.lanes_snapshot().map_err(|error| error.message())?;

    let description_source = ticket.description.clone().unwrap_or_default();
    let description_blocks = TicketMarkdown::blocks(ticket.description.as_deref());
    let description_html = markdown::document(&description_blocks);

    let comments = comments
        .into_iter()
        .map(|comment| SheetComment {
            author: comment_author(&comment.author),
            time: utc_stamp(comment.at),
            html: askama::filters::Safe(markdown::document(&TicketMarkdown::blocks(Some(
                &comment.body,
            )))),
        })
        .collect();

    let status_options = Status::WORKFLOW
        .iter()
        .map(|status| SheetStatusOption {
            value: status.as_str(),
            label: status.display_name(),
            selected: *status == ticket.status,
        })
        .collect();

    Ok(SheetView {
        revision,
        id: ticket.id.to_string(),
        board_query: filters.query(),
        filters: FiltersView {
            project: filters.project.clone(),
            show_all: filters.show_all,
            lane: filters.lane.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        handle: sheet_handle(&ticket),
        origin: ticket.origin.clone(),
        link: ticket.link.clone(),
        title: ticket.title.clone(),
        status: ticket.status.as_str(),
        assignee: ticket.assignee.clone().unwrap_or_default(),
        status_options,
        lane_options: sheet_lane_options(&ticket, &lanes),
        lane_rows: lanes
            .iter()
            .map(|lane| LaneRow {
                name: lane.clone(),
                encoded: encode_component(lane),
            })
            .collect(),
        description_empty: description_blocks.is_empty(),
        description_html: askama::filters::Safe(description_html),
        description_source,
        description_refusal,
        comment_refusal,
        notice,
        comments,
        blocked_by: ticket.blocked_by.iter().map(edge_view).collect(),
        blocks: ticket.blocks.iter().map(edge_view).collect(),
        blocked_reason: ticket.blocked_reason(),
    })
}

/// The sheet as an htmx partial or as a full page: the board is always behind
/// it, so the human can dismiss back to the face they came from. The filters
/// ride the sheet's own request (the query on a GET, the hidden fields on a
/// write form), so the board behind is the one the sheet was opened over.
fn sheet_response(
    context: &SessionContext,
    id: &str,
    params: &HashMap<String, String>,
    notice: Option<String>,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
    headers: &HeaderMap,
) -> Response {
    let filters = Filters::parse(params);
    let view = match render_sheet(
        context,
        id,
        &filters,
        notice,
        description_refusal,
        comment_refusal,
    ) {
        Ok(view) => view,
        Err(error) => return bad_request(&error),
    };
    let html = match view.render() {
        Ok(html) => html,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the sheet could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    board_with_sheet(
        context,
        &filters,
        html,
        filters.url(),
        headers,
        StatusCode::OK,
    )
}

/// One percent-encoded path segment, for a lane name in a route.
fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn form_params(
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Option<HashMap<String, String>> {
    match form {
        Ok(Form(params)) => Some(params),
        Err(_) => None,
    }
}

async fn ticket_page(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    sheet_response(&context, &id, &params, None, None, None, &headers)
}

async fn save_ticket(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &params,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let title = field(&params, "title");
    if title.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &params,
            Some(crate::domain::DomainError::EmptyTitle.message()),
            None,
            None,
            &headers,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => {
            return sheet_response(
                &context,
                &id,
                &params,
                Some(error.message()),
                None,
                None,
                &headers,
            );
        }
    };
    let assignee = field(&params, "assignee");
    let outcome = context.board().set(
        &id,
        SetFields {
            title: Edit::Set(title),
            status: Edit::Set(status),
            assignee: Edit::Set(Some(assignee)),
            ..SetFields::default()
        },
    );
    match outcome {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

async fn append_description(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &params,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let addition = field(&params, "addition");
    if addition.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &params,
            None,
            Some(crate::domain::DomainError::EmptyDescriptionBody.message()),
            None,
            &headers,
        );
    }
    match context.board().set(
        &id,
        SetFields {
            append_description: Edit::Set(addition),
            ..SetFields::default()
        },
    ) {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

async fn add_comment(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let body = field(&params, "body");
    if body.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &params,
            None,
            None,
            Some(crate::domain::DomainError::EmptyCommentBody.message()),
            &headers,
        );
    }
    match context.board().comment_write(&id, body) {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

async fn move_to_lane(
    State(context): State<Arc<SessionContext>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &params,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let lane = field(&params, "lane");
    let lane = if lane.trim().is_empty() {
        None
    } else {
        Some(lane.to_string())
    };
    match context
        .board()
        .move_ticket(&id, lane.as_deref(), usize::MAX)
    {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

/// The ticket a lane write re-draws; a form without one has no sheet to answer.
fn lane_ticket(params: &HashMap<String, String>) -> String {
    field(params, "ticket").trim().to_string()
}

async fn add_lane(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let id = lane_ticket(&params);
    if id.is_empty() {
        return bad_request("a lane write names the ticket it was drawn from");
    }
    let abbr = field(&params, "abbr").trim();
    let key = if abbr.is_empty() { None } else { Some(abbr) };
    let outcome = context.board().add_lane(field(&params, "name"), key, None);
    match outcome {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

async fn rename_lane(
    State(context): State<Arc<SessionContext>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let id = lane_ticket(&params);
    if id.is_empty() {
        return bad_request("a lane write names the ticket it was drawn from");
    }
    let abbr = field(&params, "abbr").trim();
    let key = if abbr.is_empty() { None } else { Some(abbr) };
    let outcome = context
        .board()
        .rename_lane(&name, field(&params, "to"), key, None);
    match outcome {
        Ok(_) => sheet_response(&context, &id, &params, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &params,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

// MARK: - The bikeshed notes

/// The `markdown.html` partial on its own: the notes preview's answer.
#[derive(Template)]
#[template(path = "markdown.html")]
struct MarkdownView {
    body: askama::filters::Safe<String>,
}

fn markdown_view(blocks: &[MarkdownBlock]) -> MarkdownView {
    MarkdownView {
        body: askama::filters::Safe(markdown::blocks(blocks)),
    }
}

struct TreeRowView {
    name: String,
    path: String,
    href: String,
    kind: &'static str,
    level: usize,
    indent: usize,
    folder: bool,
    selected: bool,
}

struct Crumb {
    label: String,
    current: bool,
    first: bool,
}

struct NotePane {
    path: String,
    text: String,
    baseline_hash: String,
    html: askama::filters::Safe<String>,
}

struct BacklinkRow {
    name: String,
    line: String,
    href: String,
}

struct LinkedTicket {
    reference: String,
    title: String,
    href: String,
}

struct ConflictView {
    path: String,
    mine: String,
    theirs: String,
    baseline_hash: String,
}

#[derive(Template)]
#[template(path = "notes.html")]
struct NotesView {
    rows: Vec<TreeRowView>,
    project_options: Vec<ProjectOption>,
    active_project: String,
    show_all: bool,
    path: Option<String>,
    crumbs: Vec<Crumb>,
    mode: &'static str,
    note: Option<NotePane>,
    backlinks: Vec<BacklinkRow>,
    backlink_count: usize,
    linked: Vec<LinkedTicket>,
    linked_count: usize,
    recoveries: Vec<String>,
    recovery_count: usize,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    focus_url: String,
    edit_href: String,
    read_href: String,
}

/// The notes view's own URL: the focus, the show-all escape and one note.
fn notes_href(project: &str, show_all: bool, path: Option<&str>, mode: Option<&str>) -> String {
    let mut pairs: Vec<(&str, String)> = vec![("project", project.to_string())];
    if show_all {
        pairs.push(("show_all", "1".to_string()));
    }
    if let Some(path) = path {
        pairs.push(("path", path.to_string()));
    }
    if let Some(mode) = mode {
        pairs.push(("mode", mode.to_string()));
    }
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("/notes?{query}"),
        Err(_) => "/notes".to_string(),
    }
}

/// The tree the focus draws: the `projects` folder narrowed to the one project
/// folder its name answers, case-insensitively. No focus, no matching folder
/// (ambiguous, renamed or absent) draws the whole vault, so a focus never hides
/// notes.
fn focus_nodes(nodes: Vec<BikeshedNode>, active: Option<&str>) -> Vec<BikeshedNode> {
    let Some(active) = active.filter(|active| !active.trim().is_empty()) else {
        return nodes;
    };
    let wanted = active.trim().to_lowercase();
    let mut out = nodes;
    for node in &mut out {
        if node.kind != NodeKind::Folder || node.path != "projects" {
            continue;
        }
        let narrowed: Vec<BikeshedNode> = node
            .children
            .iter()
            .filter(|child| child.kind == NodeKind::Folder && child.name.to_lowercase() == wanted)
            .cloned()
            .collect();
        if narrowed.len() == 1 {
            node.children = narrowed;
        }
    }
    out
}

fn note_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn flatten_tree(nodes: &[BikeshedNode], depth: usize, rows: &mut Vec<TreeRowView>) {
    for node in nodes {
        let folder = node.kind == NodeKind::Folder;
        rows.push(TreeRowView {
            name: note_name(&node.path),
            path: node.path.clone(),
            href: String::new(),
            kind: if folder { "folder" } else { "note" },
            level: depth + 1,
            indent: 8 + depth * 14,
            folder,
            selected: false,
        });
        if folder {
            flatten_tree(&node.children, depth + 1, rows);
        }
    }
}

fn recoverable_label(recovery: &crate::bikeshed::Recovery) -> String {
    match recovery.file.rsplit_once('/') {
        Some((folder, _)) if !folder.is_empty() => {
            format!("Unsaved draft kept from an interrupted save in {folder}")
        }
        _ => "Unsaved draft kept from an interrupted save".to_string(),
    }
}

fn render_notes(
    context: &SessionContext,
    params: &HashMap<String, String>,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
) -> Result<NotesView, String> {
    let project = field(params, "project").trim().to_string();
    let show_all = !matches!(field(params, "show_all"), "" | "0" | "false");
    let path = params
        .get("path")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mode = if field(params, "mode") == "edit" {
        "edit"
    } else {
        "read"
    };

    let board = context.board();
    let (projects, _) = board.projects_snapshot().map_err(|error| error.message())?;
    let project_options = {
        let mut options = vec![ProjectOption {
            value: String::new(),
            label: "All projects".to_string(),
            selected: project.is_empty(),
        }];
        for project in &projects {
            options.push(ProjectOption {
                value: project.name.clone(),
                label: project.name.clone(),
                selected: project.name == field(params, "project"),
            });
        }
        options
    };

    let vault = context.bikeshed();
    let tree = vault.tree();
    let focused = focus_nodes(tree, if show_all { None } else { Some(&project) });
    let mut rows = Vec::new();
    flatten_tree(&focused, 0, &mut rows);
    for row in &mut rows {
        if row.folder {
            continue;
        }
        row.href = notes_href(&project, show_all, Some(&row.path), None);
        row.selected = path.as_deref() == Some(row.path.as_str());
    }

    let mut note = None;
    let mut crumbs = Vec::new();
    let mut backlinks = Vec::new();
    let mut linked = Vec::new();
    let mut render_error = error;

    if let Some(note_path) = &path {
        match vault.read(note_path) {
            Ok(read) => {
                let content = read.value;
                let folder = note_path
                    .rsplit_once('/')
                    .map(|(folder, _)| folder)
                    .unwrap_or("");
                let mut labels: Vec<String> = if folder.is_empty() {
                    Vec::new()
                } else {
                    folder.split('/').map(str::to_string).collect()
                };
                labels.push(note_name(note_path));
                let mut crumbs_out = vec![Crumb {
                    label: "Bikeshed".to_string(),
                    current: labels.is_empty(),
                    first: true,
                }];
                for (index, label) in labels.iter().enumerate() {
                    crumbs_out.push(Crumb {
                        label: label.clone(),
                        current: index + 1 == labels.len(),
                        first: false,
                    });
                }
                crumbs = crumbs_out;
                note = Some(NotePane {
                    path: note_path.clone(),
                    text: content.text.clone(),
                    baseline_hash: content.hash,
                    html: askama::filters::Safe(markdown::document(&TicketMarkdown::blocks(Some(
                        &content.text,
                    )))),
                });
                match vault.backlinks(note_path) {
                    Ok(read) => {
                        backlinks = read
                            .value
                            .into_iter()
                            .map(|backlink| BacklinkRow {
                                name: note_name(&backlink.path),
                                href: notes_href(&project, show_all, Some(&backlink.path), None),
                                line: backlink.line,
                            })
                            .collect();
                    }
                    Err(error) => {
                        render_error.get_or_insert(error.message());
                    }
                }
                match board.list_snapshot(&TicketQuery::default()) {
                    Ok((tickets, _)) => {
                        linked = tickets
                            .into_iter()
                            .filter(|ticket| {
                                !ticket.archived
                                    && ticket.link.as_deref() == Some(note_path.as_str())
                            })
                            .map(|ticket| LinkedTicket {
                                reference: sheet_handle(&ticket),
                                title: ticket.title,
                                href: format!("/tickets/{}", ticket.id),
                            })
                            .collect();
                    }
                    Err(error) => {
                        render_error.get_or_insert(error.message());
                    }
                }
            }
            Err(error) => {
                render_error.get_or_insert(error.message());
            }
        }
    }

    let recoveries: Vec<String> = vault.recoveries().iter().map(recoverable_label).collect();

    Ok(NotesView {
        rows,
        project_options,
        active_project: field(params, "project").to_string(),
        show_all,
        path: path.clone(),
        crumbs,
        mode,
        note,
        backlink_count: backlinks.len(),
        backlinks,
        linked_count: linked.len(),
        linked,
        recovery_count: recoveries.len(),
        recoveries,
        notice,
        error: render_error,
        conflict,
        focus_url: notes_href(&project, show_all, path.as_deref(), None),
        edit_href: notes_href(&project, show_all, path.as_deref(), Some("edit")),
        read_href: notes_href(&project, show_all, path.as_deref(), None),
    })
}

fn notes_response(
    context: &SessionContext,
    params: &HashMap<String, String>,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    headers: &HeaderMap,
) -> Response {
    let view = match render_notes(context, params, notice, error, conflict) {
        Ok(view) => view,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the notes could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let body = match view.render() {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the notes could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    if is_partial(headers) {
        return html_response(body, StatusCode::OK);
    }
    match (PageView {
        body: askama::filters::Safe(body),
    })
    .render()
    {
        Ok(html) => html_response(html, StatusCode::OK),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the shell could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

async fn notes_page(
    State(context): State<Arc<SessionContext>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    notes_response(&context, &params, None, None, None, &headers)
}

async fn note_view(
    State(context): State<Arc<SessionContext>>,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.remove("mode");
    notes_response(&context, &params, None, None, None, &headers)
}

async fn note_edit(
    State(context): State<Arc<SessionContext>>,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.insert("mode".to_string(), "edit".to_string());
    notes_response(&context, &params, None, None, None, &headers)
}

async fn note_preview(
    State(_context): State<Arc<SessionContext>>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let blocks = TicketMarkdown::blocks(Some(field(&params, "text")));
    match markdown_view(&blocks).render() {
        Ok(html) => html_response(html, StatusCode::OK),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the preview could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

async fn note_save(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let text = field(&params, "text");
    let baseline = field(&params, "baseline_hash");
    let resolution = field(&params, "resolution");
    let vault = context.bikeshed();

    let disk = match vault.read(&path) {
        Ok(read) => read.value,
        Err(error) => {
            return notes_response(
                &context,
                &params,
                None,
                Some(error.message()),
                None,
                &headers,
            );
        }
    };
    if disk.hash != baseline && resolution != "keep_mine" {
        if resolution == "take_theirs" {
            return notes_response(&context, &params, None, None, None, &headers);
        }
        let conflict = ConflictView {
            path: path.clone(),
            mine: text.to_string(),
            theirs: disk.text.clone(),
            baseline_hash: disk.hash.clone(),
        };
        return notes_response(&context, &params, None, None, Some(conflict), &headers);
    }
    match vault.write(&path, text) {
        Ok(_) => notes_response(&context, &params, None, None, None, &headers),
        Err(error) => notes_response(
            &context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

async fn note_create(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, false).await
}

async fn note_create_folder(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, true).await
}

async fn note_create_thing(
    context: &SessionContext,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
    folder: bool,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let vault = context.bikeshed();
    let outcome = if folder {
        vault.create_folder(&path)
    } else {
        vault.create(&path)
    };
    match outcome {
        Ok(read) => {
            if !folder {
                params.insert("path".to_string(), read.value);
            }
            notes_response(context, &params, None, None, None, &headers)
        }
        Err(error) => notes_response(
            context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

async fn note_move(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let into = field(&params, "into").to_string();
    let vault = context.bikeshed();
    match vault.move_into(&path, &into) {
        Ok(read) => {
            if !read.value.is_folder {
                params.insert("path".to_string(), read.value.path);
            } else {
                params.remove("path");
            }
            notes_response(&context, &params, None, None, None, &headers)
        }
        Err(error) => notes_response(
            &context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
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
