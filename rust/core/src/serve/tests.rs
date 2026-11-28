//! The serve seam: the board routes over an in-process router, plus the store
//! watch the SSE route forwards.
//!
//! Every case names a throwaway home and drives the router as a `tower`
//! service, so the assertions are the HTTP facts a browser sees: status,
//! content type, CSP, and the HTML the board draws.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::*;
use crate::domain::{AddTicket, Board, Status};
use crate::store::{Home, NewProject};

/// The serve seam's tests run one at a time. Callsite interest is cached
/// process-wide, so a test on another thread can leave a callsite cached as off
/// for the subscriber a capture test installs; running them in turn keeps every
/// capture whole. The lock is a tokio mutex, so an async test may hold it across
/// its awaits.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The templates that draw a sheet, read as text: a swap that would open a
/// sheet inside a connected page is caught here, before a browser sees it.
const BOARD_TEMPLATE: &str = include_str!("../../templates/board.html");
const SHEET_TEMPLATE: &str = include_str!("../../templates/sheet.html");

/// A throwaway home under the test runner's temp directory.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("tandem-serve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn store(&self) -> Store {
        let home = Home::resolve(Some(&self.root.join("home")), None, &self.root.join("user"));
        Store::new(home)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn context(store: Store) -> SessionContext {
    SessionContext {
        store,
        actor: Actor::Human,
        environment: HashMap::new(),
        access: Access::loopback(7700),
        export_check: ExportCheck::standard(),
    }
}

/// One request, answered as is: a redirect is the answer.
async fn send_once(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body collects")
        .to_bytes();
    (status, headers, String::from_utf8_lossy(&bytes).to_string())
}

/// `send_once`, except that a GET the faces redirect to the project they open
/// on (the remembered one or the first) follows that redirect with the same
/// headers: a test about something else reads the page it lands on. The tests
/// of the redirect itself use `send_once` and `get_once`.
async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let method = request.method().clone();
    let headers = request.headers().clone();
    let (status, answer, body) = send_once(app, request).await;
    let target = answer
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .filter(|location| {
            location.starts_with("/board?project=") || location.starts_with("/notes?project=")
        });
    match target {
        Some(location) if status == StatusCode::SEE_OTHER && method == Method::GET => {
            let mut next = Request::builder().uri(location);
            for (name, value) in &headers {
                next = next.header(name, value);
            }
            send_once(app, next.body(Body::empty()).expect("the request builds")).await
        }
        _ => (status, answer, body),
    }
}

async fn get_once(app: &Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::empty())
        .expect("the request builds");
    send_once(app, request).await
}

async fn get(app: &Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::empty())
        .expect("the request builds");
    send(app, request).await
}

fn form_request(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

/// The index of the column a title sits under: the last `data-status` marker
/// before the title, proved by walking the six markers in render order.
fn column_of(body: &str, title: &str) -> Option<&'static str> {
    let at = body.find(title)?;
    let labels = [
        ("backlog", "Backlog"),
        ("on-hold", "On hold"),
        ("ready", "Ready"),
        ("in-progress", "In progress"),
        ("in-review", "In review"),
        ("done", "Done"),
    ];
    let mut found = None;
    for (key, _) in labels {
        let marker = format!("data-status=\"{key}\"");
        if let Some(index) = body[..at].rfind(&marker)
            && found.is_none_or(|(_, best)| index > best)
        {
            found = Some((key, index));
        }
    }
    found.map(|(key, _)| key)
}

/// The card's own `<a>` tag, found by the ticket it carries.
fn card_tag<'a>(page: &'a str, id: &str) -> &'a str {
    let at = page
        .find(&format!("data-ticket=\"{id}\""))
        .expect("the card draws");
    let start = page[..at].rfind("<a ").expect("the card opens");
    let end = at + page[at..].find('>').expect("the card tag closes");
    &page[start..=end]
}

fn by_title(board: &Board, title: &str) -> Ticket {
    board
        .list(&crate::domain::TicketQuery::default())
        .expect("the list reads")
        .into_iter()
        .find(|ticket| ticket.title == title)
        .unwrap_or_else(|| panic!("the ticket {title} exists"))
}

fn setup_board(store: &Store) -> Board {
    let board = Board::with_actor(store.clone(), Actor::Human);
    board
        .add_project("Alpha", None)
        .expect("the project is added");
    board
        .add_project("Beta", None)
        .expect("the project is added");
    board
        .add_project("Elsewhere", None)
        .expect("the project is added");
    board
        .add_label("alpha", Some("Alpha"))
        .expect("the label is added");
    board
        .add_label("shared", Some("Alpha"))
        .expect("the label is added");
    board
        .add_label("beta", Some("Beta"))
        .expect("the label is added");
    board
        .add(
            "Café alpha",
            AddTicket {
                label: Some("alpha"),
                status: Status::Ready,
                assignee: Some("builder"),
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
        .add(
            "Beta one",
            AddTicket {
                label: Some("beta"),
                status: Status::Backlog,
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
        .add(
            "Loose",
            AddTicket {
                project: Some("Elsewhere"),
                status: Status::Backlog,
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
        .add(
            "Shared alpha",
            AddTicket {
                label: Some("shared"),
                status: Status::InProgress,
                assignee: Some("human"),
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
}

// MARK: - Seam 1: the router

#[tokio::test]
async fn get_board_renders_six_status_columns_in_order_and_empty_copy() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-empty");
    let store = sandbox.store();
    let app = router(context(store));

    let (status, headers, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK, "the board answers: {body}");
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "text/html; charset=utf-8",
        "the board is HTML"
    );
    assert!(
        headers[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap_or("")
            .contains("script-src 'self'"),
        "HTML carries the CSP: {:?}",
        headers[header::CONTENT_SECURITY_POLICY]
    );

    let order = [
        ("backlog", "Backlog"),
        ("on-hold", "On hold"),
        ("ready", "Ready"),
        ("in-progress", "In progress"),
        ("in-review", "In review"),
        ("done", "Done"),
    ];
    let mut cursor = 0;
    for (key, label) in order {
        let marker = format!("data-status=\"{key}\"");
        let at = body[cursor..]
            .find(&marker)
            .unwrap_or_else(|| panic!("the {key} column is drawn in order: {body}"));
        cursor += at + marker.len();
        assert!(body.contains(label), "the {key} column carries its label");
        assert!(
            body.contains(&format!("<option value=\"{key}\">{label}</option>")),
            "the New form offers every column"
        );
    }
    assert!(
        body.contains("Nothing on the board"),
        "an empty store draws its copy: {body}"
    );
    assert!(
        body.contains("Add a card with New, or from the CLI."),
        "an empty home names the New control: {body}"
    );
    assert!(
        !body.contains("column's foot"),
        "no column has a foot to add from: {body}"
    );

    let partial = Request::builder()
        .uri("/board")
        .header(header::HOST, "127.0.0.1:7700")
        .header("hx-request", "true")
        .body(Body::empty())
        .expect("the request builds");
    let (status, _, body) = send(&app, partial).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.starts_with("<div id=\"board-root\""),
        "an htmx request answers the partial alone: {body}"
    );
    assert!(
        !body.contains("<!doctype html>"),
        "the partial carries no shell: {body}"
    );
}

/// The workflow tables retired in schema v11 (CUT-9), so `Status`'s six stages
/// are the whole set a ticket can hold. A home written before that still carries
/// the `selected` stage in a ticket row; the reader maps it back to Backlog, so
/// the card draws in a column instead of vanishing.
#[tokio::test]
async fn a_ticket_in_a_stage_the_board_does_not_draw_still_lands_in_a_column() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-retired-stage");
    let store = sandbox.store();
    setup_board(&store);
    {
        let connection = rusqlite::Connection::open(store.database()).expect("the store opens");
        connection
            .execute(
                "UPDATE tickets SET status = 'selected' WHERE title = 'Loose'",
                [],
            )
            .expect("the stage is written");
    }
    let app = router(context(store));

    let (status, _, body) = get(&app, "/board?project=Elsewhere").await;
    assert_eq!(status, StatusCode::OK, "the board answers: {body}");
    let backlog = body
        .find("data-status=\"backlog\"")
        .expect("the backlog column draws");
    let on_hold = body
        .find("data-status=\"on-hold\"")
        .expect("the on-hold column draws");
    let card = body
        .find("Loose")
        .unwrap_or_else(|| panic!("the card draws: {body}"));
    assert!(
        backlog < card && card < on_hold,
        "a stage the six do not name lands in Backlog: {body}"
    );
}

#[tokio::test]
async fn an_empty_project_draws_its_copy_without_a_stray_comma() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-empty-project");
    let store = sandbox.store();
    let board = Board::with_actor(store.clone(), Actor::Human);
    board
        .add_project("Quiet", None)
        .expect("the project is added");
    board
        .add_project("Busy", None)
        .expect("the project is added");
    board
        .add(
            "Elsewhere card",
            AddTicket {
                project: Some("Busy"),
                status: Status::Backlog,
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    let app = router(context(store));

    let (status, _, body) = get(&app, "/?project=Quiet").await;
    assert_eq!(status, StatusCode::OK, "the board answers: {body}");
    assert!(
        body.contains("No tickets in Quiet"),
        "the title draws: {body}"
    );
    assert!(!body.contains("with New,."), "no stray comma: {body}");
    assert!(
        body.contains("Add a card with New.</"),
        "the empty project names the New control and ends at the full stop: {body}"
    );
    assert!(
        !body.contains("column's foot"),
        "no column has a foot to add from: {body}"
    );
}

#[tokio::test]
async fn get_board_filters_by_project_focus_label_assignee_and_accent_folded_search() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-filters");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, focus) = get(&app, "/?project=Alpha").await;
    assert!(focus.contains("Café alpha"), "the focus draws its label");
    assert!(
        focus.contains("Shared alpha"),
        "the focus draws every label of the project"
    );
    assert!(
        !focus.contains("Beta one"),
        "the focus hides the other project: {focus}"
    );

    // The retired show-all flag on an old link is ignored: the project stays.
    let (_, _, old) = get(&app, "/?project=Alpha&show_all=1").await;
    assert!(
        !old.contains("Beta one"),
        "an old show_all link still draws the named project only: {old}"
    );

    let (_, _, label) = get(&app, "/?project=Beta&label=beta").await;
    assert!(label.contains("Beta one"));
    assert!(
        !label.contains("Café alpha"),
        "the label filter narrows: {label}"
    );

    let (_, _, loose) = get(&app, "/?project=Elsewhere&label=%20").await;
    assert!(loose.contains("Loose"), "the default area is selectable");
    assert!(
        !loose.contains("Beta one"),
        "the default area is one label: {loose}"
    );

    let (_, _, assigned) = get(&app, "/?project=Alpha&assignee=builder").await;
    assert!(assigned.contains("Café alpha"));
    assert!(
        !assigned.contains("Beta one"),
        "the assignee filter narrows: {assigned}"
    );

    let (_, _, searched) = get(&app, "/?project=Alpha&text=cafe").await;
    assert!(
        searched.contains("Café alpha"),
        "the search folds the accent: {searched}"
    );
    assert!(
        !searched.contains("Beta one"),
        "the search is over the title: {searched}"
    );
}

#[tokio::test]
async fn post_tickets_adds_card_in_column_and_refuses_stale_revision_with_notice() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-add");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let before = store.revision().expect("the revision reads");

    let body = format!(
        "title=First&status=ready&ticket_project=Alpha&ticket_label=&expected_revision={before}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "the add answers the board: {page}");
    assert!(page.contains("First"), "the new card is drawn: {page}");
    assert_eq!(
        column_of(&page, "First"),
        Some("ready"),
        "the card lands in its column: {page}"
    );
    assert!(
        page.contains("aria-current=\"true\""),
        "the new card is selected: {page}"
    );
    let added = by_title(&board, "First");
    assert_eq!(added.status, Status::Ready);

    let count = board
        .list(&crate::domain::TicketQuery::default())
        .expect("the list reads")
        .len();
    let stale = format!(
        "title=Second&status=backlog&ticket_project=Alpha&ticket_label=&expected_revision={before}&project=&show_all=&assignee=&text="
    );
    let (status, _, conflict) = send(&app, form_request("/tickets", &stale)).await;
    assert_eq!(status, StatusCode::OK, "a conflict still answers the board");
    assert!(
        conflict.contains("shell__notice"),
        "a stale revision draws the dismissible banner: {conflict}"
    );
    assert!(
        conflict.contains("changed while you were writing"),
        "the banner names the conflict: {conflict}"
    );
    assert!(
        !conflict.contains("Second"),
        "the stale write does not land: {conflict}"
    );
    assert_eq!(
        board
            .list(&crate::domain::TicketQuery::default())
            .expect("the list reads")
            .len(),
        count,
        "the store still holds the tickets it had"
    );
}

#[tokio::test]
async fn post_ticket_status_moves_card_across_columns_and_ignores_same_column_drop() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-status");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let ticket = by_title(&board, "Beta one");
    let revision = store.revision().expect("the revision reads");
    let uri = format!("/tickets/{}/status", ticket.id);

    let body = format!("status=in-review&expected_revision={revision}");
    let (status, _, page) = send(&app, form_request(&uri, &body)).await;
    assert_eq!(status, StatusCode::OK, "the move answers the board: {page}");
    assert_eq!(column_of(&page, "Beta one"), Some("in-review"));
    let moved = by_title(&board, "Beta one");
    assert_eq!(moved.status, Status::InReview);

    let revision = store.revision().expect("the revision reads");
    let same = format!("status=in-review&expected_revision={revision}");
    let (status, _, _) = send(&app, form_request(&uri, &same)).await;
    assert_eq!(status, StatusCode::OK, "a same-column drop still answers");
    assert_eq!(
        store.revision().expect("the revision reads"),
        revision,
        "a same-column drop writes nothing"
    );
}

#[tokio::test]
async fn a_drag_with_a_stale_revision_answers_the_conflict_notice_and_moves_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-status-stale");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let ticket = by_title(&board, "Beta one");
    let stale = store.revision().expect("the revision reads");
    // Another writer commits after the board was drawn.
    board
        .add(
            "Elsewhere",
            crate::domain::AddTicket {
                project: Some("Alpha"),
                ..crate::domain::AddTicket::default()
            },
        )
        .expect("the other write lands");
    let current = store.revision().expect("the revision reads");
    let uri = format!("/tickets/{}/status", ticket.id);

    let body = format!("status=in-review&expected_revision={stale}");
    let (status, _, page) = send(&app, form_request(&uri, &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("changed while you were writing"),
        "the banner names the conflict: {page}"
    );
    assert_eq!(by_title(&board, "Beta one").status, ticket.status);
    assert_eq!(
        store.revision().expect("the revision reads"),
        current,
        "a conflict writes nothing"
    );
}

/// The board refreshes itself over the change stream, so the client must be the
/// htmx 2 extension: the htmx 1 build defines its `hx-trigger="sse:*"` listener
/// and never registers it, reads the whole attribute as the event name, and
/// swaps through `api.selectAndSwap`, which htmx 2 does not expose.
#[tokio::test]
async fn the_change_stream_client_is_the_htmx_two_extension() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-sse-client");
    let app = router(context(sandbox.store()));

    let (status, _, htmx) = get(&app, "/static/htmx.min.js").await;
    assert_eq!(status, StatusCode::OK, "htmx is served");
    assert!(
        htmx.contains("version:\"2."),
        "the extension is written against htmx 2, so the client is htmx 2"
    );

    let (status, _, sse) = get(&app, "/static/sse.js").await;
    assert_eq!(status, StatusCode::OK, "the extension is served");
    assert!(
        sse.contains("htmx-ext-sse 2."),
        "the vendored file names the htmx 2 extension it is: {}",
        &sse[..200.min(sse.len())]
    );
    assert!(
        !sse.contains("selectAndSwap"),
        "the htmx 1 swap API is not in htmx 2"
    );
    assert!(
        sse.contains("api.getTriggerSpecs(elt)"),
        "the sse:* branch reads hx-trigger's parsed specs, so `sse:changed from:body` listens on `changed`"
    );
    assert!(
        sse.contains("source.addEventListener(ts.trigger.slice(4)"),
        "the sse:* branch registers the listener for the parsed event name"
    );

    let (_, _, page) = get(&app, "/board").await;
    assert!(
        page.contains("hx-trigger=\"sse:changed from:body\""),
        "the board waits for the change event: {page}"
    );
}

/// A drop writes a status, and its answer redraws the board: the POST carries
/// the filter bar the board was drawn with, so the partial keeps the human's
/// project, label, assignee and search rather than clearing them.
#[tokio::test]
async fn a_card_move_keeps_the_filters_the_board_was_drawn_with() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-move-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let ticket = by_title(&board, "Beta one");
    let revision = store.revision().expect("the revision reads");
    let uri = format!("/tickets/{}/status", ticket.id);

    let body = format!(
        "status=in-progress&expected_revision={revision}&project=Beta&show_all=&label=beta&assignee=&text=one"
    );
    let (status, _, page) = send(&app, form_request(&uri, &body)).await;
    assert_eq!(status, StatusCode::OK, "the move answers the board: {page}");
    assert_eq!(
        column_of(&page, "Beta one"),
        Some("in-progress"),
        "the card moved"
    );
    assert!(
        !page.contains("Café alpha"),
        "the answer is still drawn through the focus filter: {page}"
    );
    assert!(
        page.contains("<option value=\"Beta\" selected>Beta</option>"),
        "the project filter survives the drop: {page}"
    );
    assert!(
        page.contains("name=\"label\" value=\"beta\" checked"),
        "the label filter survives the drop: {page}"
    );
    assert!(
        page.contains("name=\"text\" value=\"one\""),
        "the search survives the drop: {page}"
    );
    assert!(
        page.contains("hx-get=\"/board?project=Beta"),
        "the refresh URL keeps the filters: {page}"
    );

    let (_, _, script) = get(&app, "/static/tandem.js").await;
    assert!(
        script.contains(".tickets__bar--board") && script.contains("new FormData("),
        "the drop posts the filter bar it was drawn with"
    );
    assert!(
        script.contains("note-tabs__close") && !script.contains("tandem.noteTabs = "),
        "the script keeps the tab keys and no list"
    );
    let (_, _, css) = get(&app, "/static/app.css").await;
    assert!(
        css.contains(".note-tabs__tab[aria-selected=\"true\"]"),
        "the stylesheet draws the active note tab"
    );
    for selector in [
        ".tree__label:focus-visible",
        ".sheet:focus-visible",
        ".button:focus-visible",
        ".relations-toggle:focus-visible",
        ".relations__close:focus-visible",
        ".relations__summary:focus-visible",
        ".relations__row:focus-visible",
    ] {
        assert!(
            css.contains(&format!("{selector},")) || css.contains(&format!("{selector} {{")),
            "the stylesheet keeps the accent focus ring on {selector}"
        );
    }
}

#[tokio::test]
async fn post_routes_reject_missing_form_content_type_and_cross_origin_header() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-guards");
    let store = sandbox.store();
    Board::with_actor(store.clone(), Actor::Human)
        .add_project("Main", None)
        .expect("the project is added");
    let app = router(context(store.clone()));
    let revision = store.revision().expect("the revision reads");
    let body =
        format!("title=Guarded&status=backlog&ticket_project=Main&expected_revision={revision}");

    let missing = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::from(body.clone()))
        .expect("the request builds");
    let (status, _, _) = send(&app, missing).await;
    assert_eq!(
        status,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "a POST without the form content type is refused"
    );

    let cross = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "http://evil.example")
        .body(Body::from(body.clone()))
        .expect("the request builds");
    let (status, _, _) = send(&app, cross).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a cross-origin POST is refused"
    );

    let same = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "http://127.0.0.1:7700")
        .body(Body::from(body))
        .expect("the request builds");
    let (status, _, page) = send(&app, same).await;
    assert_eq!(status, StatusCode::OK, "a same-origin POST lands: {page}");
    assert!(page.contains("Guarded"), "the guarded add landed");
}

#[tokio::test]
async fn the_new_form_files_into_its_own_project_and_label_selects() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-new-form");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));

    // One project is always open, so the Project select is preset to it, is
    // required, and every label carries its project for the page script.
    let (_, _, page) = get(&app, "/board?project=Alpha").await;
    assert!(
        page.contains("<select class=\"new-form__status\" name=\"ticket_project\" aria-label=\"Project\" required>"),
        "the Project select is required: {page}"
    );
    assert!(
        page.contains("<option value=\"Alpha\" selected>Alpha</option>"),
        "the open project is preset: {page}"
    );
    assert!(
        page.contains("<option value=\"alpha\" data-project=\"Alpha\""),
        "a label option names its project: {page}"
    );

    // Focused on a project: it is preset (zero extra clicks), only its labels
    // show, and the filter's label of that project is preset too.
    let (_, _, focused) = get(&app, "/?project=Alpha&label=alpha").await;
    assert!(
        focused.contains("<option value=\"Alpha\" selected>Alpha</option>"),
        "the focused project is preset: {focused}"
    );
    assert!(
        focused.contains(&format!(
            "<option value=\"alpha\" data-project=\"Alpha\" data-tone=\"{}\" selected>",
            super::board::label_tone("alpha")
        )),
        "the filter's label is preset: {focused}"
    );
    assert!(
        focused.contains(&format!(
            "<option value=\"beta\" data-project=\"Beta\" data-tone=\"{}\" hidden>",
            super::board::label_tone("beta")
        )),
        "another project's label is hidden without the script: {focused}"
    );
    assert!(
        !focused.contains("name=\"ticket_label\" aria-label=\"Label\" disabled"),
        "a chosen project enables the Label select"
    );
    let _ = board;

    // Filing through the selects lands the ticket in that project and label.
    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=Filed&status=ready&ticket_project=Alpha&ticket_label=shared&expected_revision={revision}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let filed = by_title(&board, "Filed");
    assert_eq!(filed.project, "Alpha");
    assert_eq!(filed.label.as_deref(), Some("shared"));
}

#[tokio::test]
async fn a_post_without_a_project_or_with_a_foreign_label_re_renders_with_a_notice() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-new-refused");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let count = || {
        board
            .list(&crate::domain::TicketQuery::default())
            .expect("the list reads")
            .len()
    };
    let before = count();

    // The hidden filter fields are never read as the new ticket's project.
    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=Nowhere&status=ready&ticket_project=&ticket_label=&expected_revision={revision}&project=Alpha&show_all=&label=alpha&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "a refusal still answers the board");
    assert!(
        page.contains("shell__notice") && page.contains("Name a project"),
        "the refusal renders as the board's notice and names the recovery: {page}"
    );
    assert_eq!(count(), before, "nothing was written");

    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=Mismatch&status=ready&ticket_project=Beta&ticket_label=alpha&expected_revision={revision}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("shell__notice") && page.contains("belongs to project"),
        "a label of another project is refused with the label's project named: {page}"
    );
    assert_eq!(count(), before);
}

#[tokio::test]
async fn an_empty_home_draws_the_new_form_disabled_with_the_recovery() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-new-empty-home");
    let store = sandbox.store();
    let app = router(context(store));
    let (_, _, page) = get(&app, "/").await;
    assert!(
        page.contains("name=\"title\" placeholder=\"Ticket title\" aria-label=\"Ticket title\" required disabled"),
        "the title is disabled: {page}"
    );
    assert!(
        page.contains("class=\"new-form__create\" type=\"submit\" disabled"),
        "Create is disabled: {page}"
    );
    assert!(
        page.contains("tandem-cli project add</code> first."),
        "the hint names the recovery: {page}"
    );
}

// MARK: - The write guard as a layer, and the request's context

/// A write route added to the table without a guard call of its own: the layer
/// must refuse it all the same.
async fn unguarded_write() -> Response {
    html_response("written".to_string(), StatusCode::OK)
}

/// A route that reads the context its request resolved, to name the seam.
async fn resolved_context(context: Context) -> Response {
    html_response(
        format!(
            "{}|{}",
            context.actor.as_str(),
            context.store.home().directory().display()
        ),
        StatusCode::OK,
    )
}

fn extra_router(extra: Router<Arc<SessionContext>>, store: Store) -> Router {
    let (changes, _) = broadcast::channel(64);
    layered(extra, changes, Access::loopback(7700), store.clone())
        .with_state(Arc::new(context(store)))
}

#[tokio::test]
async fn write_guard_is_a_layer_so_a_route_cannot_skip_it_by_omission() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("guard-layer");
    let app = extra_router(
        routes().route("/unguarded", post(unguarded_write)),
        sandbox.store(),
    );

    let missing = Request::builder()
        .method("POST")
        .uri("/unguarded")
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::from("x=1"))
        .expect("the request builds");
    let (status, _, _) = send(&app, missing).await;
    assert_eq!(
        status,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "the guard is a layer, not a call a handler can forget"
    );

    let cross = Request::builder()
        .method("POST")
        .uri("/unguarded")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "http://evil.example")
        .body(Body::from("x=1"))
        .expect("the request builds");
    let (status, _, _) = send(&app, cross).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a cross-origin write is refused"
    );

    let same = Request::builder()
        .method("POST")
        .uri("/unguarded")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "http://127.0.0.1:7700")
        .body(Body::from("x=1"))
        .expect("the request builds");
    let (status, _, body) = send(&app, same).await;
    assert_eq!(status, StatusCode::OK, "a same-origin form write lands");
    assert_eq!(body, "written", "the route itself never touched the guard");
}

#[tokio::test]
async fn the_context_resolves_per_request_to_local_modes_one_context() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("context-extractor");
    let store = sandbox.store();
    let expected = format!(
        "{}|{}",
        Actor::Human.as_str(),
        store.home().directory().display()
    );
    let app = extra_router(
        routes().route("/context", axum::routing::get(resolved_context)),
        store,
    );

    let (status, _, first) = get(&app, "/context").await;
    assert_eq!(status, StatusCode::OK, "the route answers: {first}");
    assert_eq!(first, expected, "the route read the router's context");

    let (_, _, second) = get(&app, "/context").await;
    assert_eq!(
        second, first,
        "local mode resolves every request to the same single context"
    );
}

#[tokio::test]
async fn static_assets_serve_css_htmx_and_js_and_html_sets_csp() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-static");
    let app = router(context(sandbox.store()));

    for (path, kind) in [
        ("/static/tokens.css", "text/css; charset=utf-8"),
        ("/static/app.css", "text/css; charset=utf-8"),
        ("/static/htmx.min.js", "text/javascript; charset=utf-8"),
        ("/static/sse.js", "text/javascript; charset=utf-8"),
        ("/static/tandem.js", "text/javascript; charset=utf-8"),
        ("/static/graph.js", "text/javascript; charset=utf-8"),
        ("/static/theme.js", "text/javascript; charset=utf-8"),
        ("/static/icon.svg", "image/svg+xml"),
    ] {
        let (status, headers, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::OK, "{path} is served");
        assert_eq!(
            headers[header::CONTENT_TYPE],
            kind,
            "{path} carries its type"
        );
        assert!(!body.is_empty(), "{path} carries its bytes");
        if path == "/static/graph.js" {
            assert!(
                body.contains("tandem.graph.options"),
                "graph.js carries its options key"
            );
            assert!(
                body.contains("tandem.graph.pins"),
                "graph.js carries its pins key"
            );
            assert!(
                !body.contains("history.pushState"),
                "graph.js lets htmx own the push, so Back restores a snapshot with the graph"
            );
            assert!(
                body.contains("hx-push-url"),
                "graph.js opens a node through an htmx push"
            );
        }
        if path == "/static/theme.js" {
            assert!(
                body.contains("classList.add('js')"),
                "theme.js marks <html> with the js class before the first paint"
            );
        }
        if path == "/static/app.css" {
            for key in [
                "lower-alpha",
                "lower-roman",
                "--space-list",
                "lp__line--hint",
                ".js .live[data-live-store]:not([data-live-ready])",
            ] {
                assert!(body.contains(key), "app.css carries {key}");
            }
        }
        if path == "/static/tandem.js" {
            for key in [
                "MARK: - Popover menus",
                "details.menu, details.row-menu",
                "document.addEventListener('pointerdown'",
                "tandem.collapsed",
                "tandem.collapsedSet",
                "tandem.relations.open",
                "tandem.relations.sections",
                "data-tree-action",
                "uiState",
                "data-ui-cookie",
                "data-live-store",
                "htmx:afterSwap', setUpNote",
                "live:follow",
                "/notes/open",
                "data-live-final-newline",
                "data-placeholder",
                "enterToBody",
                "'.page-title'",
                "lp__line--hint",
                "SLASH_HINT",
                "--depth",
                "data-label",
                "indentKey",
                "enterList",
                "backspaceList",
                "renumberRun",
                "Tab indents. Press Escape, then Tab, to leave the editor.",
                "data-autosave",
                "data-picker",
                "/field'",
            ] {
                assert!(body.contains(key), "tandem.js carries {key}");
            }
        }
    }

    let (status, headers, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
    let csp = headers[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .expect("the CSP is text");
    assert!(csp.contains("default-src 'self'"), "the CSP is set: {csp}");
    assert!(csp.contains("frame-ancestors 'self'"));
    assert!(
        body.contains("/static/app.css"),
        "the shell loads the styles"
    );
    assert!(
        body.contains("hx-ext=\"sse\""),
        "the shell connects the change stream"
    );
    // A linked icon keeps the browser from asking for /favicon.ico, a 404
    // that logged a warning on every page load.
    assert!(
        body.contains("rel=\"icon\" href=\"/static/icon.svg?v="),
        "the shell links the icon"
    );

    let (status, _, _) = get(&app, "/static/unknown.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an unknown asset is a 404");
}

/// The hash a hashed asset URL carries, read off the `v=` parameter.
fn hash_of(url: &str) -> &str {
    url.rsplit("v=").next().expect("the URL carries a hash")
}

async fn get_with(
    app: &Router,
    uri: &str,
    name: header::HeaderName,
    value: &str,
) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(name, value)
        .body(Body::empty())
        .expect("the request builds");
    send(app, request).await
}

#[test]
fn an_asset_digest_is_twelve_hex_characters_and_follows_the_bytes() {
    let first = assets::digest(b"a { color: red }");
    assert_eq!(first.len(), 12, "the hash is a short hex prefix: {first}");
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(
        first,
        assets::digest(b"a { color: red }"),
        "the hash is stable"
    );
    assert_ne!(
        first,
        assets::digest(b"a { color: blue }"),
        "a changed byte changes the hash"
    );
}

#[tokio::test]
async fn the_shell_links_content_hashed_assets() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-asset-urls");
    let app = router(context(sandbox.store()));

    let (status, _, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    let urls = assets::urls();
    assert!(
        body.contains(&format!("href=\"{}\"", urls.app_css)),
        "the shell links the hashed styles: {body}"
    );
    assert!(
        body.contains(&format!("src=\"{}\"", urls.htmx)),
        "the shell links the hashed htmx"
    );
    assert!(
        body.contains(&format!("src=\"{}\"", urls.tandem)),
        "the shell links the hashed tandem.js"
    );
    assert!(
        body.contains(&format!("src=\"{}\" defer", urls.graph)),
        "the shell links the hashed graph.js with defer: {body}"
    );
    assert!(
        !body.contains("href=\"/static/app.css\""),
        "no unhashed styles link remains"
    );
    assert!(
        !body.contains("src=\"/static/tandem.js\""),
        "no unhashed script link remains"
    );
}

#[tokio::test]
async fn a_hashed_asset_is_immutable_and_an_unhashed_one_revalidates() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-asset-cache");
    let app = router(context(sandbox.store()));
    let urls = assets::urls();
    let hash = hash_of(&urls.app_css).to_string();

    let (status, headers, _) = get(&app, &urls.app_css).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable",
        "the current hash is immutable"
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[header::CONTENT_TYPE], "text/css; charset=utf-8");

    let (status, headers, _) = get(&app, "/static/app.css").await;
    assert_eq!(status, StatusCode::OK, "the unhashed URL still answers");
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "no-cache",
        "an unhashed asset revalidates"
    );
    assert_eq!(
        headers[header::ETAG],
        format!("\"{hash}\"").as_str(),
        "the unhashed asset carries its hash as ETag"
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

    let (status, headers, _) = get(&app, "/static/app.css?v=stale0000000").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "no-cache",
        "a stale hash revalidates"
    );
}

#[tokio::test]
async fn a_matching_if_none_match_answers_not_modified() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-asset-304");
    let app = router(context(sandbox.store()));
    let hash = hash_of(&assets::urls().app_css).to_string();
    let etag = format!("\"{hash}\"");

    let (status, _, body) = get_with(&app, "/static/app.css", header::IF_NONE_MATCH, &etag).await;
    assert_eq!(
        status,
        StatusCode::NOT_MODIFIED,
        "a matching validator answers 304"
    );
    assert!(body.is_empty(), "a 304 carries no body");

    let (status, _, _) =
        get_with(&app, "/static/app.css", header::IF_NONE_MATCH, "\"other\"").await;
    assert_eq!(status, StatusCode::OK, "a stale validator gets the bytes");
}

// MARK: - The watch

#[test]
fn a_refused_store_backs_off_to_a_cap() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let mut wait = WATCH_INTERVAL;
    for _ in 0..16 {
        wait = backoff(wait);
    }
    assert_eq!(wait, WATCH_BACKOFF_MAX);
    assert!(WATCH_BACKOFF_MAX > WATCH_INTERVAL);
}

/// A home whose store carries no revision row, which is what a pre-v4 file
/// presents to `Store::watch`: `Ok(None)`, not an error. Unix only with the
/// counter the case reads.
#[cfg(unix)]
fn store_without_a_revision(name: &str) -> (PathBuf, Store) {
    let root =
        std::env::temp_dir().join(format!("tandem-serve-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
    let store = Store::new(home);
    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "probe",
                key: "",
                last_number: 0,
                position: 0,
            })
        })
        .expect("the first write creates the schema");
    let connection =
        rusqlite::Connection::open(root.join("home").join("tandem.db")).expect("the store opens");
    connection
        .execute_batch("DROP TABLE store_meta")
        .expect("the revision table is droppable");
    (root, store)
}

/// A home with one write, so the watch can attach and read a revision.
fn store_with_a_revision(name: &str) -> (PathBuf, Store) {
    let root =
        std::env::temp_dir().join(format!("tandem-serve-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
    let store = Store::new(home);
    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "probe",
                key: "",
                last_number: 0,
                position: 0,
            })
        })
        .expect("the first write creates the schema");
    (root, store)
}

fn seen(receiver: &mut broadcast::Receiver<i64>) -> Vec<i64> {
    let mut revisions = Vec::new();
    while let Ok(revision) = receiver.try_recv() {
        revisions.push(revision);
    }
    revisions
}

#[cfg(unix)]
#[test]
fn a_store_with_no_revision_backs_off_instead_of_reopening() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let (root, store) = store_without_a_revision("no-revision");
    let store = Arc::new(store);
    let stop = Arc::new(AtomicBool::new(false));
    let opens_before = store.watch_opens();
    let (changes, _) = broadcast::channel(8);
    let handle = {
        let store = Arc::clone(&store);
        let stop = Arc::clone(&stop);
        let changes = changes.clone();
        thread::spawn(move || watch((*store).clone(), changes, stop))
    };
    thread::sleep(Duration::from_secs(1));
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join().expect("the watch stops");
    let opens = store.watch_opens() - opens_before;
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        opens <= 10,
        "a pre-v4 store is opened {opens} times in a second, not at the 40 ms cadence"
    );
}

#[test]
fn an_idle_watch_issues_no_revision_read() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let (root, store) = store_with_a_revision("idle");
    let store = Arc::new(store);
    let stop = Arc::new(AtomicBool::new(false));
    let reads_before = store.watch_revision_reads();
    let (changes, mut receiver) = broadcast::channel(8);
    let handle = {
        let store = Arc::clone(&store);
        let stop = Arc::clone(&stop);
        let changes = changes.clone();
        thread::spawn(move || watch((*store).clone(), changes, stop))
    };
    thread::sleep(Duration::from_millis(600));
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join().expect("the watch stops");
    let reads = store.watch_revision_reads() - reads_before;
    let events = seen(&mut receiver);
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        reads <= 2,
        "an idle watch read the revision {reads} times in 600 ms, not once at attach"
    );
    assert!(
        events.is_empty(),
        "an idle watch emits no change event: {events:?}"
    );
}

#[test]
fn a_write_from_another_store_reports_changed_quickly() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let (root, store) = store_with_a_revision("latency");
    let store = Arc::new(store);
    let stop = Arc::new(AtomicBool::new(false));
    let reads_before = store.watch_revision_reads();
    let (changes, mut receiver) = broadcast::channel(8);
    let handle = {
        let store = Arc::clone(&store);
        let stop = Arc::clone(&stop);
        let changes = changes.clone();
        thread::spawn(move || watch((*store).clone(), changes, stop))
    };
    // The write must land after the watch's baseline: a write that lands
    // before it leaves the baseline already at the new revision and no event
    // ever fires. The attach's first revision read proves the thread is up.
    let attached = Instant::now() + Duration::from_secs(10);
    while store.watch_revision_reads() == reads_before && Instant::now() < attached {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        store.watch_revision_reads() > reads_before,
        "the watch never attached"
    );
    thread::sleep(WATCH_INTERVAL * 2);
    let before = store.revision().expect("the revision reads");
    let started = Instant::now();
    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "probe2",
                key: "",
                last_number: 0,
                position: 0,
            })
        })
        .expect("the other writer writes");
    let moved = store.revision().expect("the revision moved");
    assert!(moved > before, "the write moved the revision");
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        events.extend(seen(&mut receiver));
        if events.contains(&moved) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join().expect("the watch stops");
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        events.contains(&moved),
        "a write to {moved} never arrived within 2 s (saw {events:?})"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "a write took {elapsed:?} to arrive, past the 40 ms ceiling plus jitter"
    );
}

// A restore renames another file over the database, and that file's revision
// can equal the one the watch last broadcast: a restore to the revision before
// the latest write, followed by a write the watch has not seen yet, lands on
// the very same number. The board behind that number is a different one, so
// the reattach must report it rather than read the equal number as no change.
#[cfg(unix)]
#[test]
fn a_replaced_store_at_the_same_revision_still_reports_changed() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let (root, store) = store_with_a_revision("replaced-held");
    let (other_root, other) = store_with_a_revision("replaced-other");
    let revision = store.revision().expect("the revision reads");
    assert_eq!(
        other.revision().expect("the other revision reads"),
        revision,
        "the replacement carries the same revision"
    );
    // One self-contained file, the shape a backup has: no WAL beside it.
    let replacement = other_root.join("replacement.db");
    rusqlite::Connection::open(other.database())
        .expect("the other store opens")
        .execute("VACUUM INTO ?1", [replacement.to_string_lossy()])
        .expect("the other store copies into one file");
    let store = Arc::new(store);
    let stop = Arc::new(AtomicBool::new(false));
    let reads_before = store.watch_revision_reads();
    let (changes, mut receiver) = broadcast::channel(8);
    let handle = {
        let store = Arc::clone(&store);
        let stop = Arc::clone(&stop);
        let changes = changes.clone();
        thread::spawn(move || watch((*store).clone(), changes, stop))
    };
    let attached = Instant::now() + Duration::from_secs(10);
    while store.watch_revision_reads() == reads_before && Instant::now() < attached {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        store.watch_revision_reads() > reads_before,
        "the watch never attached"
    );

    let _ = std::fs::remove_file(store.wal());
    let _ = std::fs::remove_file(store.home().directory().join("tandem.db-shm"));
    std::fs::rename(&replacement, store.database()).expect("the replacement moves over the path");

    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        events.extend(seen(&mut receiver));
        if events.contains(&revision) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join().expect("the watch stops");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&other_root);
    assert!(
        events.contains(&revision),
        "the replaced store's revision {revision} never arrived (saw {events:?})"
    );
}

// MARK: - Seam 1b: the ticket sheet and the notes notes

use crate::domain::{Edit, SetFields};
use crate::notes::Notes;

fn sheet_get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header("hx-request", "true")
        .body(Body::empty())
        .expect("the request builds")
}

fn page_get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::empty())
        .expect("the request builds")
}

fn vault(store: &Store) -> Notes {
    Notes::new(store.home().directory().join("notes"))
}

#[tokio::test]
async fn the_board_and_the_sheet_say_label_and_never_lane() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("label-vocabulary");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));

    let (status, _, page) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK, "the board answers: {page}");
    assert!(page.contains("labels-menu"), "the Labels menu is drawn");
    assert!(page.contains(">Labels</summary>"), "the menu reads Labels");
    assert!(page.contains(">alpha<"), "the chip carries the label name");
    assert!(
        !page.to_lowercase().contains("lane"),
        "the board has no lane strings"
    );

    let (status, _, sheet) = send(&app, sheet_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(status, StatusCode::OK, "the sheet answers: {sheet}");
    assert!(
        sheet.contains("data-autosave=\"label\""),
        "the sheet's select is the label select"
    );
    assert!(
        !sheet.to_lowercase().contains("lane"),
        "the sheet has no lane strings"
    );
}

#[tokio::test]
async fn get_ticket_sheet_renders_markdown_blocks_metadata_row_and_comments() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-render");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    board
        .set(
            &ticket.id.to_string(),
            SetFields {
                description: Edit::Set(Some("## Why\n\nSome **bold** and `code`.\n\n- one\n- two")),
                ..SetFields::default()
            },
        )
        .expect("the description is set");
    board
        .comment_write(&ticket.id.to_string(), "A first comment")
        .expect("the comment lands");
    let beta = by_title(&board, "Beta one");
    board
        .block(&ticket.id.to_string(), &beta.id.to_string())
        .expect("the ticket waits on beta");
    let loose = by_title(&board, "Loose");
    board
        .block(&loose.id.to_string(), &ticket.id.to_string())
        .expect("loose waits on the ticket");
    let app = router(context(store));

    let (status, headers, body) = send(&app, sheet_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(status, StatusCode::OK, "the sheet answers: {body}");
    assert!(
        headers.contains_key(header::CONTENT_SECURITY_POLICY),
        "the sheet carries the policy"
    );
    for marker in [
        "sheet-backdrop",
        "role=\"dialog\"",
        "sheet__sticky",
        "sheet__meta",
        "data-status=\"ready\"",
        "name=\"assignee\"",
        "name=\"label\"",
        "Café alpha",
        "data-live",
        "data-live-store=\"sheet-description-store\"",
        "data-autosave=\"description\"",
        "data-placeholder=\"Add a more detailed description",
        "<h",
        "<code",
        "## Why",
        "A first comment",
        "Blocked by",
        "Blocks",
    ] {
        assert!(body.contains(marker), "the sheet draws {marker}: {body}");
    }
    for gone in [
        "data-description-edit",
        "sheet__description-view",
        "sheet__edit",
    ] {
        assert!(!body.contains(gone), "the sheet no longer draws {gone}");
    }
    assert!(
        !body.contains("<!doctype html>"),
        "an htmx request answers the partial: {body}"
    );

    let (status, _, page) = send(&app, page_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("<!doctype html>"),
        "a plain request draws the shell"
    );
    assert!(
        page.contains("board__column"),
        "the board sits behind the sheet"
    );
    assert!(page.contains("sheet-backdrop"), "the sheet is open");
}

#[tokio::test]
async fn a_sheet_without_a_description_draws_the_live_surface_with_its_placeholder() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-empty-description");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));
    let (status, _, body) = send(&app, sheet_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("sheet__description-live")
            && body.contains("data-live-final-newline=\"none\"")
            && body.contains("data-placeholder=\"Add a more detailed description"),
        "the live surface shows with its placeholder: {body}"
    );
    assert!(
        !body.contains("sheet__description-view") && !body.contains("data-description-edit"),
        "no rendered block and no Edit button: {body}"
    );
}

#[tokio::test]
async fn a_sheet_description_carries_its_broken_links() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-broken-links");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let notes = vault(&store);
    notes
        .create("An existing note.md")
        .expect("the note is created");
    board
        .set(
            &ticket.id.to_string(),
            SetFields {
                description: Edit::Set(Some("See [[No such note]] and [[An existing note]].")),
                ..SetFields::default()
            },
        )
        .expect("the description is set");
    let app = router(context(store));
    let (status, _, body) = send(&app, sheet_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(status, StatusCode::OK);
    let attr = body
        .split("data-broken-links=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the surface carries data-broken-links");
    assert!(
        attr.contains("no such note") && !attr.contains("an existing note"),
        "only the missing link is broken: {attr}"
    );
}

#[tokio::test]
async fn a_description_field_save_keeps_no_trailing_newline() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-description-newline");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));
    let text = "Line one\n\n- item";
    let (status, answer) = save_field(&app, &id, &field_body("description", text, "")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    assert_eq!(
        board.ticket(&id).expect("reads").description.as_deref(),
        Some(text),
        "the text reads back unchanged"
    );
    let (_, again) = save_field(&app, &id, &field_body("description", "Next", text)).await;
    assert!(
        again.contains("data-state=\"saved\""),
        "was stayed equal to what the server holds: {again}"
    );
}

/// One field save: the body the sheet's script posts.
fn field_body(field: &str, value: &str, was: &str) -> String {
    let encode = |text: &str| super::sheet::encode_component(text);
    format!("field={field}&value={}&was={}", encode(value), encode(was))
}

async fn save_field(app: &Router, id: &str, body: &str) -> (StatusCode, String) {
    let (status, _, answer) = send(app, form_request(&format!("/tickets/{id}/field"), body)).await;
    (status, answer)
}

#[tokio::test]
async fn a_title_line_break_saves_as_a_space() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-title-newline");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));
    let (status, answer) = save_field(
        &app,
        &id,
        &field_body("title", "One\nTwo\r\nThree", "Café alpha"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    assert_eq!(board.ticket(&id).expect("reads").title, "One Two Three");
}

#[tokio::test]
async fn a_field_save_writes_only_that_field() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-only");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    board
        .set(
            &id,
            SetFields {
                description: Edit::Set(Some("Body")),
                ..SetFields::default()
            },
        )
        .expect("the description is set");
    let app = router(context(store));

    let cases: [(&str, &str, &str); 5] = [
        ("title", "Renamed", "Café alpha"),
        ("status", "in-review", "ready"),
        ("assignee", "reviewer", "builder"),
        ("description", "Rewritten", "Body"),
        ("label", "shared", "alpha"),
    ];
    for (field, value, was) in cases {
        let before = board.ticket(&id).expect("reads");
        let (status, answer) = save_field(&app, &id, &field_body(field, value, was)).await;
        assert_eq!(status, StatusCode::OK, "{field}: {answer}");
        assert!(answer.contains("data-state=\"saved\""), "{field}: {answer}");
        let after = board.ticket(&id).expect("reads");
        let changed = [
            ("title", before.title != after.title),
            ("status", before.status != after.status),
            ("assignee", before.assignee != after.assignee),
            ("description", before.description != after.description),
            ("label", before.label != after.label),
        ];
        for (name, did) in changed {
            assert_eq!(did, name == field, "saving {field} changed {name}");
        }
    }
    let saved = board.ticket(&id).expect("reads");
    assert_eq!(saved.title, "Renamed");
    assert_eq!(saved.status, Status::InReview);
    assert_eq!(saved.assignee.as_deref(), Some("reviewer"));
    assert_eq!(saved.description.as_deref(), Some("Rewritten"));
    assert_eq!(saved.label.as_deref(), Some("shared"));

    let (_, answer) = save_field(&app, &id, &field_body("label", "", "shared")).await;
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    assert_eq!(board.ticket(&id).expect("reads").label, None);
}

#[tokio::test]
async fn a_field_save_ignores_writes_to_other_fields_and_tickets() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-others");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let other = by_title(&board, "Beta one");
    let id = ticket.id.to_string();
    let app = router(context(store));

    board
        .set(
            &other.id.to_string(),
            SetFields {
                title: Edit::Set("Elsewhere"),
                ..SetFields::default()
            },
        )
        .expect("the other ticket is written");
    board
        .set(
            &id,
            SetFields {
                assignee: Edit::Set(Some("agent")),
                ..SetFields::default()
            },
        )
        .expect("another field is written");
    let (_, answer) = save_field(&app, &id, &field_body("title", "Mine", "Café alpha")).await;
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    let saved = board.ticket(&id).expect("reads");
    assert_eq!(saved.title, "Mine");
    assert_eq!(
        saved.assignee.as_deref(),
        Some("agent"),
        "the other field stays"
    );
}

#[tokio::test]
async fn a_field_save_with_a_stale_was_answers_conflict_and_writes_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-conflict");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store.clone()));
    board
        .set(
            &id,
            SetFields {
                assignee: Edit::Set(Some("agent \"x\" <b>")),
                ..SetFields::default()
            },
        )
        .expect("the field is written elsewhere");
    let revision = store.revision().expect("the revision reads");

    let (status, answer) = save_field(&app, &id, &field_body("assignee", "mine", "builder")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"conflict\""), "{answer}");
    assert!(
        answer.contains("data-theirs=\"agent &#34;x&#34; &#60;b&#62;\""),
        "theirs rides the answer, escaped: {answer}"
    );
    assert!(!answer.contains("<b>"), "{answer}");
    assert_eq!(
        board.ticket(&id).expect("reads").assignee.as_deref(),
        Some("agent \"x\" <b>")
    );
    assert_eq!(
        store.revision().expect("the revision reads"),
        revision,
        "a conflict writes nothing"
    );
}

#[tokio::test]
async fn keep_mine_after_a_conflict_writes_with_theirs_as_was() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-keep-mine");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));
    board
        .set(
            &id,
            SetFields {
                assignee: Edit::Set(Some("agent")),
                ..SetFields::default()
            },
        )
        .expect("the field is written elsewhere");

    let (_, answer) = save_field(&app, &id, &field_body("assignee", "mine", "agent")).await;
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    assert!(answer.contains("data-was=\"mine\""), "{answer}");
    assert_eq!(
        board.ticket(&id).expect("reads").assignee.as_deref(),
        Some("mine")
    );
}

#[tokio::test]
async fn an_empty_title_field_save_is_refused_with_the_domain_message() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-empty-title");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));
    let (status, answer) = save_field(&app, &id, &field_body("title", "  \n ", "Café alpha")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"refused\""), "{answer}");
    assert!(
        answer.contains(&format!(
            "data-reason=\"{}\"",
            crate::domain::DomainError::EmptyTitle.message()
        )),
        "{answer}"
    );
    assert_eq!(board.ticket(&id).expect("reads").title, "Café alpha");
}

fn create_body(value: &str, was: &str) -> String {
    format!("{}&create=1", field_body("label", value, was))
}

#[tokio::test]
async fn the_sheet_label_select_is_a_picker_with_toned_options() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label-picker");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (status, _, sheet) = send(&app, sheet_get(&format!("/tickets/{id}"))).await;
    assert_eq!(status, StatusCode::OK, "{sheet}");
    assert!(sheet.contains("data-picker"), "{sheet}");
    assert!(
        sheet.contains("data-picker-create=\"Create label\""),
        "{sheet}"
    );
    assert!(
        sheet.contains("data-picker-placeholder=\"Change label"),
        "{sheet}"
    );
    for name in ["alpha", "shared"] {
        let tone = super::board::label_tone(name);
        assert!(
            sheet.contains(&format!("<option value=\"{name}\" data-tone=\"{tone}\"")),
            "{name} carries tone {tone}: {sheet}"
        );
    }
}

#[tokio::test]
async fn a_label_field_save_with_create_makes_the_label_in_the_tickets_project_and_applies_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label-create");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store.clone()));
    let (status, answer) = save_field(&app, &id, &create_body("fresh", "alpha")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"saved\""), "{answer}");
    let tone = super::board::label_tone("fresh");
    assert!(
        answer.contains(&format!("data-tone=\"{tone}\"")),
        "{answer}"
    );
    assert_eq!(
        board.ticket(&id).expect("reads").label.as_deref(),
        Some("fresh")
    );
    let projects = board.projects().expect("projects read");
    let alpha = projects.iter().find(|p| p.name == "Alpha").expect("alpha");
    assert!(
        alpha.labels.iter().any(|l| l == "fresh"),
        "{:?}",
        alpha.labels
    );
    let tree = vault(&store).tree();
    let found = tree
        .iter()
        .find(|n| n.name == "projects")
        .and_then(|n| n.children.iter().find(|c| c.name == "Alpha"))
        .and_then(|n| n.children.iter().find(|c| c.name == "fresh"));
    assert!(found.is_some(), "the vault folder exists: {tree:?}");
}

#[tokio::test]
async fn creating_a_label_whose_name_exists_in_another_project_is_refused_and_moves_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label-create-dup");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let labels = board.labels().expect("labels read");
    let (status, answer) = save_field(&app, &id, &create_body("beta", "alpha")).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(answer.contains("data-state=\"refused\""), "{answer}");
    assert!(answer.contains("already exists"), "{answer}");
    assert_eq!(
        board.ticket(&id).expect("reads").label.as_deref(),
        Some("alpha")
    );
    assert_eq!(board.labels().expect("labels read"), labels);
}

#[tokio::test]
async fn a_label_create_with_a_stale_was_answers_conflict_and_creates_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label-create-stale");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let labels = board.labels().expect("labels read");
    let (_, answer) = save_field(&app, &id, &create_body("fresh", "shared")).await;
    assert!(answer.contains("data-state=\"conflict\""), "{answer}");
    assert_eq!(board.labels().expect("labels read"), labels);
    assert_eq!(
        board.ticket(&id).expect("reads").label.as_deref(),
        Some("alpha")
    );
}

#[tokio::test]
async fn a_label_create_with_a_blank_name_is_refused_with_the_domain_message() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label-create-blank");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (_, answer) = save_field(&app, &id, &create_body("   ", "alpha")).await;
    assert!(answer.contains("data-state=\"refused\""), "{answer}");
    assert!(
        answer.contains(&crate::domain::DomainError::EmptyLabelName.message()),
        "{answer}"
    );
    assert_eq!(
        board.ticket(&id).expect("reads").label.as_deref(),
        Some("alpha")
    );
}

#[tokio::test]
async fn create_on_a_field_other_than_label_is_a_bad_request() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-create-other-field");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let body = format!("{}&create=1", field_body("title", "Renamed", "Café alpha"));
    let (status, _) = save_field(&app, &id, &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(board.ticket(&id).expect("reads").title, "Café alpha");
}

#[tokio::test]
async fn the_new_form_creates_a_new_label_in_the_chosen_project_and_files_the_ticket_in_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("new-form-label-create");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=Filed&status=ready&ticket_project=Beta&ticket_label=brandnew&ticket_label_new=1&expected_revision={revision}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let filed = by_title(&board, "Filed");
    assert_eq!(filed.project, "Beta");
    assert_eq!(filed.label.as_deref(), Some("brandnew"));
    let projects = board.projects().expect("projects read");
    let beta = projects.iter().find(|p| p.name == "Beta").expect("beta");
    assert!(beta.labels.iter().any(|l| l == "brandnew"));
}

#[tokio::test]
async fn the_new_form_with_a_new_label_and_an_empty_title_creates_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("new-form-label-empty-title");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let labels = board.labels().expect("labels read");
    let count = board
        .list(&crate::domain::TicketQuery::default())
        .expect("list")
        .len();
    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=%20%20&status=ready&ticket_project=Beta&ticket_label=brandnew&ticket_label_new=1&expected_revision={revision}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(&crate::domain::DomainError::EmptyTitle.message()),
        "{page}"
    );
    assert_eq!(board.labels().expect("labels read"), labels);
    assert_eq!(
        board
            .list(&crate::domain::TicketQuery::default())
            .expect("list")
            .len(),
        count
    );
}

#[tokio::test]
async fn the_new_form_without_the_new_flag_still_refuses_an_unknown_label() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("new-form-label-unknown");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let labels = board.labels().expect("labels read");
    let revision = store.revision().expect("the revision reads");
    let body = format!(
        "title=Typo&status=ready&ticket_project=Beta&ticket_label=brandnew&expected_revision={revision}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("shell__notice"), "{page}");
    assert_eq!(board.labels().expect("labels read"), labels);
}

#[tokio::test]
async fn the_new_form_label_select_is_a_picker_with_toned_options() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("new-form-label-picker");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let app = router(context(store));
    let (_, _, page) = get(&app, "/?project=Alpha").await;
    let tone = super::board::label_tone("shared");
    assert!(
        page.contains(&format!(
            "<option value=\"shared\" data-project=\"Alpha\" data-tone=\"{tone}\""
        )),
        "{page}"
    );
    assert!(
        page.contains("data-picker-create=\"Create label\""),
        "{page}"
    );
}

#[tokio::test]
async fn an_unknown_field_is_a_bad_request() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-unknown");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (status, _) = save_field(&app, &id, &field_body("origin", "x", "human")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_field_answer_is_a_fragment_not_the_board_root() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-field-fragment");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (_, answer) = save_field(&app, &id, &field_body("status", "done", "ready")).await;
    assert!(answer.contains("data-field-result"), "{answer}");
    assert!(answer.contains("data-field=\"status\""), "{answer}");
    assert!(!answer.contains("id=\"board-root\""), "{answer}");
}

#[tokio::test]
async fn the_sheet_draws_no_save_button_and_no_save_key() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-no-save");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (_, _, page) = send(&app, sheet_get(&format!("/tickets/{id}"))).await;
    // The board's own forms stay behind the sheet; only the sheet is judged.
    let sheet = &page[page
        .find("class=\"sheet-backdrop\"")
        .expect("the sheet draws")..];
    for gone in [
        "sheet__save\"",
        "sheet-save",
        "expected_revision",
        "Saves with Save",
        "Moves at once",
    ] {
        assert!(!sheet.contains(gone), "the sheet has no {gone}: {sheet}");
    }
    assert!(
        sheet.contains("Changes save as you go"),
        "the footer hint says so: {sheet}"
    );
    assert!(!sheet.contains("Cmd/Ctrl+Enter saves"), "{sheet}");
    for field in ["title", "status", "assignee", "label", "description"] {
        assert!(
            sheet.contains(&format!("data-autosave=\"{field}\"")),
            "{field} autosaves: {sheet}"
        );
    }
    assert!(sheet.contains(&format!("data-ticket=\"{id}\"")), "{sheet}");
    assert!(sheet.contains("data-save-state"), "{sheet}");
}

#[tokio::test]
async fn the_sheet_width_rule_is_the_wide_one() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-width");
    let app = router(context(sandbox.store()));
    let (_, _, css) = get(&app, "/static/app.css").await;
    assert!(css.contains("width: min(920px, 100%)"), "the sheet is wide");
    assert!(
        !css.contains(".sheet {\n  width: 640px")
            && !css
                .contains(".sheet {\n  display: flex;\n  flex-direction: column;\n  width: 580px"),
        "no narrow sheet width is left"
    );
}

#[tokio::test]
async fn a_comment_posts_with_a_field_failure_showing_and_the_failed_field_is_put_back() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-comment-failure");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let app = router(context(store));
    let (_, _, script) = get(&app, "/static/tandem.js").await;
    // A checkout may carry CRLF line endings: match on LF only.
    let script = script.replace("\r\n", "\n");
    let confirm_at = script
        .find("var issue = event.detail.issueRequest;\n    flushSheet(sheet).then(issue);")
        .expect("the sheet hold issues the request once the saves land, failure or not");
    let hold = &script[confirm_at.saturating_sub(400)..confirm_at];
    assert!(
        !hold.contains("failedFields()"),
        "a settled failure no longer holds the post: {hold}"
    );
    for part in [
        "addEventListener('htmx:beforeSwap'",
        "addEventListener('htmx:load'",
        "showFailure(sheet, field, carry.failures[field])",
        "control.setAttribute('data-was', kept.was)",
    ] {
        assert!(
            script.contains(part),
            "the failed fields are carried across the swap: {part}"
        );
    }
}

#[tokio::test]
async fn post_ticket_comment_appends_as_human_and_refuses_empty_body() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-comment");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));

    let (status, _, answer) = send(
        &app,
        form_request(&format!("/tickets/{id}/comments"), "body=hello"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the comment answers: {answer}");
    let comments = board.comments(&id).expect("the log reads");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].author, "human");
    assert_eq!(comments[0].body, "hello");
    assert!(
        answer.contains("hello"),
        "the sheet redraws the comment: {answer}"
    );

    let (status, _, answer) = send(
        &app,
        form_request(&format!("/tickets/{id}/comments"), "body=%20%20"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        answer.contains("sheet__refusal"),
        "an empty comment is refused: {answer}"
    );
    assert!(
        answer.contains("Comment body is empty."),
        "the refusal carries the domain's wording: {answer}"
    );
    assert_eq!(
        board.comments(&id).expect("the log reads").len(),
        1,
        "a refused comment writes nothing"
    );
}

#[tokio::test]
async fn post_ticket_label_moves_immediately_and_label_add_and_rename_update_choices() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-label");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));

    let (status, answer) = save_field(&app, &id, &field_body("label", "shared", "alpha")).await;
    assert_eq!(status, StatusCode::OK, "the label move answers: {answer}");
    assert_eq!(
        board
            .ticket(&id)
            .expect("the ticket reads")
            .label
            .as_deref(),
        Some("shared"),
        "the label moves at once"
    );

    let (status, _, answer) = send(&app, form_request("/labels", "name=gamma&project=Alpha")).await;
    assert_eq!(status, StatusCode::OK, "the label add answers: {answer}");
    assert!(
        board
            .labels()
            .expect("the labels read")
            .iter()
            .any(|label| label == "gamma"),
        "the label is declared"
    );
    assert!(
        answer.contains("value=\"gamma\""),
        "the board's label menu carries the new label: {answer}"
    );

    let (status, _, answer) = send(&app, form_request("/labels/gamma/rename", "to=delta")).await;
    assert_eq!(status, StatusCode::OK, "the label rename answers: {answer}");
    let labels = board.labels().expect("the labels read");
    assert!(
        labels.iter().any(|label| label == "delta"),
        "the label is renamed"
    );
    assert!(
        !labels.iter().any(|label| label == "gamma"),
        "the old name is gone"
    );
    assert!(
        answer.contains("delta"),
        "the board redraws the rename: {answer}"
    );

    let (status, _, answer) = send(&app, form_request("/labels", "name=alpha&abbr=ALP")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        answer.contains("shell__notice"),
        "a duplicate label draws a notice: {answer}"
    );
    assert!(
        answer.contains("already exists"),
        "the notice carries the domain's wording: {answer}"
    );
}

#[tokio::test]
async fn get_notes_renders_tree_focus_breadcrumb_and_markdown_preview_with_footer() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-render");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes
        .write("Index.md", "## Hello\n\nBody text", None)
        .expect("the note is written");
    notes.create("Other.md").expect("the note is created");
    notes
        .write("Other.md", "See [[Index]]", None)
        .expect("the note is written");
    notes
        .create("projects/alpha/inside.md")
        .expect("the note is created");
    notes
        .create("projects/beta/outside.md")
        .expect("the note is created");
    board
        .set_link(&ticket.id.to_string(), Some("Index.md"))
        .expect("the ticket links the note");
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes?path=Index.md")).await;
    assert_eq!(status, StatusCode::OK, "the notes face answers: {body}");
    for marker in [
        "<!doctype html>",
        "notes-root",
        "tree__row",
        "Index",
        "notes__breadcrumb",
        "Notes",
        "markdown__heading",
        "Hello",
        "Body text",
        "Backlinks",
        "Other",
        "Tickets",
        "Café alpha",
        "data-live",
        "data-live-store=\"note-text\"",
        "data-placeholder=\"Start writing, or type / for commands\"",
        "id=\"note-text\"",
        "aria-label=\"Tree\"",
        "data-tree-action=\"expand\"",
        "data-tree-action=\"collapse\"",
        "aria-label=\"Expand all\"",
        "aria-label=\"Collapse all\"",
    ] {
        assert!(
            body.contains(marker),
            "the notes face draws {marker}: {body}"
        );
    }

    let (_, _, read) = send(&app, page_get("/notes?path=Index.md&mode=read")).await;
    assert!(
        !read.contains("data-live"),
        "mode=read draws the page without the editor: {read}"
    );
    assert!(
        !read.contains("Start writing, or type"),
        "mode=read carries no placeholder: {read}"
    );

    let (_, _, focused) = send(&app, page_get("/notes?project=Alpha&path=Index.md")).await;
    assert!(
        focused.contains("inside"),
        "the focused tree keeps the project's folder: {focused}"
    );
    assert!(
        !focused.contains("data-path=\"projects/beta/outside.md\""),
        "the focused tree drops another project's folder: {focused}"
    );
}

#[tokio::test]
async fn the_notes_page_draws_the_relations_pane_with_collapsible_sections() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-relations");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes.create("Other.md").expect("the note is created");
    notes
        .write("Other.md", "See [[Index]]", None)
        .expect("the note is written");
    board
        .set_link(&ticket.id.to_string(), Some("Index.md"))
        .expect("the ticket links the note");
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes?path=Index.md")).await;
    for marker in [
        "<aside class=\"relations\" id=\"relations\" aria-label=\"Relations\"",
        "data-section=\"backlinks\"",
        "data-section=\"tickets\"",
        "aria-controls=\"relations\"",
        "aria-expanded=",
        "aria-label=\"Close relations\"",
    ] {
        assert!(body.contains(marker), "the pane draws {marker}: {body}");
    }
    assert!(!body.contains("note-footer"), "the footer is gone: {body}");
    let section = |kind: &str| {
        let at = body
            .find(&format!("data-section=\"{kind}\""))
            .unwrap_or_else(|| panic!("section {kind}"));
        let start = body[..at].rfind("<details").expect("a details");
        let end = at + body[at..].find('>').expect("a tag end");
        body[start..end].to_string()
    };
    assert!(
        section("backlinks").contains(" open"),
        "Backlinks starts open"
    );
    assert!(
        !section("tickets").contains(" open"),
        "Tickets starts collapsed"
    );

    // An empty Backlinks renders collapsed with its empty line.
    let (_, _, lonely) = send(&app, page_get("/notes?path=Other.md")).await;
    let at = lonely.find("data-section=\"backlinks\"").expect("section");
    let start = lonely[..at].rfind("<details").expect("a details");
    let end = at + lonely[at..].find('>').expect("a tag end");
    assert!(
        !lonely[start..end].contains(" open"),
        "an empty section is collapsed"
    );
    assert!(lonely.contains("Nothing links here yet."), "the empty line");
}

#[tokio::test]
async fn the_edit_page_keeps_a_leading_blank_line_in_the_note_text() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-leading-newline");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.create("Lead.md").expect("the note is created");
    notes
        .write("Lead.md", "\n\nfirst line", None)
        .expect("the note is written");
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes/edit?path=Lead.md")).await;
    assert_eq!(status, StatusCode::OK, "the edit page answers: {body}");
    // The HTML parser drops one newline right after the opening tag, so the
    // template emits a spare one and the text keeps its own.
    // A checkout may carry the template with CRLF line ends.
    let body = body.replace("\r\n", "\n");
    assert!(
        body.contains("hidden>\n\n\nfirst line</textarea>"),
        "the textarea keeps the leading blank lines: {body}"
    );
}

#[tokio::test]
async fn post_notes_preview_renders_markdown_blocks_without_writing_disk() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-preview");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        form_request("/notes/preview", "text=%23%23+Draft%0A%0A**bold**"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the preview answers: {body}");
    assert!(
        body.contains("markdown__heading"),
        "a heading is drawn: {body}"
    );
    assert!(
        body.contains("<strong>bold</strong>"),
        "inline styles are drawn: {body}"
    );
    assert!(
        notes.read("Draft.md").is_err(),
        "a preview writes no note to disk"
    );
}

#[tokio::test]
async fn post_notes_save_writes_when_hash_matches_and_renders_mine_theirs_conflict_when_disk_moved()
{
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-save");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes
        .write("Index.md", "one", None)
        .expect("the note is written");
    let first = notes.read("Index.md").expect("the note reads").value;
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/save",
            &format!("path=Index.md&text=two&baseline_hash={}", first.hash),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the save answers: {body}");
    assert_eq!(
        notes.read("Index.md").expect("the note reads").value.text,
        "two",
        "a save at the baseline writes the text"
    );

    let second = notes.read("Index.md").expect("the note reads").value;
    notes
        .write("Index.md", "theirs", None)
        .expect("the other writer lands");
    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/save",
            &format!("path=Index.md&text=mine&baseline_hash={}", second.hash),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for marker in ["conflict", "Mine", "Theirs", "Keep mine", "Take theirs"] {
        assert!(body.contains(marker), "the conflict draws {marker}: {body}");
    }
    assert!(
        body.contains("mine") && body.contains("theirs"),
        "both bodies are drawn: {body}"
    );
    assert_eq!(
        notes.read("Index.md").expect("the note reads").value.text,
        "theirs",
        "a conflict writes nothing"
    );

    let (status, _, _) = send(
        &app,
        form_request(
            "/notes/save",
            &format!(
                "path=Index.md&text=mine&baseline_hash={}&resolution=keep_mine",
                second.hash
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        notes.read("Index.md").expect("the note reads").value.text,
        "mine",
        "Keep mine writes the buffer over the disk"
    );
}

#[tokio::test]
async fn post_notes_rename_and_delete_manage_a_note_and_the_open_page_follows() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-manage");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, _) = send(
        &app,
        form_request(
            "/notes/create",
            "path=projects/tandem/Draft.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, headers, body) = send(
        &app,
        form_request(
            "/notes/rename",
            "path=projects/tandem/Draft.md&name=Final&open=projects/tandem/Draft.md&project=tandem&mode=edit",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the rename answers: {body}");
    assert!(
        notes.read("projects/tandem/Final.md").is_ok(),
        "the note is renamed"
    );
    assert!(
        notes.read("projects/tandem/Draft.md").is_err(),
        "the old name is gone"
    );
    assert!(
        body.contains("Final"),
        "the open page follows the rename: {body}"
    );
    assert_eq!(
        headers
            .get("hx-replace-url")
            .and_then(|value| value.to_str().ok()),
        Some("/notes?project=tandem&path=projects%2Ftandem%2FFinal.md&mode=edit"),
        "the address bar follows the rename"
    );

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/delete",
            "path=projects/tandem/Final.md&open=projects/tandem/Final.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the delete answers: {body}");
    assert!(
        notes.read("projects/tandem/Final.md").is_err(),
        "the note is deleted"
    );
    assert!(
        body.contains("No note selected"),
        "the deleted page closes: {body}"
    );
    assert!(
        !body.contains("data-note-tabs"),
        "the deleted note leaves no strip: {body}"
    );
}

#[tokio::test]
async fn deleting_a_note_offers_an_undo_and_posting_it_restores_and_reopens_the_note() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-undo");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, _) = send(
        &app,
        form_request(
            "/notes/create",
            "path=projects/tandem/Undoable.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/delete",
            "path=projects/tandem/Undoable.md&open=projects/tandem/Undoable.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the delete answers: {body}");
    assert!(
        notes.read("projects/tandem/Undoable.md").is_err(),
        "the note left the vault"
    );
    assert!(
        body.contains("Deleted projects/tandem/Undoable.md"),
        "the notice names the delete: {body}"
    );
    assert!(
        body.contains("action=\"/notes/restore\""),
        "the notice carries the undo control: {body}"
    );
    let marker = "name=\"trash\" value=\"";
    let start = body.find(marker).expect("the undo names its trash entry") + marker.len();
    let end = start + body[start..].find('"').expect("the entry value closes");
    let trash = body[start..end].to_string();
    assert!(
        trash.contains("/projects/tandem/Undoable.md"),
        "the entry names the original path: {trash}"
    );

    let (status, _, body) = send(
        &app,
        form_request("/notes/restore", &format!("trash={trash}&project=tandem")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the restore answers: {body}");
    assert!(
        notes.read("projects/tandem/Undoable.md").is_ok(),
        "the undo puts the note back"
    );
    assert!(
        body.contains("Restored projects/tandem/Undoable.md"),
        "the notice names the restore: {body}"
    );
    assert!(
        body.contains("Undoable"),
        "the restored note is open again: {body}"
    );

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/restore",
            "trash=..%2F..%2Foutside.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a refused restore still answers: {body}"
    );
    assert!(
        body.contains("banner"),
        "a crafted restore is refused on the page: {body}"
    );
}

#[tokio::test]
async fn post_notes_new_files_untitled_in_the_current_folder_with_the_title_focused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-new");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        form_request("/notes/new", "dir=projects/tandem&project=tandem"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the new note answers: {body}");
    assert!(
        notes.read("projects/tandem/Untitled.md").is_ok(),
        "an empty Untitled page is filed in the current folder"
    );
    assert!(
        body.contains("data-autofocus"),
        "the cursor goes to the title: {body}"
    );

    let (status, _, _) = send(
        &app,
        form_request("/notes/new", "dir=projects/tandem&project=tandem"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        notes.read("projects/tandem/Untitled 2.md").is_ok(),
        "a second new note does not collide"
    );
}

#[tokio::test]
async fn post_notes_daily_creates_the_days_page_once_and_opens_it_after() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-daily");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, body) = send(&app, form_request("/notes/daily", "date=2026-11-14")).await;
    assert_eq!(status, StatusCode::OK, "the daily note answers: {body}");
    assert!(
        notes.read("daily/2026-11-14.md").is_ok(),
        "today's page is created"
    );
    assert!(body.contains("2026-11-14"), "the page opens: {body}");

    let (status, _, again) = send(&app, form_request("/notes/daily", "date=2026-11-14")).await;
    assert_eq!(status, StatusCode::OK, "a second press opens it: {again}");
    assert!(
        !again.contains("shell__error"),
        "no error on reopen: {again}"
    );

    let (status, _, _) = send(&app, form_request("/notes/daily", "date=../../etc")).await;
    assert_eq!(status, StatusCode::OK, "a malformed date falls back");
    assert!(
        notes.read("daily/../../etc.md").is_err(),
        "a client date never steers the path"
    );
}

/// An htmx request that says which URL the browser shows now.
fn htmx_from(method: &str, uri: &str, current: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("hx-request", "true")
        .header("hx-current-url", current)
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

#[tokio::test]
async fn the_url_follows_the_open_note_push_for_a_new_note_replace_for_a_save() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-url-follows");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.write("a.md", "one", None).expect("a is written");
    notes.write("b.md", "two", None).expect("b is written");
    let app = router(context(store));

    let (status, headers, body) = send(
        &app,
        htmx_from(
            "GET",
            "/notes?path=b.md",
            "http://127.0.0.1:7700/notes?path=a.md",
            "",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pushed = headers
        .get("hx-push-url")
        .expect("opening another note pushes")
        .to_str()
        .expect("text");
    assert!(
        pushed.starts_with("/notes?") && pushed.contains("path=b.md"),
        "{pushed}"
    );
    assert!(headers.get("hx-replace-url").is_none());

    let (_, headers, _) = send(
        &app,
        htmx_from(
            "GET",
            "/notes?path=b.md",
            "http://127.0.0.1:7700/notes?path=b.md",
            "",
        ),
    )
    .await;
    assert!(
        headers.get("hx-push-url").is_none(),
        "same note: no new entry"
    );
    assert!(
        headers
            .get("hx-replace-url")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|url| url.contains("path=b.md"))
    );

    let first = notes.read("a.md").expect("reads").value;
    notes.write("a.md", "theirs", None).expect("other writer");
    let (_, headers, body) = send(
        &app,
        htmx_from(
            "POST",
            "/notes/save",
            "http://127.0.0.1:7700/notes?path=a.md",
            &format!("path=a.md&text=mine&baseline_hash={}", first.hash),
        ),
    )
    .await;
    assert!(
        body.contains("theirs"),
        "the conflict form is drawn: {body}"
    );
    assert!(headers.get("hx-push-url").is_none());
    assert!(
        headers
            .get("hx-replace-url")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|url| url.contains("path=a.md"))
    );

    let (_, headers, _) = send(
        &app,
        htmx_from(
            "GET",
            "/notes?path=ghost.md",
            "http://127.0.0.1:7700/notes?path=a.md",
            "",
        ),
    )
    .await;
    assert!(
        headers.get("hx-push-url").is_none() && headers.get("hx-replace-url").is_none(),
        "an unknown note never reaches the address bar"
    );
}

#[tokio::test]
#[cfg(unix)]
async fn a_failed_daily_create_shows_its_error_and_an_existing_page_opens() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-daily-error");
    let store = sandbox.store();
    let notes = vault(&store);
    let root = store.home().directory().join("notes");
    let app = router(context(store));

    notes
        .write("daily/2026-11-14.md", "kept", None)
        .expect("the page exists");
    let (status, _, body) = send(&app, form_request("/notes/daily", "date=2026-11-14")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("shell__error"),
        "an existing page opens: {body}"
    );
    assert!(body.contains("kept"), "{body}");

    // A read-only notes directory: the new page cannot be created, and the
    // answer says why instead of opening a missing path.
    use std::os::unix::fs::PermissionsExt;
    std::fs::remove_dir_all(root.join("daily")).expect("the folder goes");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).expect("locked");
    let (status, _, body) = send(&app, form_request("/notes/daily", "date=2026-11-15")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<div class=\"banner\" role=\"alert\">"),
        "the failure reaches the human: {body}"
    );
    assert!(
        !body.contains("No note at"),
        "not the unknown-note banner: {body}"
    );
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).expect("unlocked");
}

#[tokio::test]
async fn only_today_s_note_form_carries_the_daily_hook_and_the_script_matches_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-daily-hook");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes")).await;
    let form_tag = |action: &str| -> String {
        let at = body
            .find(&format!("action=\"{action}\""))
            .unwrap_or_else(|| panic!("the {action} form is drawn: {body}"));
        let open = body[..at]
            .rfind("<form")
            .expect("the form opens before its action");
        let close = body[at..].find('>').expect("the form tag closes");
        body[open..at + close].to_string()
    };
    assert!(
        form_tag("/notes/daily").contains("data-daily"),
        "today's note form carries the hook"
    );
    assert!(
        !form_tag("/notes/new").contains("data-daily"),
        "the new note form does not take the daily date"
    );
    let (_, _, script) = get(&app, "/static/tandem.js").await;
    assert!(script.contains("hasAttribute('data-daily')"));
    assert!(!script.contains("contains('tree__daily')"));
}

#[tokio::test]
async fn post_notes_create_create_folder_and_move_update_vault_and_refuse_invalid_reparent() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-move");
    let store = sandbox.store();
    let notes = vault(&store);
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/create",
            "path=projects/tandem/New.md&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the create answers: {body}");
    assert!(
        notes.read("projects/tandem/New.md").is_ok(),
        "the note is created"
    );
    assert!(body.contains("New"), "the tree redraws the note: {body}");

    let (status, _, _) = send(
        &app,
        form_request(
            "/notes/create-folder",
            "path=projects/tandem/Sub&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        notes
            .file_url("projects/tandem/Sub")
            .is_some_and(|path| path.is_dir()),
        "the folder is created"
    );

    let (status, _, _) = send(
        &app,
        form_request(
            "/notes/move",
            "path=projects/tandem/New.md&into=projects/tandem/Sub&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        notes.read("projects/tandem/Sub/New.md").is_ok(),
        "the note moves into the folder"
    );

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/move",
            "path=projects/tandem&into=projects/tandem/Sub&project=tandem",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("cannot be moved into itself"),
        "a folder moved into its own descendant is refused: {body}"
    );
    assert!(
        notes.read("projects/tandem/Sub/New.md").is_ok(),
        "the refused move changes nothing"
    );
}

// MARK: - The sheet over a filtered board

/// A sheet opens over the board the human was looking at: the card link
/// carries its filters, and the sheet draws that board behind itself with
/// them, so the project, label, assignee and search survive the click.
#[tokio::test]
async fn get_ticket_sheet_keeps_the_board_filters_behind_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        sheet_get(&format!(
            "/tickets/{}?project=Alpha&show_all=&label=alpha&assignee=builder&text=Caf",
            ticket.id
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the sheet answers: {body}");
    assert!(
        body.contains("<option value=\"Alpha\" selected>Alpha</option>"),
        "the project filter sits behind the sheet as the human set it: {body}"
    );
    assert!(
        body.contains("name=\"board_text\" value=\"Caf\""),
        "the search sits behind the sheet: {body}"
    );
    assert!(
        body.contains("name=\"board_assignee\" value=\"builder\""),
        "the board's assignee rides the sheet under its own name: {body}"
    );
    assert!(
        body.contains("href=\"/board?project=Alpha"),
        "the close link lands on the filtered board: {body}"
    );
    assert!(
        !body.contains("Beta one"),
        "the board behind the sheet is the filtered one: {body}"
    );
}

/// The event names htmx reads from the board root's `hx-trigger`, one per
/// comma-separated spec. An empty list is not "no trigger": htmx's default for
/// a bare `hx-get` is `click`, so the caller asserts the list holds the change
/// stream and nothing else.
fn board_root_triggers(page: &str) -> Vec<String> {
    let start = page
        .find("<div id=\"board-root\"")
        .expect("the board root draws");
    let tag = &page[start..start + page[start..].find('>').expect("the root tag closes")];
    let Some(at) = tag.find("hx-trigger=\"") else {
        return Vec::new();
    };
    let rest = &tag[at + "hx-trigger=\"".len()..];
    let value = &rest[..rest.find('"').expect("the attribute closes")];
    value
        .split(',')
        .filter_map(|spec| {
            let spec = spec.trim();
            let head = &spec[..spec.find(['[', ' ']).unwrap_or(spec.len())];
            (!head.is_empty()).then(|| head.to_owned())
        })
        .collect()
}

/// The open sheet is the human's, not the store's: a write from another actor
/// must not replace the form under their cursor and clear a half-typed comment.
/// The pause is the absent stream on the sheet's page, not an absent trigger:
/// htmx's default for a root with no `hx-trigger` is `click`, which refetches
/// the board on every click inside the sheet and dismisses it.
#[tokio::test]
async fn a_click_inside_the_open_sheet_does_not_refetch_the_board() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-quiet");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));

    let (status, _, sheet) = send(
        &app,
        page_get(&format!("/tickets/{}?project=Fixture", ticket.id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the sheet answers: {sheet}");
    assert_eq!(
        board_root_triggers(&sheet),
        vec!["sse:changed".to_owned()],
        "the root always carries the change stream's trigger and no trigger a \
         click can fire, or htmx refetches the board on any click inside the \
         sheet: {sheet}"
    );
    assert!(
        !sheet.contains("sse-connect"),
        "an open sheet does not re-fetch on every store change: {sheet}"
    );

    // The sheet page the save answers with is the root fragment alone; it keeps
    // the same trigger, so the click that follows a save is inert too.
    let (status, _, partial) = send(&app, sheet_get(&format!("/tickets/{}", ticket.id))).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the sheet fragment answers: {partial}"
    );
    assert_eq!(
        board_root_triggers(&partial),
        vec!["sse:changed".to_owned()],
        "the write's fresh root carries the same trigger: {partial}"
    );

    let (status, _, board_page) = send(&app, page_get("/board?project=Fixture")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        board_root_triggers(&board_page),
        vec!["sse:changed".to_owned()],
        "the plain board's root carries the same trigger: {board_page}"
    );
    assert!(
        board_page.contains("sse-connect=\"/events\""),
        "the plain board follows the change stream: {board_page}"
    );

    let (status, _, notes) = send(&app, page_get("/notes")).await;
    assert_eq!(status, StatusCode::OK, "the notes face answers: {notes}");
    assert!(
        notes.contains("sse-connect=\"/events\""),
        "only the sheet's page pauses the stream: {notes}"
    );
}

/// A sheet reached by an htmx swap lands inside the body that is already on
/// screen: the body is not re-drawn, so its `sse-connect` stays and a store
/// change still swaps the root under the human's cursor. The pause is the
/// sheet's own page, so the card that opens it navigates and no template swaps
/// a sheet into a connected page.
#[tokio::test]
async fn a_sheet_is_opened_as_its_own_document_so_the_stream_pauses() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-document");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));

    let (status, _, page) = send(&app, page_get("/board")).await;
    assert_eq!(status, StatusCode::OK, "the board answers: {page}");
    assert!(
        page.contains("sse-connect=\"/events\""),
        "the board follows the change stream: {page}"
    );
    let card = card_tag(&page, &ticket.id.to_string());
    assert!(
        card.contains(&format!("href=\"/tickets/{}", ticket.id)),
        "the card links to its sheet: {card}"
    );
    assert!(
        !card.contains("hx-get") && !card.contains("hx-target"),
        "the card navigates: a swap would draw the sheet inside the body that \
         is already connected, so the sheet's page never pauses the stream: \
         {card}"
    );

    for (name, template) in [("board", BOARD_TEMPLATE), ("sheet", SHEET_TEMPLATE)] {
        assert!(
            !template.contains("hx-get=\"/tickets"),
            "the {name} template swaps no sheet into a connected page"
        );
    }
}

/// A tab is a face change, not a partial swap: the Board tab must not target a
/// root the Notes face does not draw, or the click is a resolved-to-nothing
/// target error.
#[tokio::test]
async fn the_shell_tabs_navigate_instead_of_targeting_a_face_they_are_not_drawn_on() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("shell-tabs");
    let app = router(context(sandbox.store()));

    let (status, _, page) = send(&app, page_get("/notes")).await;
    assert_eq!(status, StatusCode::OK, "the notes face answers: {page}");
    assert!(page.contains("class=\"shell__tabs\""), "the tabs draw");
    assert!(
        page.contains("href=\"/board\">Board</a>"),
        "the Board tab is a navigation link: {page}"
    );
    assert!(
        !page.contains("hx-get=\"/board\" hx-target=\"#board-root\""),
        "the Board tab does not target #board-root from the notes face: {page}"
    );
}

// MARK: - The application log

/// The text one captured subscriber wrote, shared between the test and the
/// subscriber's writer.
#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("the buffer locks")).into_owned()
    }

    /// A debug-level fmt subscriber on this thread. The guard keeps it in place
    /// across the awaits of a current-thread test.
    fn install(&self) -> tracing::subscriber::DefaultGuard {
        let writer = self.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        // Recompute the cached interest for this subscriber, so a callsite an
        // earlier test cached as off is not left off.
        tracing::callsite::rebuild_interest_cache();
        guard
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the buffer locks")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every line the subscriber wrote starts with its timestamp, so no value
/// carried a newline into a line of its own.
fn assert_every_line_is_a_timestamped_event(text: &str) {
    for line in text.lines().filter(|line| !line.is_empty()) {
        assert!(
            line.starts_with(|c: char| c.is_ascii_digit()),
            "a value broke a log line: {line:?}"
        );
    }
}

#[tokio::test]
async fn a_board_read_logs_one_request_line_by_route_and_status_and_never_the_query() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-board");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, _, _) = get(&app, "/board?text=secretword").await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(text.contains(" INFO "), "the request is info: {text}");
    assert!(
        text.contains("route=\"/board\""),
        "the route is logged: {text}"
    );
    assert!(text.contains("status=200"), "the status is logged: {text}");
    assert!(
        text.contains("latency_ms="),
        "the latency is logged: {text}"
    );
    assert!(
        !text.contains("secretword"),
        "the search text stays out: {text}"
    );
    assert!(!text.contains("text="), "no query string is logged: {text}");
    assert_every_line_is_a_timestamped_event(&text);
}

#[test]
fn a_request_over_the_threshold_warns_slow_and_one_under_it_does_not() {
    let captured = Captured::default();
    let _guard = captured.install();
    let limit = Duration::from_millis(500);

    log_request(
        &Method::GET,
        "/board",
        200,
        Duration::from_millis(100),
        limit,
    );
    assert!(
        !captured.text().contains("slow request"),
        "under: {}",
        captured.text()
    );

    log_request(
        &Method::POST,
        "/mcp",
        200,
        Duration::from_millis(900),
        limit,
    );
    let text = captured.text();
    assert!(text.contains("slow request"), "over: {text}");
    assert!(text.contains(" WARN "), "slow is a warn: {text}");
    assert!(
        text.contains("route=\"/mcp\"") && text.contains("latency_ms=900"),
        "{text}"
    );

    let before = captured.text().matches("slow request").count();
    log_request(&Method::GET, "/events", 200, Duration::from_secs(60), limit);
    log_request(&Method::GET, "/health", 200, Duration::from_secs(60), limit);
    assert_eq!(captured.text().matches("slow request").count(), before);
}

#[tokio::test]
async fn the_page_carries_the_release_version() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("page-version");
    let app = router(context(sandbox.store()));
    let (status, _, body) = get(&app, "/board").await;
    assert_eq!(status, StatusCode::OK);
    let want = format!("data-version=\"{}\"", crate::release_version());
    assert!(body.contains(&want), "the shell names the build: {body}");
}

#[test]
fn the_source_link_names_the_tag_on_a_release_and_the_root_on_a_dev_build() {
    assert_eq!(
        super::source_url("https://example.test/tandem", Some("2.9.1")),
        "https://example.test/tandem/tree/v2.9.1"
    );
    assert_eq!(
        super::source_url("https://example.test/tandem", None),
        "https://example.test/tandem"
    );
    assert_eq!(
        super::source_url("https://example.test/tandem/", None),
        "https://example.test/tandem",
        "a trailing slash does not double"
    );
}

#[tokio::test]
async fn the_board_footer_links_the_source_and_the_sign_in_page_does_not() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("page-footer");
    let app = router(context(sandbox.store()));
    let (status, _, body) = get(&app, "/board").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<footer class=\"shell__footer\">"),
        "the shell carries a footer"
    );
    let href = format!(
        "href=\"{}\"",
        super::source_url(env!("CARGO_PKG_REPOSITORY"), option_env!("TANDEM_VERSION"))
    );
    assert!(body.contains(&href), "the footer links the source: {body}");
    assert_eq!(
        body.matches("class=\"shell__source\"").count(),
        1,
        "one source link"
    );
    let request = Request::builder()
        .uri("/board")
        .header(header::HOST, "127.0.0.1:7700")
        .header("hx-request", "true")
        .body(Body::empty())
        .expect("the request builds");
    let (_, _, partial) = send(&app, request).await;
    assert!(
        !partial.contains("shell__footer"),
        "a partial never repeats the footer"
    );
}

#[tokio::test]
async fn a_static_asset_logs_at_debug_under_its_route_template() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-static");
    let store = sandbox.store();
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, _, _) = get(&app, "/static/app.css").await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(text.contains(" DEBUG "), "a static asset is debug: {text}");
    assert!(
        text.contains("route=\"/static/{*path}\""),
        "the route is the template: {text}"
    );
    assert!(
        !text.contains(" INFO "),
        "a static asset is not info: {text}"
    );
}

#[tokio::test]
async fn a_ticket_add_logs_identifiers_and_never_the_title() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-add");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let before = store.revision().expect("the revision reads");
    let captured = Captured::default();
    let _guard = captured.install();

    let body = format!(
        "title=SecretTitleWords&status=ready&ticket_project=Alpha&ticket_label=&expected_revision={before}&project=&show_all=&assignee=&text="
    );
    let (status, _, _) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK);
    let added = by_title(&board, "SecretTitleWords");
    let text = captured.text();
    assert!(text.contains("ticket added"), "the add is an event: {text}");
    assert!(
        text.contains(&format!("id={:?}", added.id)),
        "the ticket id is logged: {text}"
    );
    assert!(
        !text.contains("SecretTitleWords"),
        "the title stays out: {text}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[tokio::test]
async fn a_card_move_logs_the_status_it_came_from_and_the_one_it_went_to() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-move");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let before = store.revision().expect("the revision reads");
    let ticket = by_title(&board, "Café alpha");
    let captured = Captured::default();
    let _guard = captured.install();

    let body = format!("status=done&expected_revision={before}&project=&show_all=&assignee=&text=");
    let uri = format!("/tickets/{}/status", ticket.id);
    let (status, _, _) = send(&app, form_request(&uri, &body)).await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(
        text.contains("ticket moved"),
        "the move is an event: {text}"
    );
    assert!(
        text.contains("ready"),
        "the origin status is logged: {text}"
    );
    assert!(
        text.contains("done"),
        "the destination status is logged: {text}"
    );
}

#[tokio::test]
async fn a_stale_revision_logs_a_warning_with_the_refusal() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-stale");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store.clone()));
    let stale = store.revision().expect("the revision reads") - 1;
    let captured = Captured::default();
    let _guard = captured.install();

    let body = format!(
        "title=Late&status=backlog&label=&expected_revision={stale}&project=&show_all=&assignee=&text="
    );
    let (status, _, _) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "a conflict still answers the board");
    let text = captured.text();
    assert!(text.contains(" WARN "), "the conflict is a warning: {text}");
    assert!(
        text.contains("changed while you were writing"),
        "the refusal's message is logged: {text}"
    );
    assert!(!text.contains("Late"), "the stale title stays out: {text}");
}

#[tokio::test]
async fn the_write_guard_logs_its_reason_and_never_the_origin_value() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-guard");
    let store = sandbox.store();
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let json = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .expect("the request builds");
    let (status, _, _) = send(&app, json).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let cross = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from("title=x"))
        .expect("the request builds");
    let (status, _, _) = send(&app, cross).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let text = captured.text();
    assert!(
        text.contains("content type is not form-encoded"),
        "the 415 logs its reason: {text}"
    );
    assert!(
        text.contains("came from another origin"),
        "the 403 logs its reason: {text}"
    );
    assert!(
        !text.contains("evil.example"),
        "the origin value stays out: {text}"
    );
}

#[tokio::test]
async fn a_control_character_in_a_user_value_stays_on_one_escaped_line() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-escape");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store.clone()));
    let captured = Captured::default();
    let _guard = captured.install();

    // A label name carrying a newline and an ESC: the log must escape both.
    let (status, _, _) = send(
        &app,
        form_request(
            "/labels",
            "name=a%0Ab%1B%5B31m&project=Alpha&show_all=&assignee=&text=",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(
        !text.contains('\u{1b}'),
        "no raw ESC reaches the log: {text:?}"
    );
    assert!(
        text.contains(r#"label="a\nb\u{1b}[31m""#),
        "the label field carries the escaped name on its line: {text:?}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[tokio::test]
async fn a_store_failure_answers_500_and_logs_one_error_line_with_the_route() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-500");
    let store = sandbox.store();
    setup_board(&store);
    // Overwrite the store with bytes that are not a database: every read fails.
    let home = sandbox.root.join("home");
    for sidecar in ["tandem.db-wal", "tandem.db-shm"] {
        let _ = std::fs::remove_file(home.join(sidecar));
    }
    std::fs::write(home.join("tandem.db"), vec![b'x'; 4096]).expect("the store is overwritten");
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, _, _) = get(&app, "/board").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let text = captured.text();
    let errors: Vec<&str> = text
        .lines()
        .filter(|line| line.contains(" ERROR ") && line.contains("route=\"/board\""))
        .collect();
    assert_eq!(errors.len(), 1, "one error line carries the route: {text}");
    assert!(
        errors[0].contains("status=500"),
        "the error line carries the status: {text}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[tokio::test]
async fn a_note_save_logs_its_path_at_info_and_never_the_body() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-save");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes
        .write("Index.md", "one", None)
        .expect("the note is written");
    let first = notes.read("Index.md").expect("the note reads").value;
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, _, body) = send(
        &app,
        form_request(
            "/notes/save",
            &format!(
                "path=Index.md&text=SecretBodyWords&baseline_hash={}",
                first.hash
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the save answers: {body}");
    let text = captured.text();
    assert!(
        text.lines().any(|line| line.contains(" INFO ")
            && line.contains("note saved")
            && line.contains("path=\"Index.md\"")),
        "the save logs its path at info: {text}"
    );
    assert!(
        !text.contains("SecretBodyWords"),
        "the note body stays out: {text}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "Windows forbids a newline or ESC in a file name; the label-name escape test covers the escape on every platform"
)]
async fn a_note_path_with_control_characters_stays_on_one_escaped_line() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-path-escape");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    // A note path carrying a newline and an ESC: the log must escape both.
    let (status, _, _) = send(
        &app,
        form_request("/notes/create", "path=a%0Ab%1B%5B31m.md"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(
        !text.contains('\u{1b}'),
        "no raw ESC reaches the log: {text:?}"
    );
    let created: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("note created"))
        .collect();
    assert_eq!(created.len(), 1, "the path is one line: {text:?}");
    assert!(
        created[0].contains(r#"path="a\nb\u{1b}[31m.md""#),
        "the path is logged escaped: {text:?}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[tokio::test]
async fn the_environment_is_never_logged() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("log-env");
    let store = sandbox.store();
    setup_board(&store);
    let secret = "tandem-secret-7f3a9c";
    // The context carries the run's environment, as `tandem serve` does.
    let mut environment = HashMap::new();
    environment.insert("TANDEM_TEST_SECRET".to_string(), secret.to_string());
    let app = router(SessionContext {
        store: store.clone(),
        actor: Actor::Human,
        environment,
        access: Access::loopback(7700),
        export_check: ExportCheck::standard(),
    });
    let before = store.revision().expect("the revision reads");
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, _, _) = get(&app, "/board").await;
    assert_eq!(status, StatusCode::OK);
    let body = format!(
        "title=EnvProbe&status=ready&label=&expected_revision={before}&project=&show_all=&assignee=&text="
    );
    let (status, _, _) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = get(&app, "/notes").await;
    assert_eq!(status, StatusCode::OK);
    let text = captured.text();
    assert!(!text.contains(secret), "the value stays out: {text}");
    assert!(
        !text.contains("TANDEM_TEST_SECRET"),
        "the variable name stays out: {text}"
    );
    assert_every_line_is_a_timestamped_event(&text);
}

#[test]
fn the_watch_logs_the_attach_of_a_store_created_after_it_started() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let root = std::env::temp_dir().join(format!(
        "tandem-serve-watch-log-late-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
    let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
    let store = Store::new(home);
    let captured = Captured::default();
    let _guard = captured.install();
    let stop = Arc::new(AtomicBool::new(false));
    let (changes, _receiver) = broadcast::channel(8);
    let handle = watch(store.clone(), changes, Arc::clone(&stop));

    // No store yet: several ticks pass and the watch logs nothing.
    thread::sleep(Duration::from_millis(300));
    assert!(
        captured.text().is_empty(),
        "a home with no store logs nothing: {}",
        captured.text()
    );

    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "first",
                key: "",
                last_number: 0,
                position: 0,
            })
        })
        .expect("the first write creates the store");
    let attached = Instant::now() + Duration::from_secs(10);
    while !captured.text().contains("store watch attached") && Instant::now() < attached {
        thread::sleep(Duration::from_millis(5));
    }
    // Several ticks pass with the store still: only the transition is logged.
    thread::sleep(Duration::from_millis(300));
    let text = captured.text();
    assert_eq!(
        text.matches("store watch").count(),
        1,
        "the attach logs once and the ticks log nothing: {text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains(" INFO ") && line.contains("store watch attached")),
        "the attach logs at info: {text}"
    );

    stop.store(true, Ordering::Relaxed);
    handle.join().expect("the watch exits");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_watch_logs_its_attach_and_a_broadcast_and_no_tick() {
    let _serial = ONE_AT_A_TIME.blocking_lock();
    let captured = Captured::default();
    let _guard = captured.install();
    let (root, store) = store_with_a_revision("log-attach");
    let stop = Arc::new(AtomicBool::new(false));
    let (changes, _receiver) = broadcast::channel(8);
    let handle = watch(store.clone(), changes, Arc::clone(&stop));

    let attached = Instant::now() + Duration::from_secs(10);
    while !captured.text().contains("store watch attached") && Instant::now() < attached {
        thread::sleep(Duration::from_millis(5));
    }
    // Several ticks pass with the store still: an idle watch logs nothing.
    thread::sleep(Duration::from_millis(300));
    let idle = captured.text();
    assert_eq!(
        idle.matches("store watch").count(),
        1,
        "the attach logs once and the ticks log nothing: {idle}"
    );
    assert!(
        !idle.contains("store revision broadcast"),
        "no tick broadcasts: {idle}"
    );

    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "second",
                key: "",
                last_number: 0,
                position: 1,
            })
        })
        .expect("the second write lands");
    let moved = Instant::now() + Duration::from_secs(10);
    while !captured.text().contains("store revision broadcast") && Instant::now() < moved {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        captured.text().contains("store revision broadcast"),
        "a revision that moves is broadcast at debug: {}",
        captured.text()
    );

    stop.store(true, Ordering::Relaxed);
    handle.join().expect("the watch exits");
    let _ = std::fs::remove_dir_all(root);
}

// MARK: - POST /mcp, the guard's two policies and the request's actor (ADR 0047)

const MCP_BODY: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#;

/// A `/mcp` request that passes every check: loopback Host, JSON, no Origin.
fn mcp_builder() -> axum::http::request::Builder {
    Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/json")
}

fn mcp_request(body: &str) -> Request<Body> {
    mcp_builder()
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

fn tool_call_body(name: &str, arguments: serde_json::Value) -> String {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    })
    .to_string()
}

fn author_of_project(store: &Store, name: &str) -> String {
    let connection = rusqlite::Connection::open(store.home().directory().join("tandem.db"))
        .expect("the store opens");
    connection
        .query_row(
            "SELECT author FROM projects WHERE name = ?1",
            [name],
            |row| row.get::<_, String>(0),
        )
        .expect("the project row exists")
}

#[tokio::test]
async fn mcp_answers_initialize_and_tools_list_over_json() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-json");
    let app = router(context(sandbox.store()));

    let (status, headers, body) = send(&app, mcp_request(MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK, "initialize answers: {body}");
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    assert!(
        headers.get("mcp-session-id").is_none(),
        "the door is stateless: no session id is issued"
    );
    let answer: serde_json::Value = serde_json::from_str(&body).expect("one JSON frame");
    assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");

    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let (status, _, body) = send(&app, mcp_request(list)).await;
    assert_eq!(status, StatusCode::OK);
    let answer: serde_json::Value = serde_json::from_str(&body).expect("one JSON frame");
    let tools = answer["result"]["tools"].as_array().expect("a tool list");
    assert!(
        tools.iter().any(|tool| tool["name"] == "ticket_list"),
        "the list names the tools"
    );
}

#[tokio::test]
async fn a_rebinding_host_is_refused_on_mcp_and_on_a_form_route() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-rebinding");
    let app = router(context(sandbox.store()));

    let (status, _, _) = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "evil.example:7700")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(MCP_BODY))
            .expect("the request builds"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a rebinding Host is refused");

    let on_form = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "evil.example:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("title=Rebound"))
        .expect("the request builds");
    let (status, _, _) = send(&app, on_form).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a form route is guarded too");

    let on_read = Request::builder()
        .uri("/board")
        .header(header::HOST, "evil.example:7700")
        .body(Body::empty())
        .expect("the request builds");
    let (status, _, _) = send(&app, on_read).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a read is not exempt");

    let missing = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, missing).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no Host is refused");

    for host in ["localhost:7700", "[::1]:7700", "LOCALHOST:7700"] {
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(MCP_BODY))
            .expect("the request builds");
        let (status, _, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::OK, "{host} is a loopback authority");
    }
    let wrong_port = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1:9999")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, wrong_port).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the port is the bound one");
}

#[tokio::test]
async fn a_cross_origin_mcp_request_with_a_valid_host_is_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-origin");
    let app = router(context(sandbox.store()));

    for origin in [
        "http://evil.example",
        "https://127.0.0.1:7700",
        "null",
        "http://127.0.0.1:7701",
    ] {
        let request = mcp_builder()
            .header(header::ORIGIN, origin)
            .body(Body::from(MCP_BODY))
            .expect("the request builds");
        let (status, headers, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin} is refused");
        assert!(
            headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(),
            "no CORS header is ever answered"
        );
    }
    let same = mcp_builder()
        .header(header::ORIGIN, "http://localhost:7700")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, same).await;
    assert_eq!(status, StatusCode::OK, "a loopback Origin passes");

    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::empty())
        .expect("the request builds");
    let (status, headers, _) = send(&app, preflight).await;
    assert_eq!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "no permissive OPTIONS"
    );
    assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn the_content_type_is_a_media_type_not_a_prefix() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-content-type");
    let app = router(context(sandbox.store()));

    for content_type in [
        "application/x-www-form-urlencoded",
        "application/jsonx",
        "text/plain",
    ] {
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:7700")
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(MCP_BODY))
            .expect("the request builds");
        let (status, _, _) = send(&app, request).await;
        assert_eq!(
            status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{content_type} is not application/json"
        );
    }
    let with_parameter = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, with_parameter).await;
    assert_eq!(status, StatusCode::OK, "parameters are allowed");

    let json_on_a_form_route = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Nope"}"#))
        .expect("the request builds");
    let (status, _, _) = send(&app, json_on_a_form_route).await;
    assert_eq!(
        status,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "no UI route accepts JSON"
    );
}

#[tokio::test]
async fn mcp_answers_a_notification_202_and_the_other_methods_405() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-methods");
    let app = router(context(sandbox.store()));

    let notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let (status, _, body) = send(&app, mcp_request(notification)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "a notification is 202");
    assert!(body.is_empty(), "with no body");

    for method in ["GET", "DELETE"] {
        let request = Request::builder()
            .method(method)
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:7700")
            .body(Body::empty())
            .expect("the request builds");
        let (status, _, _) = send(&app, request).await;
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} /mcp is 405"
        );
    }

    let batch = r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#;
    let (status, _, body) = send(&app, mcp_request(batch)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("batches are not supported"), "{body}");
}

#[tokio::test]
async fn an_unknown_protocol_version_header_is_400_and_an_absent_one_is_accepted() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-version");
    let app = router(context(sandbox.store()));

    let unknown = mcp_builder()
        .header("mcp-protocol-version", "1999-01-01")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, unknown).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let known = mcp_builder()
        .header("mcp-protocol-version", "2025-06-18")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, known).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send(&app, mcp_request(MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK, "an absent header is accepted");
}

#[tokio::test]
async fn the_actor_header_lands_on_the_mutation_row_and_none_records_the_server_actor() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-actor");
    let store = sandbox.store();
    let app = router(context(store.clone()));

    let named = mcp_builder()
        .header("x-tandem-agent", "builder")
        .body(Body::from(tool_call_body(
            "project_add",
            serde_json::json!({"name": "by-header"}),
        )))
        .expect("the request builds");
    let (status, _, body) = send(&app, named).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("isError"), "the write succeeds: {body}");
    assert_eq!(author_of_project(&store, "by-header"), "builder");

    let human = mcp_builder()
        .header("x-tandem-agent", "human")
        .body(Body::from(tool_call_body(
            "project_add",
            serde_json::json!({"name": "by-human"}),
        )))
        .expect("the request builds");
    send(&app, human).await;
    assert_eq!(author_of_project(&store, "by-human"), "human");

    let server = SessionContext {
        actor: Actor::Agent("serve-runner".to_string()),
        ..context(store.clone())
    };
    let app = router(server);
    let unnamed = mcp_request(&tool_call_body(
        "project_add",
        serde_json::json!({"name": "by-server"}),
    ));
    send(&app, unnamed).await;
    assert_eq!(
        author_of_project(&store, "by-server"),
        "serve-runner",
        "no header records the server's own actor"
    );
}

#[tokio::test]
async fn a_bad_actor_header_is_400_and_never_trimmed_into_a_name() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-bad-actor");
    let store = sandbox.store();
    let app = router(context(store.clone()));

    let too_long = "a".repeat(65);
    for value in [
        "Bad Name",
        "Builder",
        " builder",
        "",
        "bu_ilder",
        too_long.as_str(),
    ] {
        let request = mcp_builder()
            .header("x-tandem-agent", value)
            .body(Body::from(MCP_BODY))
            .expect("the request builds");
        let (status, _, body) = send(&app, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value:?} is refused");
        assert!(
            !body.contains(value) || value.is_empty(),
            "the value is not echoed"
        );
    }
}

#[tokio::test]
async fn a_request_header_never_reaches_the_environment() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-environment");
    let store = sandbox.store();
    let home = store.home().directory().to_path_buf();
    let app = router(context(store));

    let request = mcp_builder()
        .header("tandem-home", "/tmp/elsewhere")
        .header("x-tandem-home", "/tmp/elsewhere")
        .body(Body::from(tool_call_body(
            "doctor_report",
            serde_json::json!({}),
        )))
        .expect("the request builds");
    let (_, _, body) = send(&app, request).await;
    assert!(!body.contains("/tmp/elsewhere"), "{body}");
    assert!(
        body.contains(&home.display().to_string().replace('\\', "\\\\")) || body.contains("home"),
        "the report names the server's own home"
    );
}

#[tokio::test]
async fn health_is_a_fixed_body_with_no_auth() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("health");
    let app = router(context(sandbox.store()));

    let (status, _, body) = get(&app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok\n", "nothing but ok: no version, path or actor");
}

fn remote_app(store: Store, token: &str) -> Router {
    router(SessionContext {
        access: Access::token(7700, token),
        ..context(store)
    })
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn off_loopback_every_route_but_health_wants_the_token() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("token");
    let app = remote_app(sandbox.store(), TOKEN);

    let attempts: [(&str, &str, &str); 5] = [
        ("GET", "/", ""),
        ("GET", "/board", ""),
        ("GET", "/events", ""),
        ("POST", "/tickets", "application/x-www-form-urlencoded"),
        ("POST", "/mcp", "application/json"),
    ];
    let wrong = format!("Bearer {}x", TOKEN);
    let authorizations: [Option<&str>; 5] = [
        None,
        Some("Bearer wrong-token-wrong-token-wrong-tok"),
        Some(wrong.as_str()),
        Some(TOKEN),
        Some("Basic dXNlcjpwYXNz"),
    ];
    for (method, uri, content_type) in attempts {
        for authorization in authorizations {
            let mut builder = Request::builder()
                .method(method)
                .uri(uri)
                .header(header::HOST, "board.example");
            if !content_type.is_empty() {
                builder = builder.header(header::CONTENT_TYPE, content_type);
            }
            if let Some(value) = authorization {
                builder = builder.header(header::AUTHORIZATION, value);
            }
            let request = builder.body(Body::from("")).expect("the request builds");
            let (status, headers, body) = send(&app, request).await;
            if method == "GET" && uri != "/events" {
                // A page load with no credential goes to the login page.
                assert_eq!(
                    status,
                    StatusCode::SEE_OTHER,
                    "{method} {uri} with {authorization:?} is sent to sign in"
                );
                assert!(
                    headers
                        .get(header::LOCATION)
                        .and_then(|value| value.to_str().ok())
                        .is_some_and(|location| location.starts_with("/login")),
                    "{headers:?}"
                );
                continue;
            }
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} with {authorization:?} is one 401"
            );
            assert_eq!(body, "unauthorized\n", "one failure path");
            assert!(headers.get(header::WWW_AUTHENTICATE).is_none());
        }
    }

    let (status, _, body) = send(
        &app,
        Request::builder()
            .uri("/health")
            .header(header::HOST, "board.example")
            .body(Body::empty())
            .expect("the request builds"),
    )
    .await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "ok\n"));

    let board = Request::builder()
        .uri("/board")
        .header(header::HOST, "board.example")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .expect("the request builds");
    let (status, _, _) = send(&app, board).await;
    assert_eq!(status, StatusCode::OK, "the token opens the UI");

    let mcp = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "board.example")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header("x-tandem-agent", "reviewer")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, mcp).await;
    assert_eq!(status, StatusCode::OK, "the token opens /mcp");
}

#[test]
fn the_token_never_reaches_a_debug_print() {
    let access = Access::token(7700, TOKEN);
    let printed = format!("{access:?}");
    assert!(!printed.contains(TOKEN), "{printed}");
    assert!(printed.contains("redacted"), "{printed}");
}

#[tokio::test]
async fn note_read_with_a_parent_path_is_refused_through_mcp() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-note-path");
    let app = router(context(sandbox.store()));

    for path in ["../tandem.db", "/etc/passwd", "a/../../x"] {
        let request = mcp_request(&tool_call_body(
            "note_read",
            serde_json::json!({"path": path}),
        ));
        let (status, _, body) = send(&app, request).await;
        assert_eq!(status, StatusCode::OK);
        let answer: serde_json::Value = serde_json::from_str(&body).expect("one frame");
        assert_eq!(
            answer["result"]["isError"], true,
            "{path} is refused through /mcp: {body}"
        );
    }
}

#[tokio::test]
async fn the_by_construction_layer_covers_mcp_and_health() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("guard-layer-mcp");
    let app = extra_router(routes(), sandbox.store());

    let rebound = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "evil.example:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    let (status, _, _) = send(&app, rebound).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "/mcp crosses the one layer");

    let (status, _, _) = send(
        &app,
        Request::builder()
            .uri("/health")
            .header(header::HOST, "evil.example:7700")
            .body(Body::empty())
            .expect("the request builds"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "/health crosses it too");
}

#[tokio::test]
async fn an_opted_in_non_loopback_bind_admits_any_host_without_a_token() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("open-bind");
    let app = router(SessionContext {
        access: Access::open(7700),
        ..context(sandbox.store())
    });

    let board = Request::builder()
        .uri("/board")
        .header(header::HOST, "machine.tailnet.ts.net:7700")
        .body(Body::empty())
        .expect("the request builds");
    assert_eq!(send(&app, board).await.0, StatusCode::OK);

    let mcp = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "machine.tailnet.ts.net:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    assert_eq!(send(&app, mcp).await.0, StatusCode::OK);

    let foreign = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "machine.tailnet.ts.net:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, "http://evil.example")
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    assert_eq!(
        send(&app, foreign).await.0,
        StatusCode::FORBIDDEN,
        "the Origin check holds on an open bind"
    );
}

// MARK: - The browser sign-in (CUT-34, ADR 0049)

const REMOTE_HOST: &str = "board.example";
const REMOTE_ORIGIN: &str = "https://board.example";

/// The `name=value` pair of the session cookie a response sets, if it sets one.
fn session_cookie_pair(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(guard::SESSION_COOKIE))
        .and_then(|value| value.split(';').next())
        .map(str::to_string)
}

fn set_cookie_line(headers: &HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(guard::SESSION_COOKIE))
        .unwrap_or("")
        .to_string()
}

fn remote_get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(uri)
        .header(header::HOST, REMOTE_HOST);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).expect("the request builds")
}

fn remote_form(uri: &str, body: &str, origin: Option<&str>, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::HOST, REMOTE_HOST)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

fn login_body(token: &str, next: &str) -> String {
    serde_urlencoded::to_string([("username", "tandem"), ("password", token), ("next", next)])
        .expect("the form encodes")
}

/// Sign in with the right token and return the session cookie's pair.
async fn sign_in(app: &Router) -> String {
    let (status, headers, _) = send(
        app,
        remote_form(
            "/login",
            &login_body(TOKEN, "/board"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    session_cookie_pair(&headers).expect("a right token sets the cookie")
}

#[tokio::test]
async fn the_login_page_and_static_assets_open_without_the_token_and_nothing_else_does() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-open");
    let app = remote_app(sandbox.store(), TOKEN);

    let (status, _, body) = send(&app, remote_get("/login", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("type=\"password\""), "{body}");
    assert!(body.contains("autocomplete=\"current-password\""), "{body}");
    assert!(body.contains("autofocus"), "{body}");
    assert!(!body.contains("maxlength"), "no length cap on the field");
    assert!(
        body.contains("name=\"username\""),
        "a field a manager can fill"
    );
    assert!(
        body.contains("rel=\"icon\" href=\"/static/icon.svg?v="),
        "the sign-in page links the icon"
    );

    let css = Request::builder()
        .uri("/static/app.css")
        .header(header::HOST, REMOTE_HOST)
        .body(Body::empty())
        .expect("the request builds");
    assert_eq!(send(&app, css).await.0, StatusCode::OK, "the page's CSS");

    for uri in ["/board", "/notes", "/tickets/x", "/mcp"] {
        let (status, _, _) = send(&app, remote_get(uri, None)).await;
        assert!(
            matches!(status, StatusCode::SEE_OTHER | StatusCode::UNAUTHORIZED),
            "{uri} stays behind the token: {status}"
        );
    }
    let (status, _, _) = send(&app, remote_form("/logout", "", Some(REMOTE_ORIGIN), None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "sign out needs a session");
    let post_static = Request::builder()
        .method("POST")
        .uri("/static/app.css")
        .header(header::HOST, REMOTE_HOST)
        .body(Body::empty())
        .expect("the request builds");
    assert_ne!(send(&app, post_static).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_right_token_at_login_sets_the_session_cookie_with_its_attributes() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-cookie");
    let app = remote_app(sandbox.store(), TOKEN);

    // Trimmed: a phone paste carries a trailing space.
    let padded = format!(" {TOKEN} ");
    let (status, headers, _) = send(
        &app,
        remote_form(
            "/login",
            &login_body(&padded, "/notes"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers[header::LOCATION],
        "/login?signed-in=1&next=%2Fnotes",
        "a sign-in goes on through the cookie check"
    );
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let line = set_cookie_line(&headers);
    assert!(line.starts_with("__Host-tandem_session="), "{line}");
    for attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=2592000",
    ] {
        assert!(line.contains(attribute), "{attribute} in {line}");
    }
    assert!(!line.contains("Domain"), "{line}");
    let value = line
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value)
        .expect("a value");
    assert_eq!(value.len(), 64, "32 bytes in hex: {line}");
}

#[tokio::test]
async fn the_cookie_check_goes_on_to_the_target_when_the_browser_kept_the_session() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-cookie-kept");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let (status, headers, _) = send(
        &app,
        remote_get("/login?signed-in=1&next=%2Fnotes", Some(&cookie)),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/notes");
}

#[tokio::test]
async fn the_cookie_check_says_so_when_the_browser_dropped_the_session() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-cookie-dropped");
    let app = remote_app(sandbox.store(), TOKEN);

    // A browser on plain http at an address that is not localhost drops the
    // `Secure` cookie, so it comes back to the check with no session.
    let (status, headers, body) =
        send(&app, remote_get("/login?signed-in=1&next=%2Fnotes", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(body.contains("did not keep the sign-in"), "{body}");
    assert!(body.contains("localhost"), "{body}");
    assert!(body.contains("role=\"alert\""), "{body}");
    assert!(
        body.contains("value=\"/notes\""),
        "the return target is kept"
    );

    // The plain login page says nothing of it.
    let (_, _, plain) = send(&app, remote_get("/login", None)).await;
    assert!(!plain.contains("did not keep the sign-in"), "{plain}");
}

#[tokio::test]
async fn a_wrong_token_at_login_answers_the_generic_page_and_sets_no_cookie() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-wrong");
    let app = remote_app(sandbox.store(), TOKEN);

    let (status, headers, body) = send(
        &app,
        remote_form(
            "/login",
            &login_body("not-the-token", "/notes"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(session_cookie_pair(&headers).is_none());
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(body.contains("role=\"alert\""), "{body}");
    assert!(body.contains("aria-describedby=\"login-error\""), "{body}");
    assert!(
        body.contains("value=\"/notes\""),
        "the return target is kept"
    );
    assert!(!body.contains("not-the-token"), "the attempt is not echoed");
}

#[tokio::test]
async fn a_login_post_without_a_matching_origin_is_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-origin");
    let app = remote_app(sandbox.store(), TOKEN);
    let body = login_body(TOKEN, "/");

    for origin in [None, Some("https://evil.example"), Some("null")] {
        let (status, headers, _) = send(&app, remote_form("/login", &body, origin, None)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "origin {origin:?}");
        assert!(session_cookie_pair(&headers).is_none());
    }
}

#[tokio::test]
async fn the_session_cookie_opens_the_board_and_the_events_stream() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("cookie-opens");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let (status, headers, body) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Sign out"), "{body}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");

    let response = app
        .clone()
        .oneshot(remote_get("/events", Some(&cookie)))
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    let frame = stream.frame().await.expect("a frame").expect("readable");
    let text = String::from_utf8_lossy(frame.data_ref().expect("data")).to_string();
    assert!(text.contains("event: changed"), "{text}");
}

#[tokio::test]
async fn the_session_cookie_never_opens_mcp() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("cookie-mcp");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, REMOTE_HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &cookie)
        .body(Body::from(MCP_BODY))
        .expect("the request builds");
    assert_eq!(send(&app, request).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_cookie_write_needs_a_present_and_matching_origin_and_a_bearer_write_does_not() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("cookie-origin");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;
    let write = "title=From+a+browser";

    for origin in [None, Some("https://evil.example"), Some("null")] {
        let (status, _, _) =
            send(&app, remote_form("/tickets", write, origin, Some(&cookie))).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "cookie write, origin {origin:?}"
        );
    }
    let (status, _, _) = send(
        &app,
        remote_form("/tickets", write, Some(REMOTE_ORIGIN), Some(&cookie)),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "a matching Origin passes");
    assert_ne!(status, StatusCode::UNAUTHORIZED);

    // A bearer write keeps today's rule: an absent Origin passes.
    let mut bearer = remote_form("/tickets", write, None, None);
    bearer.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {TOKEN}").parse().expect("a header value"),
    );
    let (status, _, _) = send(&app, bearer).await;
    assert_ne!(status, StatusCode::FORBIDDEN);
    assert_ne!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_signed_out_page_load_redirects_to_login_and_a_background_request_gets_401() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("signed-out");
    let app = remote_app(sandbox.store(), TOKEN);

    let (status, headers, _) = send(&app, remote_get("/notes?path=a%20b.md", None)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers[header::LOCATION],
        "/login?next=%2Fnotes%3Fpath%3Da%2520b.md"
    );

    let (status, headers, _) = send(&app, remote_get("/", None)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/login");

    let (status, _, _) = send(&app, remote_get("/events", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let mut htmx = remote_get("/board", None);
    htmx.headers_mut()
        .insert("hx-request", "true".parse().expect("value"));
    assert_eq!(send(&app, htmx).await.0, StatusCode::UNAUTHORIZED);

    let (status, _, _) = send(
        &app,
        remote_form(
            "/tickets",
            "title=x",
            Some(REMOTE_ORIGIN),
            Some("__Host-tandem_session=00"),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a bogus cookie is no cookie"
    );
}

#[tokio::test]
async fn the_return_target_is_a_same_origin_path_or_the_root() {
    let hostile = [
        "//evil.example",
        "/\\evil.example",
        "https://evil.example/",
        "evil.example",
        "",
        "/ok\r\nSet-Cookie: x=1",
        "/ok\u{7}",
        "javascript:alert(1)",
        "/login",
        "/logout",
    ];
    for value in hostile {
        assert_eq!(guard::return_target(value), "/", "{value:?}");
    }
    for value in [
        "/",
        "/board?label=a",
        "/notes?path=x%2Fy.md",
        "/tickets/abc",
    ] {
        assert_eq!(guard::return_target(value), value);
    }

    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("return-target");
    let app = remote_app(sandbox.store(), TOKEN);
    let (_, headers, _) = send(
        &app,
        remote_form(
            "/login",
            &login_body(TOKEN, "//evil.example"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(
        headers[header::LOCATION],
        "/login?signed-in=1&next=%2F",
        "a hostile target lands on the root, through the cookie check"
    );
    // The check reads its target through the same guard.
    let cookie = session_cookie_pair(&headers).expect("a right token sets the cookie");
    let (_, headers, _) = send(
        &app,
        remote_get("/login?signed-in=1&next=%2F%2Fevil.example", Some(&cookie)),
    )
    .await;
    assert_eq!(headers[header::LOCATION], "/");
}

#[tokio::test]
async fn sign_out_ends_the_session_on_the_server() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sign-out");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let (status, headers, _) = send(
        &app,
        remote_form("/logout", "", Some(REMOTE_ORIGIN), Some(&cookie)),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/login?notice=signed-out");
    let line = set_cookie_line(&headers);
    assert!(line.contains("Max-Age=0"), "the cookie is cleared: {line}");

    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the old cookie is refused");

    let (status, _, body) = send(&app, remote_get("/login?notice=signed-out", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("You are signed out."), "{body}");
}

#[tokio::test]
async fn a_session_from_another_token_is_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("other-token");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let rotated = remote_app(sandbox.store(), "a-different-token-a-different-token");
    let (status, _, _) = send(&rotated, remote_get("/board", Some(&cookie))).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "a rotated token orphans every row"
    );
}

#[tokio::test]
async fn a_session_survives_a_new_router_over_the_same_home() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restart");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;
    drop(app);

    let restarted = remote_app(sandbox.store(), TOKEN);
    let (status, _, _) = send(&restarted, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK, "a restart leaves the tab signed in");
}

/// Rewrite every session's last use, the way time passing would.
fn age_sessions(sandbox: &Sandbox, last_used_at: i64) {
    let connection = rusqlite::Connection::open(sandbox.store().database()).expect("opens");
    connection
        .execute(
            "UPDATE browser_sessions SET last_used_at = ?1",
            [last_used_at],
        )
        .expect("ages");
}

#[tokio::test]
async fn a_session_idle_past_thirty_days_is_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("idle");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;
    let day = 24 * 60 * 60;

    age_sessions(&sandbox, crate::store::unix_now() - 29 * day);
    let (status, headers, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK, "29 days idle is still in");
    assert!(
        set_cookie_line(&headers).contains("Max-Age=2592000"),
        "the lifetime is refreshed with the touch"
    );

    age_sessions(&sandbox, crate::store::unix_now() - 31 * day);
    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "31 days idle is out");
}

#[tokio::test]
async fn login_and_logout_do_not_exist_off_token_mode() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-modes");
    let open = router(SessionContext {
        access: Access::open(7700),
        ..context(sandbox.store())
    });
    let loopback = router(context(sandbox.store()));
    for app in [&open, &loopback] {
        let host = if std::ptr::eq(app, &open) {
            REMOTE_HOST
        } else {
            "127.0.0.1:7700"
        };
        let page = Request::builder()
            .uri("/login")
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("the request builds");
        assert_eq!(send(app, page).await.0, StatusCode::NOT_FOUND);
        for uri in ["/login", "/logout"] {
            let post = Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::HOST, host)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(login_body(TOKEN, "/")))
                .expect("the request builds");
            assert_eq!(send(app, post).await.0, StatusCode::NOT_FOUND, "{uri}");
        }
        let board = Request::builder()
            .uri("/board")
            .header(header::HOST, host)
            .body(Body::empty())
            .expect("the request builds");
        let (_, _, body) = send(app, board).await;
        assert!(!body.contains("Sign out"), "no Sign out off token mode");
    }
}

#[tokio::test]
async fn the_shell_shows_sign_out_in_token_mode() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("shell-sign-out");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;
    for uri in ["/board", "/notes"] {
        let (_, _, body) = send(&app, remote_get(uri, Some(&cookie))).await;
        assert!(
            body.contains("<form class=\"shell__signout\" method=\"post\" action=\"/logout\">"),
            "{uri}: {body}"
        );
        assert!(body.contains("id=\"signed-out-banner\""), "{uri}");
    }
}

#[tokio::test]
async fn a_flood_of_wrong_tokens_is_throttled_and_a_right_token_still_signs_in() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("throttle");
    let app = remote_app(sandbox.store(), TOKEN);

    let wrong = |n: usize| {
        let app = app.clone();
        async move {
            send(
                &app,
                remote_form(
                    "/login",
                    &login_body(&format!("wrong-{n}"), "/"),
                    Some(REMOTE_ORIGIN),
                    None,
                ),
            )
            .await
            .0
        }
    };
    let flood: Vec<_> = (0..12).map(|n| tokio::spawn(wrong(n))).collect();
    // While the wrong ones wait, a right token is neither delayed nor refused.
    tokio::task::yield_now().await;
    let (status, _, _) = send(
        &app,
        remote_form("/login", &login_body(TOKEN, "/"), Some(REMOTE_ORIGIN), None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "a right token still signs in"
    );

    let mut statuses = Vec::new();
    for task in flood {
        statuses.push(task.await.expect("joins"));
    }
    let throttled = statuses
        .iter()
        .filter(|status| **status == StatusCode::TOO_MANY_REQUESTS)
        .count();
    let answered = statuses
        .iter()
        .filter(|status| **status == StatusCode::UNAUTHORIZED)
        .count();
    assert!(throttled >= 1, "the excess is turned away: {statuses:?}");
    assert!(answered >= 1, "some wait and are answered: {statuses:?}");
}

#[tokio::test]
async fn an_events_stream_closes_when_its_session_is_signed_out() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("stream-close");
    let app = remote_app(sandbox.store(), TOKEN);
    let cookie = sign_in(&app).await;

    let response = app
        .clone()
        .oneshot(remote_get("/events", Some(&cookie)))
        .await
        .expect("the router answers");
    let mut stream = response.into_body();
    stream
        .frame()
        .await
        .expect("the first frame")
        .expect("readable");

    send(
        &app,
        remote_form("/logout", "", Some(REMOTE_ORIGIN), Some(&cookie)),
    )
    .await;
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = stream.frame().await {
            if frame.is_err() {
                break;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream closes once its session is gone");
}

#[tokio::test]
async fn login_logs_neither_the_token_nor_the_cookie() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("login-logs");
    let app = remote_app(sandbox.store(), TOKEN);
    let captured = Captured::default();
    let _guard = captured.install();

    send(
        &app,
        remote_form(
            "/login",
            &login_body("wrong-token-value", "/"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    send(
        &app,
        remote_form("/login", &login_body(TOKEN, "/"), Some(REMOTE_ORIGIN), None),
    )
    .await;
    let cookie = sign_in(&app).await;
    send(&app, remote_get("/board", Some(&cookie))).await;
    send(
        &app,
        remote_form("/login", &login_body(TOKEN, "/"), None, None),
    )
    .await;
    send(
        &app,
        remote_form("/logout", "", Some(REMOTE_ORIGIN), Some(&cookie)),
    )
    .await;

    let text = captured.text();
    let value = cookie.split_once('=').map(|(_, value)| value).unwrap_or("");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("wrong-token-value"), "{text}");
    assert!(!value.is_empty() && !text.contains(value), "{text}");
    assert!(!text.contains("tandem_session"), "{text}");
}

#[tokio::test]
async fn a_store_without_the_sessions_table_reads_as_signed_out() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("no-table");
    // A fresh home has no database at all; a v12 store has no table. Both read
    // as signed out, never as an error page.
    let app = remote_app(sandbox.store(), TOKEN);
    let forged = format!("{}={}", guard::SESSION_COOKIE, "ab".repeat(32));
    let (status, _, _) = send(&app, remote_get("/board", Some(&forged))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // A store migrated to v12 only: build it by hand.
    let upgraded = Sandbox::new("no-table-v12");
    let store = upgraded.store();
    std::fs::create_dir_all(store.home().directory()).expect("home");
    let connection = rusqlite::Connection::open(store.database()).expect("opens");
    crate::store::schema::migrate_with(
        &connection,
        &crate::store::schema::MIGRATIONS[..12],
        &Actor::Human,
    )
    .expect("v12 applies");
    drop(connection);
    let app = remote_app(store, TOKEN);
    let (status, _, _) = send(&app, remote_get("/board", Some(&forged))).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "no table is no session");
    // The first login migrates the store lazily and signs in.
    let cookie = sign_in(&app).await;
    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
}

// CUT-37: a public origin behind a TLS proxy.

const PUBLIC: &str = "https://ws.example.com";

fn public_app(store: Store, origin: Option<&str>) -> Router {
    let access = Access::token(7700, TOKEN);
    let access = match origin {
        Some(origin) => access
            .with_public_origin(origin)
            .expect("the origin is valid"),
        None => access,
    };
    router(SessionContext {
        access,
        ..context(store)
    })
}

fn public_form(host: &str, origins: &[&str]) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, host)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    for origin in origins {
        builder = builder.header(header::ORIGIN, *origin);
    }
    builder
        .body(Body::from("title=x"))
        .expect("the request builds")
}

fn public_mcp(origins: &[&str]) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    for origin in origins {
        builder = builder.header(header::ORIGIN, *origin);
    }
    builder
        .body(Body::from(MCP_BODY))
        .expect("the request builds")
}

#[tokio::test]
async fn behind_a_public_name_a_form_write_with_a_forwarded_host_passes_unconfigured() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-forwarded");
    let app = public_app(sandbox.store(), None);
    let (status, _, _) = send(&app, public_form("ws.example.com", &[PUBLIC])).await;
    assert_ne!(status, StatusCode::FORBIDDEN, "the proxy kept Host");
}

#[tokio::test]
async fn behind_a_public_name_a_form_write_with_a_rewritten_host_is_refused_unconfigured() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-rewritten");
    let app = public_app(sandbox.store(), None);
    let (status, _, _) = send(&app, public_form("127.0.0.1:7700", &[PUBLIC])).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the proxy rewrote Host");
}

#[tokio::test]
async fn a_configured_origin_admits_a_form_write_with_a_rewritten_host() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-admits");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    let (status, _, _) = send(&app, public_form("127.0.0.1:7700", &[PUBLIC])).await;
    assert_ne!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(&app, public_form("127.0.0.1:7700", &[])).await;
    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "an absent Origin still passes"
    );
}

#[tokio::test]
async fn a_configured_origin_refuses_every_other_origin_on_a_form_write() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-hostile");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    let hostile: [&[&str]; 11] = [
        &["https://evil.example"],
        &["http://ws.example.com"],
        &["https://ws.example.com/"],
        &["HTTPS://ws.example.com"],
        &["https://ws.example.com@evil.example"],
        &["https://evil.example@ws.example.com"],
        &["https://ws.example.com:443"],
        &["ws.example.com"],
        &["null"],
        &["https://ws.example.com.evil.example"],
        &[PUBLIC, "https://evil.example"],
    ];
    for origins in hostile {
        let (status, headers, body) = send(&app, public_form("ws.example.com", origins)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origins:?} is refused");
        assert_eq!(body, "the request came from another origin\n");
        assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }
    let (status, _, _) = send(&app, public_form("evil.example", &[PUBLIC])).await;
    assert_ne!(status, StatusCode::FORBIDDEN, "the exact value passes");
    let loopback = public_form("127.0.0.1:7700", &["http://127.0.0.1:7700"]);
    let (status, _, _) = send(&app, loopback).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a form write wants the public origin"
    );
}

#[tokio::test]
async fn mcp_accepts_the_configured_origin_beside_loopback_and_refuses_every_other() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-mcp");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    for origins in [&[PUBLIC][..], &["http://127.0.0.1:7700"], &[]] {
        let (status, _, _) = send(&app, public_mcp(origins)).await;
        assert_eq!(status, StatusCode::OK, "{origins:?} is admitted");
    }
    for origins in [
        &["https://evil.example"][..],
        &["http://ws.example.com"],
        &["https://ws.example.com:443"],
        &["https://127.0.0.1:7700"],
        &["null"],
        &[PUBLIC, PUBLIC],
    ] {
        let (status, _, _) = send(&app, public_mcp(origins)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origins:?} is refused");
    }
}

#[tokio::test]
async fn without_a_configured_origin_a_public_https_origin_on_mcp_is_still_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-mcp-unset");
    let app = public_app(sandbox.store(), None);
    let (status, _, _) = send(&app, public_mcp(&[PUBLIC])).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(&app, public_mcp(&["http://127.0.0.1:7700"])).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn events_behind_a_public_origin_streams_with_the_token() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-events");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    let request = |bearer: bool| {
        let mut builder = Request::builder()
            .uri("/events")
            .header(header::HOST, "ws.example.com")
            .header(header::ORIGIN, PUBLIC);
        if bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
        }
        builder.body(Body::empty()).expect("the request builds")
    };
    let response = app.clone().oneshot(request(true)).await.expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    let response = app.clone().oneshot(request(false)).await.expect("answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_configured_origin_adds_no_host_check_and_no_cors_header() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-no-host");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    for host in ["anything.example", "127.0.0.1:7700", "ws.example.com"] {
        let request = Request::builder()
            .uri("/board")
            .header(header::HOST, host)
            .header(header::ORIGIN, PUBLIC)
            .header("x-forwarded-host", "evil.example")
            .header("x-forwarded-proto", "http")
            .header("forwarded", "host=evil.example")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .expect("the request builds");
        let (status, headers, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::OK, "{host} is not checked");
        assert!(
            headers
                .keys()
                .all(|name| !name.as_str().starts_with("access-control"))
        );
    }
    let (status, headers, _) = send(
        &app,
        public_form("ws.example.com", &["https://evil.example"]),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        headers
            .keys()
            .all(|name| !name.as_str().starts_with("access-control"))
    );
}

#[tokio::test]
async fn an_open_bind_with_a_configured_origin_refuses_a_direct_address_write() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-open");
    let access = Access::open(7700)
        .with_public_origin(PUBLIC)
        .expect("the origin is valid");
    let app = router(SessionContext {
        access,
        ..context(sandbox.store())
    });
    let direct = Request::builder()
        .method("POST")
        .uri("/tickets")
        .header(header::HOST, "100.64.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "http://100.64.0.1:7700")
        .body(Body::from("title=x"))
        .expect("the request builds");
    let (status, _, _) = send(&app, direct).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_guard_logs_which_check_refused_and_never_the_origin_value() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("public-log");
    let app = public_app(sandbox.store(), Some(PUBLIC));
    let captured = Captured::default();
    let _guard = captured.install();
    let (status, _, _) = send(
        &app,
        public_form("ws.example.com", &["https://evil.example"]),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(&app, public_mcp(&["https://evil.example"])).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let text = captured.text();
    assert!(text.contains("the form origin rule refused"), "{text}");
    assert!(text.contains("the /mcp origin rule refused"), "{text}");
    assert!(text.contains("public_origin_configured=true"), "{text}");
    assert!(!text.contains("evil.example"), "{text}");
    assert!(!text.contains("ws.example.com"), "{text}");
}

#[test]
fn access_debug_prints_the_origin_and_redacts_the_token() {
    let access = Access::token(7700, TOKEN)
        .with_public_origin(PUBLIC)
        .expect("the origin is valid");
    let printed = format!("{access:?}");
    assert!(printed.contains(PUBLIC), "{printed}");
    assert!(!printed.contains(TOKEN), "{printed}");
}

#[test]
fn the_public_origin_normalizer_accepts_the_browser_forms_and_refuses_the_rest() {
    let accepted = [
        ("https://ws.example.com", "https://ws.example.com"),
        ("HTTPS://WS.Example.COM", "https://ws.example.com"),
        ("https://ws.example.com:443", "https://ws.example.com"),
        ("https://ws.example.com:8443", "https://ws.example.com:8443"),
        ("https://203.0.113.5", "https://203.0.113.5"),
        ("https://[2001:DB8::1]:8443", "https://[2001:db8::1]:8443"),
    ];
    for (given, wanted) in accepted {
        assert_eq!(
            super::normalize_public_origin(given).as_deref(),
            Ok(wanted),
            "{given}"
        );
    }
    let refused = [
        "",
        "http://ws.example.com",
        "ws.example.com",
        "https://ws.example.com/",
        "https://ws.example.com/path",
        "https://ws.example.com?x=1",
        "https://ws.example.com#f",
        "https://user@ws.example.com",
        "https://ws.example.com.",
        "https://.example.com",
        "https://ws..example.com",
        "https://ws.example.com:0",
        "https://ws.example.com:0443",
        "https://ws.example.com:65536",
        "https://ws.example.com:",
        "https://ws.example.com:x",
        "https://ws example.com",
        "https://w\u{e9}.example.com",
        "https://ws.example.com\\x",
        "https://ws%2eexample.com",
        "https://localhost",
        "https://127.0.0.1:7700",
        "https://[::1]",
        "https://",
    ];
    for given in refused {
        assert!(
            super::normalize_public_origin(given).is_err(),
            "{given:?} is refused"
        );
    }
}

#[tokio::test]
async fn every_route_and_refusal_carries_the_security_headers() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("security-headers");
    let app = router(context(sandbox.store()));
    let asserts = |label: &str, headers: &HeaderMap| {
        assert_eq!(
            headers[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff",
            "{label}"
        );
        assert_eq!(headers[header::REFERRER_POLICY], "same-origin", "{label}");
        assert_eq!(
            headers["cross-origin-opener-policy"], "same-origin",
            "{label}"
        );
        assert_eq!(
            headers["cross-origin-resource-policy"], "same-origin",
            "{label}"
        );
        assert!(
            headers.contains_key(header::CONTENT_SECURITY_POLICY),
            "{label}: CSP"
        );
        assert!(
            headers.contains_key(header::CACHE_CONTROL),
            "{label}: cache policy"
        );
    };
    let bare = |method: &str, uri: &str| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1:7700")
            .body(Body::empty())
            .expect("the request builds")
    };
    for uri in [
        "/",
        "/board",
        "/health",
        "/notes",
        "/notes/view",
        "/notes/edit",
        "/notes/open",
        "/notes/concept",
        "/tickets/none",
        "/static/missing.css",
        "/no/such/route",
    ] {
        let (_, headers, _) = send(&app, bare("GET", uri)).await;
        asserts(uri, &headers);
    }
    for uri in [
        "/mcp",
        "/tickets",
        "/tickets/x",
        "/tickets/x/status",
        "/tickets/x/comments",
        "/tickets/x/field",
        "/labels",
        "/labels/x/rename",
        "/notes/preview",
        "/notes/save",
        "/notes/create",
        "/notes/create-folder",
        "/notes/daily",
        "/notes/new",
        "/notes/move",
        "/notes/rename",
        "/notes/delete",
        "/notes/restore",
    ] {
        // No form content type: the guard refuses, and the refusal is covered too.
        let (status, headers, _) = send(&app, bare("POST", uri)).await;
        asserts(uri, &headers);
        assert!(status.is_client_error() || uri == "/mcp", "{uri}: {status}");
    }
    let (_, headers, _) = get(&app, "/").await;
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "no-store",
        "pages are never cached"
    );
}

#[tokio::test]
async fn a_store_newer_than_the_build_draws_a_page_notice_not_a_500() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-newer-store");
    setup_board(&sandbox.store());
    let ahead = crate::store::CURRENT_VERSION + 1;
    rusqlite::Connection::open(sandbox.root.join("home").join("tandem.db"))
        .expect("the store opens")
        .execute(
            "INSERT INTO schema_migrations (version, applied_at, author) VALUES (?1, '2026-11-16T00:00:00Z', 'newer')",
            [ahead],
        )
        .expect("the newer version is stamped");
    let app = router(context(sandbox.store()));

    let (status, _, body) = get(&app, "/").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("written by a newer Tandem"), "{body}");
    assert!(body.contains("Upgrade this binary"), "{body}");
    assert!(!body.contains("ADR 0031"), "{body}");
}

// MARK: - The export route (CUT-38)

/// A board with tickets and two notes, so an archive has something to restore.
fn exportable(store: &Store) -> Board {
    let board = setup_board(store);
    let notes = vault(store);
    notes
        .write("export-probe.md", "# Probe\n\nA note that travels.\n", None)
        .expect("the note is written");
    notes
        .write("folder/inner.md", "inner\n", None)
        .expect("the nested note is written");
    board
}

fn export_get(headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder()
        .uri("/export")
        .header(header::HOST, "127.0.0.1:7700");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder.body(Body::empty()).expect("the request builds")
}

async fn send_raw(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body collects")
        .to_bytes();
    (status, headers, bytes.to_vec())
}

fn refusing_context(store: Store) -> SessionContext {
    SessionContext {
        export_check: ExportCheck::new(|_| Err(crate::archive::ArchiveError::NotAllowed)),
        ..context(store)
    }
}

#[tokio::test]
async fn the_export_route_streams_an_archive_that_imports_into_an_empty_home() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-roundtrip");
    let store = sandbox.store();
    let board = exportable(&store);
    let app = router(context(store.clone()));

    let (status, _, bytes) = send_raw(&app, export_get(&[])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "the body is gzip");

    let fresh = Home::resolve(
        Some(&sandbox.root.join("restored")),
        None,
        &sandbox.root.join("user"),
    );
    crate::archive::import(&fresh, bytes.as_slice(), None).expect("the archive imports");
    let restored = Board::with_actor(Store::new(fresh.clone()), Actor::Human);
    let query = crate::domain::TicketQuery::default();
    assert_eq!(
        restored
            .list(&query)
            .expect("the restored board lists")
            .len(),
        board.list(&query).expect("the board lists").len()
    );
    let note = std::fs::read_to_string(fresh.directory().join("notes/export-probe.md"))
        .expect("the note is restored");
    assert!(note.contains("A note that travels."));
}

#[tokio::test]
async fn the_export_route_wants_the_token_off_loopback() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-token");
    exportable(&sandbox.store());
    let app = remote_app(sandbox.store(), TOKEN);

    let request = |auth: Option<&str>| {
        let mut builder = Request::builder()
            .uri("/export")
            .header(header::HOST, REMOTE_HOST);
        if let Some(auth) = auth {
            builder = builder.header(header::AUTHORIZATION, auth);
        }
        builder.body(Body::empty()).expect("the request builds")
    };
    for auth in [None, Some("Bearer wrong-token")] {
        let (status, headers, _) = send_raw(&app, request(auth)).await;
        assert_ne!(status, StatusCode::OK, "{auth:?}");
        assert_ne!(
            headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/gzip"),
            "{auth:?}"
        );
    }
    let (status, headers, bytes) = send_raw(&app, request(Some(&format!("Bearer {TOKEN}")))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/gzip");
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);
}

#[tokio::test]
async fn the_export_route_refuses_a_cross_site_request() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-site");
    exportable(&sandbox.store());
    let app = router(context(sandbox.store()));

    let table: [(Option<&str>, StatusCode); 6] = [
        (None, StatusCode::OK),
        (Some("same-origin"), StatusCode::OK),
        (Some("none"), StatusCode::OK),
        (Some("cross-site"), StatusCode::FORBIDDEN),
        (Some("same-site"), StatusCode::FORBIDDEN),
        (Some("anything-else"), StatusCode::FORBIDDEN),
    ];
    for (site, expected) in table {
        let headers: Vec<(&str, &str)> = site.map(|s| ("sec-fetch-site", s)).into_iter().collect();
        let (status, response_headers, body) = send_raw(&app, export_get(&headers)).await;
        assert_eq!(status, expected, "Sec-Fetch-Site {site:?}");
        if expected == StatusCode::FORBIDDEN {
            assert_ne!(response_headers[header::CONTENT_TYPE], "application/gzip");
            assert!(
                !body.is_empty() && body[0] != 0x1f,
                "a readable refusal, not an archive"
            );
        }
    }
}

#[tokio::test]
async fn the_export_response_is_an_attachment_and_is_never_cached() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-headers");
    exportable(&sandbox.store());
    let app = router(context(sandbox.store()));

    let (status, headers, _) = send_raw(&app, export_get(&[])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/gzip");
    let disposition = headers[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("the disposition is text");
    assert!(
        disposition.starts_with("attachment; filename=\"tandem-export-")
            && disposition.ends_with(".tar.gz\""),
        "{disposition}"
    );
    let date = &disposition["attachment; filename=\"tandem-export-".len()..][..10];
    assert!(
        date.len() == 10
            && date.as_bytes()[4] == b'-'
            && date.as_bytes()[7] == b'-'
            && date.bytes().filter(u8::is_ascii_digit).count() == 8,
        "a UTC date: {date}"
    );
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(
        headers.get(header::CONTENT_LENGTH).is_none(),
        "the size is not known up front"
    );
}

#[tokio::test]
async fn a_refused_export_answers_a_readable_403_and_streams_nothing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-refused");
    let store = sandbox.store();
    exportable(&store);
    let app = router(refusing_context(store));
    let captured = Captured::default();
    let _guard = captured.install();

    let (status, headers, body) = send_raw(&app, export_get(&[])).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_ne!(headers[header::CONTENT_TYPE], "application/gzip");
    assert!(headers.get(header::CONTENT_DISPOSITION).is_none());
    let text = String::from_utf8(body).expect("the refusal is text");
    assert!(text.contains("may not export"), "{text}");
    assert!(
        !captured.text().contains("export started"),
        "a refused export writes no record"
    );
}

#[tokio::test]
async fn the_shell_shows_the_export_control_only_when_export_is_allowed() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-control");
    let store = sandbox.store();
    exportable(&store);
    let allowed = router(context(store.clone()));
    let refused = router(refusing_context(store));

    for uri in ["/board", "/notes"] {
        let (_, _, page) = get(&allowed, uri).await;
        assert!(
            page.contains(
                "<a class=\"menu__option\" href=\"/export\" download>Export workspace</a>"
            ),
            "{uri}: {page}"
        );
        assert!(page.contains("is not encrypted"), "{uri}: the warning line");
        let (_, _, page) = get(&refused, uri).await;
        assert!(!page.contains("/export"), "{uri}: {page}");
        assert!(!page.contains("Export workspace"), "{uri}");
    }
}

#[tokio::test]
async fn an_export_logs_the_actor_and_the_time_and_never_a_secret_or_a_path() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-log");
    let store = sandbox.store();
    exportable(&store);
    let app = remote_app(store, TOKEN);
    let captured = Captured::default();
    let _guard = captured.install();

    let request = Request::builder()
        .uri("/export")
        .header(header::HOST, REMOTE_HOST)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header("x-tandem-agent", "backup-bot")
        .body(Body::empty())
        .expect("the request builds");
    let (status, _, _) = send_raw(&app, request).await;
    assert_eq!(status, StatusCode::OK);

    let text = captured.text();
    let line = text
        .lines()
        .find(|line| line.contains("export started"))
        .unwrap_or_else(|| panic!("the record is logged: {text}"));
    assert!(line.contains("backup-bot"), "the actor: {line}");
    assert!(
        line.contains("door=\"board\"") || line.contains("door=board"),
        "{line}"
    );
    assert!(line.contains("at="), "the time: {line}");
    assert!(!text.contains(TOKEN), "the token stays out");
    assert!(
        !text.contains(&sandbox.root.display().to_string()),
        "no path in the log: {text}"
    );
    assert!(!text.contains("A note that travels"), "no content");
}

#[tokio::test]
async fn a_board_write_succeeds_while_an_export_is_streaming() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-concurrent");
    let store = sandbox.store();
    let board = exportable(&store);
    // Incompressible enough to fill the channel several times over, so the
    // writer is parked mid-archive while the write below runs.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut big = String::new();
    while big.len() < 6 * 1024 * 1024 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        big.push_str(&format!("{seed:016x}"));
    }
    std::fs::write(store.home().directory().join("notes/big.md"), big)
        .expect("the large note is written");
    let app = router(context(store.clone()));

    let response = app
        .clone()
        .oneshot(export_get(&[]))
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let before = store.revision().expect("the revision reads");
    let body = format!(
        "title=During&status=ready&ticket_project=Alpha&ticket_label=&expected_revision={before}&project=&show_all=&assignee=&text="
    );
    let (status, _, page) = send(&app, form_request("/tickets", &body)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("During"));
    assert!(store.revision().expect("the revision reads") > before);
    assert_eq!(
        board
            .list(&crate::domain::TicketQuery::default())
            .expect("the board lists")
            .iter()
            .filter(|ticket| ticket.title == "During")
            .count(),
        1
    );

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the stream finishes")
        .to_bytes();
    assert!(bytes.len() > 1024 * 1024, "the whole archive streamed");
}

#[tokio::test]
async fn export_answers_get_only() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-methods");
    exportable(&sandbox.store());
    let app = router(context(sandbox.store()));

    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let request = Request::builder()
            .method(method)
            .uri("/export")
            .header(header::HOST, "127.0.0.1:7700")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::empty())
            .expect("the request builds");
        let (status, headers, _) = send_raw(&app, request).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert!(
            headers.get(header::CONTENT_DISPOSITION).is_none(),
            "{method}"
        );
    }
}

#[tokio::test]
async fn an_export_moves_neither_the_revision_nor_the_events_stream() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-quiet");
    let store = sandbox.store();
    exportable(&store);
    let app = router(context(store.clone()));

    let stop = Arc::new(AtomicBool::new(false));
    let (changes, mut receiver) = broadcast::channel(8);
    let reads_before = store.watch_revision_reads();
    let handle = watch(store.clone(), changes, Arc::clone(&stop));
    let attached = Instant::now() + Duration::from_secs(10);
    while store.watch_revision_reads() == reads_before && Instant::now() < attached {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(WATCH_INTERVAL * 3).await;
    let _ = seen(&mut receiver);
    let before = store.revision().expect("the revision reads");

    let (status, _, _) = send_raw(&app, export_get(&[])).await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(WATCH_INTERVAL * 6).await;

    let events = seen(&mut receiver);
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
    assert_eq!(store.revision().expect("the revision reads"), before);
    assert!(events.is_empty(), "the stream stayed quiet: {events:?}");
}

/// The route subscribes before it reads the revision (events.rs). A write that
/// is broadcast in the gap between the two must still reach the stream; with the
/// lines swapped the gap is before the subscribe and the frame never arrives.
#[tokio::test]
async fn a_write_broadcast_between_subscribe_and_revision_still_reaches_the_stream() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("events-order");
    let store = sandbox.store();
    let (changes, _receiver) = broadcast::channel(8);
    let app = router_with(context(store), changes.clone());

    // A revision no store in this test reaches, so its frame is unmistakable.
    const WRITE: i64 = 1 << 40;
    let writer = changes.clone();
    events::run_between_subscribe_and_revision(move || {
        let _ = writer.send(WRITE);
    });
    let request = Request::builder()
        .uri("/events")
        .header(header::HOST, "127.0.0.1:7700")
        .body(Body::empty())
        .expect("the request builds");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let mut stream = response.into_body();
    let mut text = String::new();
    let wanted = format!("data: {WRITE}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !text.contains(&wanted) && Instant::now() < deadline {
        if let Ok(Some(Ok(frame))) =
            tokio::time::timeout(Duration::from_millis(500), stream.frame()).await
            && let Some(data) = frame.data_ref()
        {
            text.push_str(&String::from_utf8_lossy(data));
        }
    }
    assert!(
        text.contains(&wanted),
        "a write broadcast between subscribe and the revision read never reached the stream: {text}"
    );
}

// MARK: - Members and personal tokens (CUT-36, ADR 0052)

const NOW: i64 = 1_800_000_000;

/// A team home: two members, a token each, and a sign-in key for the first.
struct Team {
    alice_token: String,
    bob_token: String,
    alice_key: String,
}

fn team(store: &Store) -> Team {
    // A project to file into: a ticket always lives in one (TAN-339).
    Board::with_actor(store.clone(), Actor::Human)
        .add_project("Main", None)
        .expect("the project is added");
    store
        .member_add("alice", crate::store::Role::Admin, NOW)
        .unwrap();
    store
        .member_add("bob", crate::store::Role::Member, NOW)
        .unwrap();
    let issue = |name: &str| {
        store
            .member_token_issue(name, "an agent", NOW)
            .unwrap()
            .unwrap()
            .reveal()
            .to_string()
    };
    Team {
        alice_token: issue("alice"),
        bob_token: issue("bob"),
        alice_key: store
            .member_signin_key_issue("alice", NOW)
            .unwrap()
            .unwrap()
            .reveal()
            .to_string(),
    }
}

fn bearer_mcp(token: &str, agent: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, REMOTE_HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(agent) = agent {
        builder = builder.header("x-tandem-agent", agent);
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

fn bearer_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::HOST, REMOTE_HOST)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request builds")
}

fn project_call(name: &str) -> String {
    tool_call_body("project_add", serde_json::json!({ "name": name }))
}

fn label_author(store: &Store, name: &str) -> Option<String> {
    let connection = rusqlite::Connection::open(store.home().directory().join("tandem.db"))
        .expect("the store opens");
    connection
        .query_row("SELECT author FROM labels WHERE name = ?1", [name], |row| {
            row.get::<_, String>(0)
        })
        .ok()
}

/// Sign in at `/login` with a sign-in key or the owner token: the cookie pair.
async fn sign_in_with(app: &Router, secret: &str) -> String {
    let (status, headers, _) = send(
        app,
        remote_form(
            "/login",
            &login_body(secret, "/board"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    session_cookie_pair(&headers).expect("a right credential sets the cookie")
}

fn add_label_form(name: &str, cookie: &str) -> Request<Body> {
    remote_form(
        "/labels",
        &format!("name={name}&project=Main"),
        Some(REMOTE_ORIGIN),
        Some(cookie),
    )
}

fn is_ok_frame(body: &str) -> bool {
    !body.contains("isError")
}

#[tokio::test]
async fn a_personal_token_reaches_mcp_and_records_its_member_as_the_actor() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-mcp");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);

    let (status, _, body) = send(
        &app,
        bearer_mcp(&team.alice_token, None, &project_call("by-alice")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(is_ok_frame(&body), "the write succeeds: {body}");
    assert_eq!(author_of_project(&store, "by-alice"), "alice");

    let (status, _, body) = send(
        &app,
        bearer_mcp(&team.bob_token, None, &project_call("by-bob")),
    )
    .await;
    assert_eq!((status, is_ok_frame(&body)), (StatusCode::OK, true));
    assert_eq!(author_of_project(&store, "by-bob"), "bob");

    let (status, _, _) = send(&app, bearer_get("/health", &team.alice_token)).await;
    assert_eq!(status, StatusCode::OK, "a token reaches /health");
}

#[tokio::test]
async fn a_comment_without_an_author_is_signed_by_the_credentials_member() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-comment");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let board = Board::with_actor(store.clone(), Actor::Human);
    let ticket = board
        .add(
            "Discussed",
            AddTicket {
                project: Some("Main"),
                ..AddTicket::default()
            },
        )
        .expect("the board changes");
    let id = ticket.id.to_string();

    for (author, body) in [(None, "unsigned"), (Some("  "), "blank")] {
        let mut arguments = serde_json::json!({ "id": id, "body": body });
        if let Some(author) = author {
            arguments["author"] = serde_json::json!(author);
        }
        let (status, _, reply) = send(
            &app,
            bearer_mcp(
                &team.bob_token,
                None,
                &tool_call_body("ticket_comment_add", arguments),
            ),
        )
        .await;
        assert_eq!(
            (status, is_ok_frame(&reply)),
            (StatusCode::OK, true),
            "{reply}"
        );
    }
    // A named author is the caller's to give, as before.
    let (_, _, reply) = send(
        &app,
        bearer_mcp(
            &team.bob_token,
            None,
            &tool_call_body(
                "ticket_comment_add",
                serde_json::json!({ "id": id, "author": "builder", "body": "signed" }),
            ),
        ),
    )
    .await;
    assert!(is_ok_frame(&reply), "{reply}");

    let authors: Vec<(String, String)> = board
        .comments(&id)
        .expect("the log reads")
        .into_iter()
        .map(|comment| (comment.author, comment.body))
        .collect();
    assert_eq!(
        authors,
        [
            ("bob".to_string(), "unsigned".to_string()),
            ("bob".to_string(), "blank".to_string()),
            ("builder".to_string(), "signed".to_string()),
        ]
    );
}

/// Every route of `routes()`, with a concrete path for each template.
const ROUTE_TABLE: &[(&str, &str, &str)] = &[
    ("GET", "/", "/"),
    ("GET", "/board", "/board"),
    ("GET", "/events", "/events"),
    ("GET", "/export", "/export"),
    ("GET", "/health", "/health"),
    ("GET", "/login", "/login"),
    ("POST", "/login", "/login"),
    ("POST", "/logout", "/logout"),
    ("POST", "/mcp", "/mcp"),
    ("GET", "/static/{*path}", "/static/tandem.css"),
    ("POST", "/tickets", "/tickets"),
    ("GET", "/tickets/{id}", "/tickets/abc"),
    ("POST", "/tickets/{id}/status", "/tickets/abc/status"),
    ("POST", "/tickets/{id}/comments", "/tickets/abc/comments"),
    ("POST", "/tickets/{id}/field", "/tickets/abc/field"),
    ("POST", "/labels", "/labels"),
    ("POST", "/labels/{name}/rename", "/labels/x/rename"),
    ("GET", "/tokens", "/tokens"),
    ("POST", "/tokens", "/tokens"),
    ("GET", "/tokens/{id}/revoke", "/tokens/1/revoke"),
    ("POST", "/tokens/{id}/revoke", "/tokens/1/revoke"),
    ("GET", "/people", "/people"),
    ("POST", "/people", "/people"),
    ("GET", "/people/{name}", "/people/bob"),
    ("GET", "/people/{name}/key", "/people/bob/key"),
    ("POST", "/people/{name}/key", "/people/bob/key"),
    ("POST", "/people/{name}/role", "/people/bob/role"),
    ("GET", "/people/{name}/remove", "/people/bob/remove"),
    ("POST", "/people/{name}/remove", "/people/bob/remove"),
    (
        "GET",
        "/people/{name}/tokens/{id}/revoke",
        "/people/bob/tokens/1/revoke",
    ),
    (
        "POST",
        "/people/{name}/tokens/{id}/revoke",
        "/people/bob/tokens/1/revoke",
    ),
    ("GET", "/notes", "/notes"),
    ("GET", "/notes/graph", "/notes/graph"),
    ("GET", "/notes/graph.json", "/notes/graph.json"),
    ("GET", "/notes/targets", "/notes/targets"),
    ("GET", "/notes/view", "/notes/view"),
    ("GET", "/notes/edit", "/notes/edit"),
    ("GET", "/notes/open", "/notes/open"),
    ("GET", "/notes/concept", "/notes/concept"),
    ("POST", "/notes/preview", "/notes/preview"),
    ("POST", "/notes/save", "/notes/save"),
    ("POST", "/notes/create", "/notes/create"),
    ("POST", "/notes/create-folder", "/notes/create-folder"),
    ("POST", "/notes/daily", "/notes/daily"),
    ("POST", "/notes/new", "/notes/new"),
    ("POST", "/notes/move", "/notes/move"),
    ("POST", "/notes/rename", "/notes/rename"),
    ("POST", "/notes/delete", "/notes/delete"),
    ("POST", "/notes/restore", "/notes/restore"),
];

fn table_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, REMOTE_HOST)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    builder = match uri {
        "/mcp" => builder.header(header::CONTENT_TYPE, "application/json"),
        _ if method == "POST" => {
            builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        }
        _ => builder,
    };
    let body = if uri == "/mcp" { MCP_BODY } else { "" };
    builder
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

#[tokio::test]
async fn a_personal_token_is_refused_403_on_every_route_but_mcp_and_health() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-routes");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store, TOKEN);

    // The table covers the whole route table: a route added later fails here
    // until it is placed.
    let source = include_str!("mod.rs");
    let routes_fn = &source[source.find("fn routes()").expect("routes()")
        ..source.find("fn layered(").expect("layered()")];
    let mut declared: Vec<&str> = routes_fn
        .split(".route(")
        .skip(1)
        .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
        .collect();
    // The operator family is not a member's to reach: its own tests walk it.
    declared.retain(|route| !route.starts_with(guard::OPERATOR_PREFIX));
    declared.sort_unstable();
    declared.dedup();
    let mut covered: Vec<&str> = ROUTE_TABLE
        .iter()
        .map(|(_, template, _)| *template)
        .collect();
    covered.sort_unstable();
    covered.dedup();
    assert_eq!(declared, covered, "the table names every route");

    let captured = Captured::default();
    let _guard = captured.install();
    for (method, _, uri) in ROUTE_TABLE {
        let (status, _, body) = send(&app, table_request(method, uri, &team.alice_token)).await;
        let admitted = matches!(*uri, "/mcp" | "/health");
        if admitted {
            assert_eq!(status, StatusCode::OK, "{method} {uri}: {body}");
        } else {
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
            assert!(
                !body.contains(&team.alice_token),
                "the refusal never echoes the credential"
            );
        }
    }
    let text = captured.text();
    assert!(
        text.contains("a personal token reaches /mcp and /health only"),
        "the reason is in the log: {text}"
    );
    assert!(!text.contains(&team.alice_token), "{text}");
}

#[tokio::test]
async fn the_agent_header_cannot_name_another_member_or_human() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-header");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let captured = Captured::default();
    let _guard = captured.install();

    // Absent and the member's own name are accepted, and record the member.
    for (index, agent) in [None, Some("alice")].into_iter().enumerate() {
        let name = format!("ok-{index}");
        let (status, _, body) = send(
            &app,
            bearer_mcp(&team.alice_token, agent, &project_call(&name)),
        )
        .await;
        assert_eq!(
            (status, is_ok_frame(&body)),
            (StatusCode::OK, true),
            "{agent:?}"
        );
        assert_eq!(author_of_project(&store, &name), "alice");
    }
    // Anyone else, `human` included, is refused 403 and writes nothing.
    for (index, agent) in ["bob", "human", "builder"].into_iter().enumerate() {
        let name = format!("refused-{index}");
        let (status, _, body) = send(
            &app,
            bearer_mcp(&team.alice_token, Some(agent), &project_call(&name)),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{agent}");
        assert!(!body.contains(agent), "the value is not echoed: {body}");
        assert!(
            store
                .read(|connection| connection
                    .query_row(
                        "SELECT 1 FROM projects WHERE name = ?1",
                        [&name],
                        |_| Ok(())
                    )
                    .map_err(|error| crate::store::translate(error, connection)))
                .map(|row| row.is_none())
                .unwrap_or(true),
            "{agent} wrote nothing"
        );
    }
    // A syntax error stays a 400, as for the owner.
    let (status, _, _) = send(
        &app,
        bearer_mcp(&team.alice_token, Some("Not A Name"), MCP_BODY),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A member's browser session holds to the same rule.
    let cookie = sign_in_with(&app, &team.alice_key).await;
    let mut named = add_label_form("by-session", &cookie);
    named
        .headers_mut()
        .insert("x-tandem-agent", "bob".parse().unwrap());
    assert_eq!(send(&app, named).await.0, StatusCode::FORBIDDEN);
    assert_eq!(label_author(&store, "by-session"), None);

    let text = captured.text();
    assert!(
        text.contains("names someone other than the credential's member"),
        "the reason is logged: {text}"
    );
    assert!(!text.contains("builder"), "no value is logged: {text}");
}

#[tokio::test]
async fn a_revoked_an_unknown_a_removed_and_a_malformed_token_answer_alike() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-alike");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);

    let revoked = store
        .member_token_issue("alice", "to revoke", NOW)
        .unwrap()
        .unwrap();
    let id = store.member_tokens("alice").unwrap().last().unwrap().id;
    assert!(store.member_token_revoke("alice", id, NOW).unwrap());
    store.member_remove("bob", NOW).unwrap();
    let unknown = format!("tdm_{}", "B".repeat(43));

    let mut answers = Vec::new();
    for presented in [
        revoked.reveal().to_string(),
        unknown,
        team.bob_token.clone(),
        "tdm_short".to_string(),
        format!("tdk_{}", "C".repeat(43)),
        "nonsense".to_string(),
        "wrong-token-wrong-token-wrong-tok".to_string(),
    ] {
        let (status, headers, body) = send(&app, bearer_mcp(&presented, None, MCP_BODY)).await;
        let mut kept: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
            .collect();
        kept.sort();
        answers.push((status, body, kept));
    }
    // Every personal-token-shaped value, revoked, unknown, removed or short,
    // gets one answer a tool can act on; the rest get the plain one.
    assert_eq!(answers[0].0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        answers[0].1, "This token has been revoked or does not exist.\n",
        "one body a tool can act on"
    );
    for answer in &answers[1..4] {
        assert_eq!(
            answer, &answers[0],
            "one status, one body, one set of headers"
        );
    }
    assert_eq!(answers[4].1, "unauthorized\n");
    for answer in &answers[5..] {
        assert_eq!(answer, &answers[4], "the other refusals are one answer");
    }
    // On a page route the same credentials all meet the same redirect.
    let (status, _, _) = send(&app, bearer_get("/board", &team.bob_token)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn a_sign_in_key_at_login_mints_a_session_that_belongs_to_its_member_and_never_works_as_a_bearer()
 {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-login");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);

    let cookie = sign_in_with(&app, &team.alice_key).await;
    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK, "the session opens the board");
    let member_ids: Vec<Option<i64>> = {
        let connection = rusqlite::Connection::open(store.home().directory().join("tandem.db"))
            .expect("the store opens");
        let mut statement = connection
            .prepare("SELECT member_id FROM browser_sessions")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
    };
    let alice = store.member_find("alice").unwrap().unwrap();
    assert_eq!(
        member_ids,
        vec![Some(alice.id)],
        "the session names its member"
    );

    // A key is never a bearer: not on /mcp, not on a page.
    let (status, _, body) = send(&app, bearer_mcp(&team.alice_key, None, MCP_BODY)).await;
    assert_eq!(
        (status, body.as_str()),
        (StatusCode::UNAUTHORIZED, "unauthorized\n")
    );
    let (status, _, _) = send(&app, bearer_get("/board", &team.alice_key)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // A personal token is another kind: it never signs a browser in.
    let (status, headers, _) = send(
        &app,
        remote_form(
            "/login",
            &login_body(&team.alice_token, "/"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(session_cookie_pair(&headers).is_none());

    // The owner token still signs in, to an owner session with no member.
    let owner = sign_in_with(&app, TOKEN).await;
    assert_eq!(
        send(&app, remote_get("/board", Some(&owner))).await.0,
        StatusCode::OK
    );
    let nulls: i64 = rusqlite::Connection::open(store.home().directory().join("tandem.db"))
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM browser_sessions WHERE member_id IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(nulls, 1);
}

#[tokio::test]
async fn a_member_browser_write_records_the_member_and_an_owner_write_records_the_server_actor() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-browser-write");
    let store = sandbox.store();
    let team = team(&store);
    let app = router(SessionContext {
        access: Access::token(7700, TOKEN),
        actor: Actor::Agent("serve-runner".to_string()),
        ..context(store.clone())
    });

    let member = sign_in_with(&app, &team.alice_key).await;
    let owner = sign_in_with(&app, TOKEN).await;
    assert_eq!(
        send(&app, add_label_form("by-member", &member)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&app, add_label_form("by-owner", &owner)).await.0,
        StatusCode::OK
    );
    assert_eq!(label_author(&store, "by-member").as_deref(), Some("alice"));
    assert_eq!(
        label_author(&store, "by-owner").as_deref(),
        Some("serve-runner")
    );
}

#[tokio::test]
async fn a_token_or_key_below_the_length_or_without_the_prefix_is_refused_before_lookup() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-shape");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store, TOKEN);
    let captured = Captured::default();
    let _guard = captured.install();

    let body = "A".repeat(43);
    let malformed = [
        format!("tdm_{}", "A".repeat(42)),
        format!("tdm_{}", "A".repeat(44)),
        format!("tdx_{body}"),
        format!("TDM_{body}"),
        format!("tdm_{}!", "A".repeat(42)),
        body.clone(),
        "tdm_".to_string(),
    ];
    for presented in &malformed {
        let started = Instant::now();
        let (status, _, _) = send(&app, bearer_mcp(presented, None, MCP_BODY)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{presented}");
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "{presented} is refused at once, not after a lookup's wait"
        );
    }
    let (status, _, _) = send(
        &app,
        remote_form(
            "/login",
            &login_body(&format!("tdk_{}", "A".repeat(42)), "/"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        !captured
            .text()
            .contains("a member credential was not accepted"),
        "no lookup ran for a malformed bearer: {}",
        captured.text()
    );
    // A well-formed unknown one does reach the lookup.
    let _ = &team;
    send(&app, bearer_mcp(&format!("tdm_{body}"), None, MCP_BODY)).await;
    assert!(
        captured
            .text()
            .contains("a member credential was not accepted")
    );
}

#[tokio::test]
async fn the_owner_token_keeps_the_header_behaviour_and_every_route_it_reached_before() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-owner");
    let store = sandbox.store();
    team(&store);
    let app = remote_app(store.clone(), TOKEN);

    // Every route the owner reached before is still reachable: none answers it
    // 401 or 403.
    for (method, _, uri) in ROUTE_TABLE {
        if matches!(*uri, "/login" | "/logout") && *method == "POST" {
            continue;
        }
        // The owner holds no personal tokens: its tokens writes are refused
        // by name, and its tokens reads answer a page or a 404.
        if uri.starts_with("/tokens") && *method == "POST" {
            continue;
        }
        let status = if *uri == "/events" {
            // The stream never ends: read the status and leave it.
            app.clone()
                .oneshot(table_request(method, uri, TOKEN))
                .await
                .expect("the router answers")
                .status()
        } else {
            send(&app, table_request(method, uri, TOKEN)).await.0
        };
        assert!(
            status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
            "{method} {uri}: {status}"
        );
    }
    // The header keeps ADR 0047's behaviour byte for byte, a member's name and
    // `human` included.
    for (agent, name) in [
        ("builder", "owner-builder"),
        ("human", "owner-human"),
        ("alice", "owner-as-alice"),
    ] {
        let (status, _, body) =
            send(&app, bearer_mcp(TOKEN, Some(agent), &project_call(name))).await;
        assert_eq!(
            (status, is_ok_frame(&body)),
            (StatusCode::OK, true),
            "{agent}"
        );
        assert_eq!(author_of_project(&store, name), agent);
    }
    let (status, _, _) = send(&app, bearer_mcp(TOKEN, Some("Bad Name"), MCP_BODY)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn solo_modes_with_no_members_are_unchanged() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-solo");
    let store = sandbox.store();

    // Loopback: the header names the actor; a bearer is not looked at.
    let loopback = router(context(store.clone()));
    let (status, _, _) = send(
        &loopback,
        mcp_builder()
            .header("x-tandem-agent", "builder")
            .header(
                header::AUTHORIZATION,
                format!("Bearer tdm_{}", "A".repeat(43)),
            )
            .body(Body::from(project_call("loop-builder")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(author_of_project(&store, "loop-builder"), "builder");
    let (status, _, _) = send(
        &loopback,
        mcp_builder()
            .header("x-tandem-agent", "human")
            .body(Body::from(project_call("loop-human")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(author_of_project(&store, "loop-human"), "human");
    assert_eq!(
        send(&loopback, get_with_host("/board", "evil.example:7700"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Open: no credential, the header names the actor, a member-shaped bearer is ignored.
    let open = router(SessionContext {
        access: Access::open(7700),
        ..context(store.clone())
    });
    let (status, _, _) = send(
        &open,
        Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "machine.tailnet.ts.net:7700")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-tandem-agent", "reviewer")
            .body(Body::from(project_call("open-reviewer")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(author_of_project(&store, "open-reviewer"), "reviewer");

    // A lone TANDEM_TOKEN: the owner bearer and header as before; anything else
    // is the one 401; a member-shaped value finds no members.
    let solo = remote_app(store.clone(), TOKEN);
    let (status, _, _) = send(
        &solo,
        bearer_mcp(TOKEN, Some("refiner"), &project_call("solo-refiner")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(author_of_project(&store, "solo-refiner"), "refiner");
    for (presented, answer) in [
        (
            "wrong-token-wrong-token-wrong-tok".to_string(),
            "unauthorized\n",
        ),
        (
            format!("tdm_{}", "A".repeat(43)),
            "This token has been revoked or does not exist.\n",
        ),
    ] {
        let (status, _, body) = send(&solo, bearer_mcp(&presented, None, MCP_BODY)).await;
        assert_eq!((status, body.as_str()), (StatusCode::UNAUTHORIZED, answer));
    }
    assert!(!store.team_mode().unwrap());
    // The owner's browser sign-in is unchanged.
    let cookie = sign_in_with(&solo, TOKEN).await;
    assert_eq!(
        send(&solo, remote_get("/board", Some(&cookie))).await.0,
        StatusCode::OK
    );
}

fn get_with_host(uri: &str, host: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::HOST, host)
        .body(Body::empty())
        .expect("the request builds")
}

#[tokio::test]
async fn team_mode_fails_closed_when_the_members_table_is_unreadable_and_the_owner_token_still_admits()
 {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-fail-closed");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let cookie = sign_in_with(&app, &team.alice_key).await;
    assert_eq!(
        send(&app, remote_get("/board", Some(&cookie))).await.0,
        StatusCode::OK
    );
    assert!(store.team_mode().unwrap());

    // The members table goes missing: team mode stays on, and no member
    // credential admits anything.
    rusqlite::Connection::open(store.home().directory().join("tandem.db"))
        .unwrap()
        .execute_batch("DROP TABLE members")
        .unwrap();
    assert!(
        store.team_mode().unwrap(),
        "the persisted flag is not unset"
    );

    let (status, _, body) = send(&app, bearer_mcp(&team.alice_token, None, MCP_BODY)).await;
    assert_eq!(
        (status, body.as_str()),
        (
            StatusCode::UNAUTHORIZED,
            "This token has been revoked or does not exist.\n"
        )
    );
    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "a member session is signed out"
    );
    let (status, _, _) = send(
        &app,
        remote_form(
            "/login",
            &login_body(&team.alice_key, "/"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The owner is never locked out.
    let (status, _, _) = send(&app, bearer_get("/board", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, body) = send(&app, bearer_mcp(TOKEN, None, &project_call("owner-still"))).await;
    assert_eq!((status, is_ok_frame(&body)), (StatusCode::OK, true));
    let owner = sign_in_with(&app, TOKEN).await;
    assert_eq!(
        send(&app, remote_get("/board", Some(&owner))).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_removed_members_session_and_open_events_stream_stop() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-removal");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let cookie = sign_in_with(&app, &team.alice_key).await;

    let response = app
        .clone()
        .oneshot(remote_get("/events", Some(&cookie)))
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    stream
        .frame()
        .await
        .expect("the first frame")
        .expect("readable");

    store
        .member_add("zed", crate::store::Role::Admin, NOW)
        .unwrap();
    assert!(store.member_remove("alice", NOW + 1).unwrap());
    // A board change after the removal must not reach the stream.
    let board = Board::with_actor(store.clone(), Actor::Human);
    board
        .add(
            "After the removal",
            AddTicket {
                project: Some("Main"),
                ..AddTicket::default()
            },
        )
        .expect("the board changes");

    let mut seen = String::new();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = stream.frame().await {
            match frame {
                Ok(frame) => {
                    if let Some(data) = frame.data_ref() {
                        seen.push_str(&String::from_utf8_lossy(data));
                    }
                }
                Err(_) => break,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream closes within the bound");
    assert!(
        !seen.contains("event: changed"),
        "the stream received nothing after the removal: {seen}"
    );

    // The session and the token are gone at once; the owner is not affected.
    let (status, _, _) = send(&app, remote_get("/board", Some(&cookie))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, _) = send(&app, bearer_mcp(&team.alice_token, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(&app, bearer_get("/board", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn failed_member_credentials_are_throttled_like_login_and_a_right_one_is_never_delayed() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-throttle");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store, TOKEN);

    let wrong = |n: usize| {
        let app = app.clone();
        async move {
            let presented = format!("tdm_{:0>43}", n);
            send(&app, bearer_mcp(&presented, None, MCP_BODY)).await.0
        }
    };
    let flood: Vec<_> = (0..12).map(|n| tokio::spawn(wrong(n))).collect();
    tokio::task::yield_now().await;
    let started = Instant::now();
    let (status, _, _) = send(&app, bearer_mcp(&team.alice_token, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK, "a right token still passes");
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "a right token is never delayed"
    );
    let owner_started = Instant::now();
    let (status, _, _) = send(&app, bearer_get("/board", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(owner_started.elapsed() < Duration::from_millis(400));

    let mut statuses = Vec::new();
    for task in flood {
        statuses.push(task.await.expect("joins"));
    }
    let throttled = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    let answered = statuses
        .iter()
        .filter(|s| **s == StatusCode::UNAUTHORIZED)
        .count();
    assert!(throttled >= 1, "the excess is turned away: {statuses:?}");
    assert!(answered >= 1, "some wait and are answered: {statuses:?}");
}

#[tokio::test]
async fn a_wrong_sign_in_key_as_a_bearer_waits_on_the_same_throttle() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("key-bearer-throttle");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store, TOKEN);

    let wrong_key = |n: usize| {
        let app = app.clone();
        async move {
            let presented = format!("tdk_{:0>43}", n);
            send(&app, bearer_mcp(&presented, None, MCP_BODY)).await.0
        }
    };
    let flood: Vec<_> = (0..12).map(|n| tokio::spawn(wrong_key(n))).collect();
    let mut statuses = Vec::new();
    for task in flood {
        statuses.push(task.await.expect("joins"));
    }
    assert!(
        statuses.contains(&StatusCode::TOO_MANY_REQUESTS),
        "a wrong sign-in key as a bearer is throttled too: {statuses:?}"
    );
    assert!(
        statuses
            .iter()
            .all(|s| *s == StatusCode::UNAUTHORIZED || *s == StatusCode::TOO_MANY_REQUESTS),
        "a sign-in key is never admitted as a bearer: {statuses:?}"
    );
    let (status, _, _) = send(&app, bearer_mcp(&team.alice_token, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK, "a right token still passes");
}

#[tokio::test]
async fn token_and_key_use_logs_no_credential_header_or_digest() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-logs");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let captured = Captured::default();
    let _guard = captured.install();

    let unknown = format!("tdm_{}", "Z".repeat(43));
    send(
        &app,
        bearer_mcp(&team.alice_token, None, &project_call("logged")),
    )
    .await;
    send(&app, bearer_mcp(&unknown, None, MCP_BODY)).await;
    send(&app, bearer_mcp(&team.alice_token, Some("bob"), MCP_BODY)).await;
    send(&app, bearer_get("/board", &team.alice_token)).await;
    send(&app, bearer_mcp(&team.alice_key, None, MCP_BODY)).await;
    let wrong_key = format!("tdk_{}", "Y".repeat(43));
    send(
        &app,
        remote_form(
            "/login",
            &login_body(&wrong_key, "/"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    let cookie = sign_in_with(&app, &team.alice_key).await;
    send(&app, remote_get("/board", Some(&cookie))).await;
    store
        .member_add("zed", crate::store::Role::Admin, NOW)
        .unwrap();
    store.member_remove("alice", NOW + 1).unwrap();
    send(&app, bearer_mcp(&team.alice_token, None, MCP_BODY)).await;

    let text = captured.text();
    let value = cookie.split_once('=').map(|(_, value)| value).unwrap_or("");
    for secret in [
        team.alice_token.as_str(),
        team.alice_key.as_str(),
        team.bob_token.as_str(),
        unknown.as_str(),
        wrong_key.as_str(),
        value,
    ] {
        assert!(
            !secret.is_empty() && !text.contains(secret),
            "{secret}: {text}"
        );
        assert!(
            !text.contains(&crate::store::credential_digest(secret)),
            "no digest is logged"
        );
    }
    for needle in ["tdm_", "tdk_", "Bearer", "authorization", "x-tandem-agent"] {
        assert!(
            !text.to_lowercase().contains(&needle.to_lowercase()),
            "{needle}: {text}"
        );
    }
    assert!(!text.contains("tandem_session"), "{text}");
}

// MARK: - Roles in one place (CUT-39)

/// The ux wording of a refused page.
const NO_ACCESS: &str = "You do not have access to this page";
const ONLY_ADMINS: &str = "Only admins can manage people. Ask an admin if you need a change.";

fn issue_key(store: &Store, name: &str) -> String {
    store
        .member_signin_key_issue(name, NOW)
        .unwrap()
        .unwrap()
        .reveal()
        .to_string()
}

fn cookie_get(uri: &str, cookie: &str) -> Request<Body> {
    remote_get(uri, Some(cookie))
}

fn export_rows(store: &Store) -> Vec<crate::store::AccessRow> {
    store
        .access_record()
        .unwrap()
        .into_iter()
        .filter(|row| row.event == "export")
        .collect()
}

#[tokio::test]
async fn every_route_in_the_table_maps_to_a_permission_and_an_unmapped_route_is_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("route-map");

    // Every route `routes()` declares has a line in the guard's map, and every
    // line names a route that exists.
    let source = include_str!("mod.rs");
    let routes_fn = &source[source.find("fn routes()").expect("routes()")
        ..source.find("fn layered(").expect("layered()")];
    let mut declared: Vec<&str> = routes_fn
        .split(".route(")
        .skip(1)
        .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
        .collect();
    declared.sort_unstable();
    declared.dedup();
    let mut mapped: Vec<&str> = guard::ROUTE_NEEDS.iter().map(|(path, _)| *path).collect();
    mapped.sort_unstable();
    mapped.dedup();
    assert_eq!(declared, mapped, "the map names every route and no other");
    for route in &declared {
        assert!(guard::route_need(route).is_some(), "{route}");
    }
    // The export route needs the export permission, the pages the board.
    assert_eq!(
        guard::route_need("/export"),
        Some(guard::Need::Permit(crate::permission::Permission::Export))
    );
    assert_eq!(
        guard::route_need("/board"),
        Some(guard::Need::Permit(crate::permission::Permission::Board))
    );
    assert_eq!(guard::route_need("/health"), Some(guard::Need::Public));
    assert_eq!(guard::route_need("/nowhere"), None);

    // A route added to the router and not to the map is refused, even for the
    // owner on loopback: a later route cannot opt out.
    let app = extra_router(
        routes().route("/nowhere", axum::routing::get(resolved_context)),
        sandbox.store(),
    );
    let captured = Captured::default();
    let _guard = captured.install();
    let (status, _, body) = get(&app, "/nowhere").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(
        captured
            .text()
            .contains("the route has no permission mapped"),
        "{}",
        captured.text()
    );
}

#[tokio::test]
async fn a_member_session_gets_the_403_page_on_export_and_a_token_gets_the_403_on_everything_but_mcp()
 {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("member-403");
    let store = sandbox.store();
    let team = team(&store);
    let bob_key = issue_key(&store, "bob");
    let app = remote_app(store.clone(), TOKEN);
    let bob = sign_in_with(&app, &bob_key).await;

    let (status, headers, body) = send(&app, cookie_get("/export", &bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(body.contains(NO_ACCESS), "{body}");
    assert!(body.contains(ONLY_ADMINS), "{body}");
    assert!(
        body.contains("href=\"/\""),
        "a link back to the board: {body}"
    );
    assert!(!body.contains(&bob), "no credential in the refusal");
    // A member still reaches the board.
    assert_eq!(
        send(&app, cookie_get("/board", &bob)).await.0,
        StatusCode::OK
    );

    // A token, of an admin or a member, is refused on every page route.
    for token in [&team.alice_token, &team.bob_token] {
        for (method, _, uri) in ROUTE_TABLE {
            let (status, _, body) = send(&app, table_request(method, uri, token)).await;
            if matches!(*uri, "/mcp" | "/health") {
                assert_eq!(status, StatusCode::OK, "{method} {uri}: {body}");
            } else {
                assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
                assert!(body.contains(NO_ACCESS), "{method} {uri}: {body}");
            }
        }
    }

    // The /mcp refusal is the JSON-RPC error, in its own wording.
    let response = guard::refuse_access("/mcp", "a test");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let answer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(answer["jsonrpc"], "2.0");
    assert_eq!(
        answer["error"]["message"],
        "A personal token cannot manage people or export the workspace."
    );
    assert!(
        !String::from_utf8_lossy(&bytes).contains(NO_ACCESS),
        "the two wordings are separate"
    );
}

#[tokio::test]
async fn an_admin_session_exports_and_a_member_session_does_not() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("admin-export");
    let store = sandbox.store();
    exportable(&store);
    let team = team(&store);
    let bob_key = issue_key(&store, "bob");
    let app = remote_app(store.clone(), TOKEN);
    let alice = sign_in_with(&app, &team.alice_key).await;
    let bob = sign_in_with(&app, &bob_key).await;

    let (status, headers, bytes) = send_raw(&app, cookie_get("/export", &alice)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/gzip");
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);

    let (status, headers, _) = send_raw(&app, cookie_get("/export", &bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(headers.get(header::CONTENT_DISPOSITION).is_none());

    // The shell draws the control for the admin and not for the member.
    let (_, _, page) = send(&app, cookie_get("/board", &alice)).await;
    assert!(page.contains("Export workspace"));
    let (_, _, page) = send(&app, cookie_get("/board", &bob)).await;
    assert!(!page.contains("Export workspace"));
    assert!(!page.contains("/export"));

    // The owner's bearer still exports.
    let (status, _, _) = send_raw(&app, bearer_get("/export", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_demoted_admins_next_request_and_open_stream_follow_the_new_role() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("demotion");
    let store = sandbox.store();
    exportable(&store);
    let team = team(&store);
    store
        .member_add("zed", crate::store::Role::Admin, NOW)
        .unwrap();
    let app = remote_app(store.clone(), TOKEN);
    let alice = sign_in_with(&app, &team.alice_key).await;
    assert_eq!(
        send_raw(&app, cookie_get("/export", &alice)).await.0,
        StatusCode::OK
    );

    let response = app
        .clone()
        .oneshot(remote_get("/events", Some(&alice)))
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    stream
        .frame()
        .await
        .expect("the first frame")
        .expect("readable");

    // Another admin demotes her: no sign-out, the role is read on each request.
    assert_eq!(
        store.member_set_role_by(
            "alice",
            crate::store::Role::Member,
            NOW + 1,
            &crate::store::AccessActor::member_session("zed"),
        ),
        Ok(true)
    );
    let (status, _, body) = send(&app, cookie_get("/export", &alice)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        send(&app, cookie_get("/board", &alice)).await.0,
        StatusCode::OK,
        "she is still a member"
    );

    // The stream she opened as an admin ends within the bound.
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(frame) = stream.frame().await {
            if frame.is_err() {
                break;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream closes within the bound");
}

#[tokio::test]
async fn the_mcp_tool_list_holds_no_export_or_people_tool_for_any_principal() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("mcp-tools");
    let store = sandbox.store();
    let team = team(&store);
    let app = remote_app(store, TOKEN);
    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;

    let mut seen = Vec::new();
    for token in [TOKEN, &team.alice_token, &team.bob_token] {
        let (status, _, body) = send(&app, bearer_mcp(token, None, list)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let answer: serde_json::Value = serde_json::from_str(&body).expect("one JSON frame");
        let mut names: Vec<String> = answer["result"]["tools"]
            .as_array()
            .expect("a tool list")
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        seen.push(names);
    }
    assert_eq!(seen[0], seen[1], "an admin's token sees the owner's list");
    assert_eq!(seen[1], seen[2], "a member's token sees the same list");
    let mut pinned = vec![
        "doctor_report",
        "label_add",
        "label_delete",
        "label_list",
        "label_move",
        "label_rename",
        "note_backlinks",
        "note_create",
        "note_create_folder",
        "note_edit",
        "note_health",
        "note_move",
        "note_read",
        "note_search",
        "note_tree",
        "note_write",
        "project_add",
        "project_delete",
        "project_list",
        "project_rename",
        "ticket_add",
        "ticket_archive",
        "ticket_block",
        "ticket_comment_add",
        "ticket_comment_list",
        "ticket_complete",
        "ticket_list",
        "ticket_move",
        "ticket_set",
        "ticket_show",
        "ticket_unblock",
    ];
    pinned.sort_unstable();
    assert_eq!(
        seen[0], pinned,
        "the list is pinned: a new tool is a decision"
    );
    for name in &seen[0] {
        for word in [
            "export", "import", "member", "people", "role", "access", "signin",
        ] {
            assert!(!name.contains(word), "{name} names {word}");
        }
    }
}

#[tokio::test]
async fn a_board_export_writes_one_access_record_row_and_a_cli_export_writes_none() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("export-record");
    let store = sandbox.store();
    exportable(&store);
    let team = team(&store);
    let app = remote_app(store.clone(), TOKEN);
    let alice = sign_in_with(&app, &team.alice_key).await;
    let before = store.access_record().unwrap().len();
    let revision = store.revision().unwrap();

    let (status, _, _) = send_raw(&app, cookie_get("/export", &alice)).await;
    assert_eq!(status, StatusCode::OK);
    let rows = export_rows(&store);
    assert_eq!(rows.len(), 1, "one export, one row");
    assert_eq!(
        (
            rows[0].actor.as_str(),
            rows[0].credential.as_str(),
            rows[0].door.as_deref()
        ),
        ("alice", "member-session", Some("board"))
    );
    assert_eq!(store.access_record().unwrap().len(), before + 1);
    assert_eq!(store.revision().unwrap(), revision, "no revision bump");

    // The owner's bearer is recorded as the owner.
    let (status, _, _) = send_raw(&app, bearer_get("/export", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    let rows = export_rows(&store);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].actor, "owner");
    assert_eq!(rows[1].credential, "owner");

    // The CLI door asks the same check, opens the home directly and writes the
    // log line only.
    let principal = crate::archive::Principal {
        access: crate::archive::Access::Local,
        actor: Actor::Human,
    };
    crate::archive::start_export(&principal, crate::archive::Door::Cli).expect("allowed");
    let mut sink = Vec::new();
    crate::archive::export(store.home(), &mut sink).expect("the CLI export runs");
    assert!(!sink.is_empty());
    assert_eq!(export_rows(&store).len(), 2, "the CLI door writes no row");
}

#[tokio::test]
async fn solo_modes_still_export_as_before() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("solo-export");
    let store = sandbox.store();
    exportable(&store);

    // Loopback.
    let loopback = router(context(store.clone()));
    let (status, _, bytes) = send_raw(&loopback, export_get(&[])).await;
    assert_eq!((status, &bytes[..2]), (StatusCode::OK, &[0x1f, 0x8b][..]));
    let (_, _, page) = get(&loopback, "/board").await;
    assert!(page.contains("Export workspace"));

    // `--allow-unauthenticated`.
    let open = router(SessionContext {
        access: Access::open(7700),
        ..context(store.clone())
    });
    let request = Request::builder()
        .uri("/export")
        .header(header::HOST, REMOTE_HOST)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send_raw(&open, request).await.0, StatusCode::OK);

    // A lone TANDEM_TOKEN, by bearer and by the owner's browser session.
    let token = remote_app(store.clone(), TOKEN);
    assert_eq!(
        send_raw(&token, bearer_get("/export", TOKEN)).await.0,
        StatusCode::OK
    );
    let cookie = sign_in(&token).await;
    assert_eq!(
        send_raw(&token, cookie_get("/export", &cookie)).await.0,
        StatusCode::OK
    );
    let (_, _, page) = send(&token, cookie_get("/board", &cookie)).await;
    assert!(page.contains("Export workspace"));
}

#[tokio::test]
async fn a_refused_act_logs_its_reason_and_no_value_and_writes_no_row() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("refused-no-row");
    let store = sandbox.store();
    exportable(&store);
    let team = team(&store);
    let bob_key = issue_key(&store, "bob");
    let app = remote_app(store.clone(), TOKEN);
    let bob = sign_in_with(&app, &bob_key).await;
    let before = store.access_record().unwrap();
    let captured = Captured::default();
    let _guard = captured.install();

    // A member session on export, and a token on a page.
    assert_eq!(
        send_raw(&app, cookie_get("/export", &bob)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, bearer_get("/export", &team.alice_token)).await.0,
        StatusCode::FORBIDDEN
    );

    let text = captured.text();
    assert!(
        text.contains("the credential does not hold the export permission"),
        "{text}"
    );
    assert!(
        text.contains("a personal token reaches /mcp and /health only"),
        "{text}"
    );
    let values: [&str; 5] = [&bob, &bob_key, &team.alice_token, &team.alice_key, TOKEN];
    for value in values {
        assert!(!text.contains(value), "no value in the log: {text}");
    }
    assert!(!text.contains("export started"), "no export began");
    assert_eq!(
        store.access_record().unwrap(),
        before,
        "a refused attempt writes no row"
    );
}

// MARK: - The operator interface (CUT-40)

const OPERATOR_KEY: &str = "operator-key-operator-key-operator-key-0001";
const OPERATOR_NEXT: &str = "operator-next-operator-next-operator-next-02";

fn operator_app(store: Store, key: &str, next: Option<&str>) -> Router {
    router(SessionContext {
        access: Access::token(7700, TOKEN).with_operator_keys(key, next),
        ..context(store)
    })
}

fn operator_request(
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, REMOTE_HOST);
    if let Some(key) = key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .expect("the request builds")
}

async fn operator_send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let (status, _, text) =
        send(app, operator_request(method, uri, Some(OPERATOR_KEY), body)).await;
    let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    (status, value)
}

fn operator_rows(store: &Store) -> Vec<crate::store::AccessRow> {
    store
        .access_record()
        .unwrap()
        .into_iter()
        .filter(|row| row.actor == "operator")
        .collect()
}

#[tokio::test]
async fn the_operator_key_adds_changes_the_role_of_removes_and_counts_members() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-acts");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    let (status, body) = operator_send(&app, "GET", "/operator/v1/members/count", None).await;
    assert_eq!(
        (status, body),
        (StatusCode::OK, serde_json::json!({"members": 0}))
    );

    let (status, body) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(r#"{"name":"ada","role":"admin"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "ada");
    assert_eq!(body["role"], "admin");
    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(r#"{"name":"bo","role":"member"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, body) = operator_send(&app, "GET", "/operator/v1/members/count", None).await;
    assert_eq!(body, serde_json::json!({"members": 2}));

    let (status, body) = operator_send(
        &app,
        "PATCH",
        "/operator/v1/members/bo",
        Some(r#"{"role":"admin"}"#),
    )
    .await;
    assert_eq!(
        (status, &body["role"]),
        (StatusCode::OK, &serde_json::json!("admin"))
    );
    assert_eq!(
        store.member_find("bo").unwrap().map(|member| member.role),
        Some(crate::store::Role::Admin)
    );
    let (status, _) = operator_send(
        &app,
        "PATCH",
        "/operator/v1/members/nobody",
        Some(r#"{"role":"member"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = operator_send(&app, "DELETE", "/operator/v1/members/bo", None).await;
    assert_eq!(
        (status, &body["removed"]),
        (StatusCode::OK, &serde_json::json!(true))
    );
    assert_eq!(store.member_find("bo").unwrap(), None);
    let (status, _) = operator_send(&app, "DELETE", "/operator/v1/members/bo", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a removed member is gone");
    let (_, body) = operator_send(&app, "GET", "/operator/v1/members/count", None).await;
    assert_eq!(body, serde_json::json!({"members": 1}));
}

#[tokio::test]
async fn the_count_is_a_number_and_nothing_else() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-count");
    let store = sandbox.store();
    let team = team(&store);
    let app = operator_app(store.clone(), OPERATOR_KEY, None);
    let before = store.access_record().unwrap();

    let (status, headers, body) = send(
        &app,
        operator_request(
            "GET",
            "/operator/v1/members/count",
            Some(OPERATOR_KEY),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"members":2}"#, "a number and nothing else");
    assert!(
        !body.contains("alice") && !body.contains(&team.alice_token),
        "{body}"
    );
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "no CORS header"
    );
    assert_eq!(
        store.access_record().unwrap(),
        before,
        "a count is read-only and writes no row"
    );
}

#[tokio::test]
async fn the_add_response_carries_no_secret() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-no-secret");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    let (status, headers, body) = send(
        &app,
        operator_request(
            "POST",
            "/operator/v1/members",
            Some(OPERATOR_KEY),
            Some(r#"{"name":"ada","role":"admin"}"#),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["id", "name", "role"], "{body}");
    for needle in ["tdm_", "tdk_", "token", "key", OPERATOR_KEY] {
        assert!(!body.contains(needle), "{needle}: {body}");
    }
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no session is opened for the new member"
    );
    assert!(
        store.member_tokens("ada").unwrap().is_empty(),
        "no credential was issued"
    );
}

#[tokio::test]
async fn the_operator_key_is_refused_on_every_board_events_static_export_and_mcp_route() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-deny");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    let mut checked = 0;
    for (route, need) in guard::ROUTE_NEEDS {
        let in_family = route.starts_with(guard::OPERATOR_PREFIX);
        assert_eq!(
            *need == guard::Need::Operator,
            in_family,
            "{route}: only the family takes the operator key, and the whole family does"
        );
        if in_family {
            continue;
        }
        let path = route
            .replace("{id}", "x")
            .replace("{name}", "x")
            .replace("{*path}", "x.css");
        for method in ["GET", "POST", "DELETE"] {
            let body = (method == "POST").then_some("{}");
            let (status, _, text) = send(
                &app,
                operator_request(method, &path, Some(OPERATOR_KEY), body),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {path} is not the operator's: {text}"
            );
            checked += 1;
        }
    }
    assert!(checked > 60, "the table was walked: {checked}");
    assert_eq!(
        operator_send(&app, "GET", "/nowhere", None).await.0,
        StatusCode::UNAUTHORIZED,
        "a path no route names is refused too"
    );
    assert!(store.access_record().unwrap().is_empty(), "no row");
}

#[tokio::test]
async fn a_member_token_a_member_session_and_the_owner_token_never_open_the_operator_routes() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-others");
    let store = sandbox.store();
    let team = team(&store);
    let app = operator_app(store.clone(), OPERATOR_KEY, None);
    let cookie = sign_in_with(&app, &team.alice_key).await;
    let before = store.access_record().unwrap();

    let routes = [
        ("GET", "/operator/v1/members/count", None),
        (
            "POST",
            "/operator/v1/members",
            Some(r#"{"name":"eve","role":"admin"}"#),
        ),
        (
            "PATCH",
            "/operator/v1/members/bob",
            Some(r#"{"role":"admin"}"#),
        ),
        ("DELETE", "/operator/v1/members/bob", None),
    ];
    for (method, uri, body) in routes {
        for credential in [
            Some(team.alice_token.as_str()),
            Some(team.bob_token.as_str()),
            Some(team.alice_key.as_str()),
            Some(TOKEN),
            None,
        ] {
            let (status, _, text) =
                send(&app, operator_request(method, uri, credential, body)).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {text}");
        }
        let mut request = operator_request(method, uri, None, body);
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let (status, _, text) = send(&app, request).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "a session: {method} {uri}: {text}"
        );
    }
    assert_eq!(store.members().unwrap().len(), 2, "nothing changed");
    assert_eq!(store.access_record().unwrap(), before);
}

#[tokio::test]
async fn the_operator_cannot_remove_or_demote_the_last_admin() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-last-admin");
    let store = sandbox.store();
    let _team = team(&store);
    let app = operator_app(store.clone(), OPERATOR_KEY, None);
    let before = store.access_record().unwrap();

    let (status, body) = operator_send(&app, "DELETE", "/operator/v1/members/alice", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"].as_str().unwrap().contains("last admin"));
    let (status, _) = operator_send(
        &app,
        "PATCH",
        "/operator/v1/members/alice",
        Some(r#"{"role":"member"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        store
            .member_find("alice")
            .unwrap()
            .map(|member| member.role),
        Some(crate::store::Role::Admin)
    );
    assert_eq!(
        store.access_record().unwrap(),
        before,
        "a refusal writes no row"
    );

    // With a second admin the first may go.
    let (status, _) = operator_send(
        &app,
        "PATCH",
        "/operator/v1/members/bob",
        Some(r#"{"role":"admin"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = operator_send(&app, "DELETE", "/operator/v1/members/alice", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn every_operator_act_writes_one_access_record_row_as_operator_in_the_same_transaction() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-rows");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    let acts: [(&str, &str, Option<&str>, &str, &str); 3] = [
        (
            "POST",
            "/operator/v1/members",
            Some(r#"{"name":"ada","role":"admin"}"#),
            "member-added",
            "ada",
        ),
        (
            "PATCH",
            "/operator/v1/members/ada",
            Some(r#"{"role":"member"}"#),
            "member-role-changed",
            "ada",
        ),
        (
            "DELETE",
            "/operator/v1/members/ada",
            None,
            "member-removed",
            "ada",
        ),
    ];
    // A second admin so the first can be demoted and removed.
    store
        .member_add("zed", crate::store::Role::Admin, NOW)
        .unwrap();
    let mut seen = 0;
    for (method, uri, body, event, target) in acts {
        let (status, _) = operator_send(&app, method, uri, body).await;
        assert!(status.is_success(), "{method} {uri}: {status}");
        let rows = operator_rows(&store);
        assert_eq!(rows.len(), seen + 1, "one row per act: {rows:?}");
        let row = rows.last().unwrap();
        assert_eq!(row.actor, "operator");
        assert_eq!(row.credential, "operator");
        assert_eq!(row.event, event);
        assert_eq!(row.target.as_deref(), Some(target));
        seen += 1;
    }
    // The change and its row are one write: a refused act leaves neither.
    let (status, _) = operator_send(&app, "DELETE", "/operator/v1/members/zed", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(operator_rows(&store).len(), seen);
    assert!(store.member_find("zed").unwrap().is_some());
    // A rejected request writes no row.
    send(
        &app,
        operator_request(
            "DELETE",
            "/operator/v1/members/zed",
            Some("wrong-key"),
            None,
        ),
    )
    .await;
    assert_eq!(operator_rows(&store).len(), seen);
}

#[tokio::test]
async fn reserved_and_duplicate_and_malformed_names_are_refused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-names");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);
    let add = |name: &str, role: &str| format!(r#"{{"name":"{name}","role":"{role}"}}"#);

    for name in [
        "count",
        "operator",
        "human",
        "owner",
        "Ada",
        "has space",
        "",
        &"a".repeat(65),
    ] {
        let (status, body) = operator_send(
            &app,
            "POST",
            "/operator/v1/members",
            Some(&add(name, "admin")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name:?}: {body}");
    }
    assert!(
        store.members().unwrap().is_empty(),
        "a refused name, `count` included, writes no row"
    );
    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(&add("ada", "admin")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(&add("ada", "admin")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a duplicate name");
    store
        .member_add("zed", crate::store::Role::Admin, NOW)
        .unwrap();
    operator_send(&app, "DELETE", "/operator/v1/members/ada", None).await;
    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(&add("ada", "member")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a name is never reused");

    for body in [
        "not json",
        r#"{"name":"cy"}"#,
        r#"{"name":"cy","role":"owner"}"#,
        r#"{"name":"cy","role":"member","extra":1}"#,
        r#"{"name":7,"role":"member"}"#,
    ] {
        let (status, _) = operator_send(&app, "POST", "/operator/v1/members", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, _, _) = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/operator/v1/members")
            .header(header::HOST, REMOTE_HOST)
            .header(header::AUTHORIZATION, format!("Bearer {OPERATOR_KEY}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("name=cy&role=member"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(store.members().unwrap().len(), 1, "only zed is a member");
}

#[tokio::test]
async fn the_operators_first_member_must_be_an_admin() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-first-admin");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    let (status, body) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(r#"{"name":"bo","role":"member"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("first member must be an admin")
    );
    assert!(store.members().unwrap().is_empty());
    assert!(
        store.access_record().unwrap().is_empty(),
        "a refusal writes no row"
    );
    assert!(
        !store.team_mode().unwrap(),
        "the home stays out of team mode"
    );

    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(r#"{"name":"ada","role":"admin"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = operator_send(
        &app,
        "POST",
        "/operator/v1/members",
        Some(r#"{"name":"bo","role":"member"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "a member may follow an admin");
}

#[tokio::test]
async fn without_an_operator_key_the_family_answers_404() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-off");
    let store = sandbox.store();
    let app = remote_app(store.clone(), TOKEN);

    for (method, uri) in [
        ("GET", "/operator/v1/members/count"),
        ("POST", "/operator/v1/members"),
        ("PATCH", "/operator/v1/members/ada"),
        ("DELETE", "/operator/v1/members/ada"),
    ] {
        for credential in [None, Some(OPERATOR_KEY), Some(TOKEN)] {
            let body = (method != "GET" && method != "DELETE").then_some(r#"{"role":"admin"}"#);
            let (status, _, _) = send(&app, operator_request(method, uri, credential, body)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
        }
    }
    let loopback = router(context(sandbox.store()));
    let (status, _, _) = send(
        &loopback,
        Request::builder()
            .uri("/operator/v1/members/count")
            .header(header::HOST, "127.0.0.1:7700")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_next_key_is_accepted_beside_the_current_one() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-next");
    let store = sandbox.store();
    let app = operator_app(store, OPERATOR_KEY, Some(OPERATOR_NEXT));

    for key in [OPERATOR_KEY, OPERATOR_NEXT] {
        let (status, _, body) = send(
            &app,
            operator_request("GET", "/operator/v1/members/count", Some(key), None),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, _, _) = send(
        &app,
        operator_request(
            "GET",
            "/operator/v1/members/count",
            Some("a-third-key"),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // The next key is the operator's too, so it opens nothing else.
    let (status, _, _) = send(&app, bearer_get("/board", OPERATOR_NEXT)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_operator_request_with_an_origin_header_or_an_agent_header_is_refused_or_ignored() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-headers");
    let store = sandbox.store();
    let app = operator_app(store.clone(), OPERATOR_KEY, None);

    for origin in [REMOTE_ORIGIN, "null", "http://127.0.0.1:7700"] {
        let mut request = operator_request(
            "POST",
            "/operator/v1/members",
            Some(OPERATOR_KEY),
            Some(r#"{"name":"ada","role":"admin"}"#),
        );
        request
            .headers_mut()
            .insert(header::ORIGIN, origin.parse().unwrap());
        let (status, headers, _) = send(&app, request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
        assert!(!headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }
    assert!(store.members().unwrap().is_empty(), "nothing was added");

    // The agent header is ignored, valid or not: the actor stays the operator.
    for agent in ["bob", "human", "NOT VALID!"] {
        let mut request = operator_request(
            "POST",
            "/operator/v1/members",
            Some(OPERATOR_KEY),
            Some(&format!(r#"{{"name":"m-{}","role":"admin"}}"#, agent.len())),
        );
        request
            .headers_mut()
            .insert("x-tandem-agent", agent.parse().unwrap());
        let (status, _, body) = send(&app, request).await;
        assert_eq!(status, StatusCode::CREATED, "{agent}: {body}");
    }
    let rows = operator_rows(&store);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.actor == "operator"));
}

#[tokio::test]
async fn the_operator_key_is_never_logged() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("operator-log");
    let store = sandbox.store();
    let app = operator_app(store, OPERATOR_KEY, Some(OPERATOR_NEXT));
    let captured = Captured::default();
    let _guard = captured.install();
    let wrong = "wrong-operator-key-wrong-operator-key";

    for key in [OPERATOR_KEY, OPERATOR_NEXT, wrong] {
        send(
            &app,
            operator_request(
                "POST",
                "/operator/v1/members",
                Some(key),
                Some(r#"{"name":"ada","role":"admin"}"#),
            ),
        )
        .await;
        send(&app, bearer_get("/board", key)).await;
        send(
            &app,
            operator_request("GET", "/operator/v1/members/count", Some(key), None),
        )
        .await;
    }
    let mut origin = operator_request(
        "GET",
        "/operator/v1/members/count",
        Some(OPERATOR_KEY),
        None,
    );
    origin
        .headers_mut()
        .insert(header::ORIGIN, "https://evil.example".parse().unwrap());
    send(&app, origin).await;

    let text = captured.text();
    assert!(
        text.contains("operator"),
        "the refusals log a reason: {text}"
    );
    for secret in [OPERATOR_KEY, OPERATOR_NEXT, wrong, "evil.example"] {
        assert!(!text.contains(secret), "{secret}: {text}");
        assert!(
            !text.contains(&crate::store::credential_digest(secret)),
            "digest"
        );
    }
    assert!(!text.to_lowercase().contains("bearer"), "{text}");
}

// MARK: - The Personal tokens screen (CUT-41)

const LAPTOP: &str = "Claude Code on my laptop";

/// A team home behind the owner token: `alice` (an admin) and `bob` (a member)
/// with no tokens yet, and `bob` signed in.
struct TokenScreen {
    sandbox: Sandbox,
    store: Store,
    app: Router,
    bob: String,
    bob_key: String,
}

async fn token_screen(name: &str) -> TokenScreen {
    let sandbox = Sandbox::new(name);
    let store = sandbox.store();
    store
        .member_add("alice", crate::store::Role::Admin, NOW)
        .unwrap();
    store
        .member_add("bob", crate::store::Role::Member, NOW)
        .unwrap();
    let app = remote_app(store.clone(), TOKEN);
    let bob_key = issue_key(&store, "bob");
    let bob = sign_in_with(&app, &bob_key).await;
    TokenScreen {
        sandbox,
        store,
        app,
        bob,
        bob_key,
    }
}

/// A token for `member`, made straight through the store: its value.
fn issued(store: &Store, member: &str, label: &str) -> String {
    store
        .member_token_issue(member, label, NOW)
        .unwrap()
        .unwrap()
        .reveal()
        .to_string()
}

fn token_post(uri: &str, fields: &[(&str, &str)], cookie: &str) -> Request<Body> {
    let body = serde_urlencoded::to_string(fields).expect("the form encodes");
    remote_form(uri, &body, Some(REMOTE_ORIGIN), Some(cookie))
}

fn create_form(key: &str, name: &str, cookie: &str) -> Request<Body> {
    token_post("/tokens", &[("key", key), ("name", name)], cookie)
}

/// The idempotence key the page's form carries.
fn form_key(page: &str) -> String {
    let marker = "name=\"key\" value=\"";
    let at = page.find(marker).expect("the form carries a key") + marker.len();
    page[at..at + 32].to_string()
}

/// The token a show page holds.
fn shown_token(page: &str) -> String {
    let at = page.find("value=\"tdm_").expect("the page shows a token") + "value=\"".len();
    page[at..at + 47].to_string()
}

/// The page inside the shell's `<main>`.
fn main_of(page: &str) -> &str {
    &page[page.find("<main").expect("the shell has a main")..]
}

fn token_count(store: &Store, member: &str) -> usize {
    store
        .member_tokens(member)
        .unwrap()
        .iter()
        .filter(|token| token.revoked_at.is_none())
        .count()
}

fn token_rows(store: &Store) -> Vec<crate::store::AccessRow> {
    store
        .access_record()
        .unwrap()
        .into_iter()
        .filter(|row| row.event.starts_with("token-"))
        .collect()
}

#[tokio::test]
async fn creating_a_token_shows_it_once_with_no_store_and_a_later_list_never_shows_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-show-once").await;

    let (status, headers, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        !page.contains("project-switch"),
        "the tokens screen draws no project switcher: {page}"
    );
    let main = main_of(&page);
    assert_eq!(main.matches("<h1").count(), 1, "one h1 in the page");
    assert!(main.contains(">Personal tokens</h1>"));
    assert!(main.contains("href=\"/board\""), "a link back to the board");
    assert!(main.contains(
        "A personal token lets a tool you run, such as Claude Code, work on the board as you. It cannot export the workspace or manage people."
    ));
    assert!(main.contains("If you lose a token, revoke it and make a new one."));
    assert!(
        main.contains("You have no personal tokens. Create one to let a tool work as you."),
        "the empty state"
    );
    assert!(
        main.contains("Claude Code on my laptop"),
        "a visible example"
    );
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|value| value.to_str().unwrap()),
        Some("no-store")
    );
    let key = form_key(&page);

    // The response to the create is the page that shows the value.
    let (status, headers, shown) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    assert_eq!(status, StatusCode::OK, "not a redirect: {shown}");
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|value| value.to_str().unwrap()),
        Some("no-store")
    );
    assert!(headers.get(header::LOCATION).is_none());
    let token = shown_token(&shown);
    assert!(shown.contains("readonly"), "a read-only selectable field");
    assert!(shown.contains(">Copy token</button>"));
    assert!(shown.contains("Copy it now. You will not see it again."));
    assert!(shown.contains(LAPTOP));
    assert_eq!(
        shown.matches(&token).count(),
        1,
        "the value is in one place"
    );
    let made = screen.store.member_by_token(&token, NOW, false).unwrap();
    assert_eq!(made.map(|member| member.name), Some("bob".to_string()));

    // A reload of the list, and the list again, never carry it.
    let (status, headers, list) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.contains(LAPTOP));
    assert!(!list.contains(&token) && !list.contains("tdm_"), "{list}");
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|value| value.to_str().unwrap()),
        Some("no-store")
    );
    assert!(!list.contains("You have no personal tokens"));
}

#[tokio::test]
async fn a_double_submit_makes_one_token() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-double").await;
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let key = form_key(&page);

    let (first, _, shown) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    assert_eq!(first, StatusCode::OK);
    assert!(shown.contains("tdm_"));
    let (second, headers, body) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    assert_eq!(second, StatusCode::SEE_OTHER, "{body}");
    assert!(!body.contains("tdm_"), "a second token is never shown");
    assert_eq!(
        headers
            .get(header::LOCATION)
            .map(|value| value.to_str().unwrap()),
        Some("/tokens?notice=duplicate")
    );
    assert_eq!(token_count(&screen.store, "bob"), 1, "one token, not two");

    let (_, _, list) = send(
        &screen.app,
        cookie_get("/tokens?notice=duplicate", &screen.bob),
    )
    .await;
    assert!(list.contains("That token was already made."), "{list}");
    assert_eq!(token_rows(&screen.store).len(), 1);
}

#[tokio::test]
async fn a_failed_create_leaves_no_token() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-failed").await;
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let key = form_key(&page);
    let database = screen.store.home().directory().join("tandem.db");

    // The record cannot be written, so the act does not happen.
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER no_room BEFORE INSERT ON access_record
             WHEN NEW.event = 'token-created'
             BEGIN SELECT RAISE(ABORT, 'no room'); END;",
        )
        .unwrap();
    let (status, _, body) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("Could not save. Nothing changed. Try again."));
    assert!(!body.contains("tdm_"));
    assert!(
        body.contains(&format!("value=\"{LAPTOP}\"")),
        "the input is kept"
    );
    assert_eq!(screen.store.member_tokens("bob").unwrap().len(), 0);

    // The same form, submitted again, is not a duplicate: nothing was made.
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute_batch("DROP TRIGGER no_room")
        .unwrap();
    let (status, _, shown) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert!(shown.contains("tdm_"));
    assert_eq!(token_count(&screen.store, "bob"), 1);
}

#[tokio::test]
async fn the_token_list_shows_name_created_and_last_used_and_never_a_value_or_digest() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-list").await;
    let used = screen
        .store
        .member_token_issue("bob", "used one", NOW)
        .unwrap()
        .unwrap()
        .reveal()
        .to_string();
    let unused = screen
        .store
        .member_token_issue("bob", "unused one", NOW)
        .unwrap()
        .unwrap()
        .reveal()
        .to_string();
    // Touch the first: its last use is two days on.
    assert!(
        screen
            .store
            .member_by_token(&used, NOW + 2 * 86_400, true)
            .unwrap()
            .is_some()
    );

    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let main = main_of(&page);
    assert!(main.contains("used one") && main.contains("unused one"));
    assert!(main.contains("2027-01-15"), "created: {main}");
    assert!(main.contains("2027-01-17"), "last used: {main}");
    assert_eq!(main.matches("Never used").count(), 1);
    assert_eq!(main.matches(">Revoke token<").count(), 2, "{main}");
    for secret in [
        used.as_str(),
        unused.as_str(),
        crate::store::credential_digest(&used).as_str(),
        crate::store::credential_digest(&unused).as_str(),
        "tdm_",
    ] {
        assert!(!page.contains(secret), "the list holds no {secret}");
    }
}

#[tokio::test]
async fn revoking_a_token_goes_through_a_confirm_page_and_the_old_token_then_gets_the_revoked_401()
{
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-revoke").await;
    let value = screen
        .store
        .member_token_issue("bob", LAPTOP, NOW)
        .unwrap()
        .unwrap()
        .reveal()
        .to_string();
    let id = screen.store.member_tokens("bob").unwrap()[0].id;
    let (status, _, _) = send(&screen.app, bearer_mcp(&value, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK, "the tool works before");

    // A link, not a bare POST: the confirm page names the token and its
    // cancel is the default focus.
    let (_, _, list) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    assert!(
        list.contains(&format!("href=\"/tokens/{id}/revoke\"")),
        "{list}"
    );
    let (status, headers, confirm) = send(
        &screen.app,
        cookie_get(&format!("/tokens/{id}/revoke"), &screen.bob),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|value| value.to_str().unwrap()),
        Some("no-store")
    );
    assert!(confirm.contains(
        "Revoke 'Claude Code on my laptop'? Tools using it stop working at once. This cannot be undone."
    ));
    let cancel = confirm
        .find("href=\"/tokens\" autofocus")
        .expect("cancel is focused");
    let submit = confirm
        .find(">Revoke token</button>")
        .expect("the revoke button");
    assert!(cancel < submit, "cancel comes first and holds the focus");
    assert_eq!(
        token_count(&screen.store, "bob"),
        1,
        "a GET revokes nothing"
    );

    let (status, headers, _) = send(
        &screen.app,
        token_post(&format!("/tokens/{id}/revoke"), &[], &screen.bob),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(header::LOCATION)
            .map(|value| value.to_str().unwrap()),
        Some("/tokens?notice=revoked")
    );
    let (_, _, after) = send(
        &screen.app,
        cookie_get("/tokens?notice=revoked", &screen.bob),
    )
    .await;
    assert!(
        after.contains("role=\"status\" aria-live=\"polite\" tabindex=\"-1\" autofocus data-autofocus>Token revoked.</p>"),
        "the status line holds the focus: {after}"
    );
    assert!(after.contains("You have no personal tokens."));

    let (status, _, body) = send(&screen.app, bearer_mcp(&value, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, "This token has been revoked or does not exist.\n");
}

#[tokio::test]
async fn a_stale_confirm_answers_the_end_state() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-stale").await;
    screen
        .store
        .member_token_issue("bob", LAPTOP, NOW)
        .unwrap()
        .unwrap();
    let id = screen.store.member_tokens("bob").unwrap()[0].id;
    let uri = format!("/tokens/{id}/revoke");
    assert!(screen.store.member_token_revoke("bob", id, NOW).unwrap());

    let (status, _, page) = send(&screen.app, cookie_get(&uri, &screen.bob)).await;
    assert_eq!(status, StatusCode::OK, "an end state, not an error page");
    assert!(page.contains("Already revoked."), "{page}");
    assert!(!page.contains(">Revoke token</button>"));

    let (status, headers, _) = send(&screen.app, token_post(&uri, &[], &screen.bob)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(header::LOCATION)
            .map(|value| value.to_str().unwrap()),
        Some("/tokens?notice=already-revoked")
    );
    let (_, _, list) = send(
        &screen.app,
        cookie_get("/tokens?notice=already-revoked", &screen.bob),
    )
    .await;
    assert!(list.contains(">Already revoked.</p>"), "{list}");
    // One row for the one revocation.
    assert_eq!(
        token_rows(&screen.store)
            .iter()
            .filter(|row| row.event == "token-revoked")
            .count(),
        1
    );
}

#[tokio::test]
async fn a_member_sees_and_revokes_only_their_own_tokens() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-own").await;
    let alice_token = issued(&screen.store, "alice", "alice only");
    issued(&screen.store, "bob", "bob only");
    let alices = screen
        .store
        .member_tokens("alice")
        .unwrap()
        .iter()
        .map(|token| token.id)
        .collect::<Vec<_>>();

    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    assert!(
        page.contains("bob only") && !page.contains("alice only"),
        "{page}"
    );
    let alice = sign_in_with(&screen.app, &issue_key(&screen.store, "alice")).await;
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &alice)).await;
    assert!(
        page.contains("alice only") && !page.contains("bob only"),
        "{page}"
    );

    // Another member's token is not found, never revoked, by id.
    for id in &alices {
        let uri = format!("/tokens/{id}/revoke");
        let (status, _, body) = send(&screen.app, cookie_get(&uri, &screen.bob)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        let (status, _, _) = send(&screen.app, token_post(&uri, &[], &screen.bob)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(
        token_count(&screen.store, "alice"),
        1,
        "alice's are untouched"
    );
    let (status, _, _) = send(&screen.app, bearer_mcp(&alice_token, None, MCP_BODY)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = send(&screen.app, cookie_get("/tokens/abc/revoke", &screen.bob)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a path that is no id");
}

#[tokio::test]
async fn a_personal_token_cannot_create_or_revoke_tokens() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-by-token").await;
    let bob_token = issued(&screen.store, "bob", "bob's tool");
    let alice_token = issued(&screen.store, "alice", "alice's tool");
    let id = screen.store.member_tokens("bob").unwrap()[0].id;
    let before = screen.store.access_record().unwrap().len();

    for (method, uri, body) in [
        ("GET", "/tokens".to_string(), String::new()),
        (
            "POST",
            "/tokens".to_string(),
            format!("key={}&name=sneaky", "a".repeat(32)),
        ),
        ("GET", format!("/tokens/{id}/revoke"), String::new()),
        ("POST", format!("/tokens/{id}/revoke"), String::new()),
    ] {
        for token in [&bob_token, &alice_token] {
            let mut builder = Request::builder()
                .method(method)
                .uri(&uri)
                .header(header::HOST, REMOTE_HOST)
                .header(header::AUTHORIZATION, format!("Bearer {token}"));
            if method == "POST" {
                builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            }
            let request = builder.body(Body::from(body.clone())).unwrap();
            let (status, _, page) = send(&screen.app, request).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {page}");
            assert!(!page.contains("tdm_"));
        }
    }
    assert_eq!(
        token_count(&screen.store, "bob"),
        1,
        "nothing was made or revoked"
    );
    assert_eq!(screen.store.access_record().unwrap().len(), before);
}

#[tokio::test]
async fn the_twenty_first_token_is_refused_with_a_sentence() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-limit").await;
    for index in 0..20 {
        screen
            .store
            .member_token_issue("bob", &format!("token {index}"), NOW)
            .unwrap()
            .unwrap();
    }
    assert_eq!(token_count(&screen.store, "bob"), 20);

    let sentence =
        "You have 20 personal tokens, the most one member can hold. Revoke one to make another.";
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    assert!(page.contains(sentence), "{page}");
    assert!(!page.contains("action=\"/tokens\""), "no form at the limit");

    let (status, _, body) = send(
        &screen.app,
        create_form(&"b".repeat(32), "one too many", &screen.bob),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains(sentence) && !body.contains("tdm_"), "{body}");
    assert_eq!(token_count(&screen.store, "bob"), 20);
}

#[tokio::test]
async fn an_empty_name_a_long_name_and_a_wrong_origin_are_refused_and_keep_the_input() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-names").await;
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let key = form_key(&page);

    let long = "x".repeat(61);
    for (name, sentence) in [
        ("   ", "Enter a name for the token."),
        (long.as_str(), "Use 60 characters or fewer."),
        ("two\nlines", "Use one line of plain text."),
    ] {
        let (status, _, body) = send(&screen.app, create_form(&key, name, &screen.bob)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name:?}: {body}");
        assert!(body.contains(sentence), "{name:?}: {body}");
        assert!(body.contains("aria-invalid=\"true\""), "tied to the field");
        assert!(
            body.contains(&format!("value=\"{name}\"")),
            "the input is kept"
        );
        assert!(!body.contains("tdm_"));
        assert_eq!(form_key(&body), key, "the key is not spent by a refusal");
    }
    // The sixty-character name passes: the bound is on both sides.
    let (status, _, _) = send(&screen.app, create_form(&key, &"y".repeat(60), &screen.bob)).await;
    assert_eq!(status, StatusCode::OK);

    // Another origin, and no origin on a cookie write, are refused outright.
    let before = token_count(&screen.store, "bob");
    let fresh = "c".repeat(32);
    for origin in [Some("https://evil.example"), Some("null"), None] {
        let body =
            serde_urlencoded::to_string([("key", fresh.as_str()), ("name", LAPTOP)]).unwrap();
        let (status, _, page) = send(
            &screen.app,
            remote_form("/tokens", &body, origin, Some(&screen.bob)),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin:?}: {page}");
        assert!(!page.contains("tdm_"));
    }
    assert_eq!(token_count(&screen.store, "bob"), before);
}

#[tokio::test]
async fn token_create_and_revoke_each_write_an_access_record_row_and_no_secret() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-record").await;
    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let before = token_rows(&screen.store).len();

    let (_, _, shown) = send(
        &screen.app,
        create_form(&form_key(&page), LAPTOP, &screen.bob),
    )
    .await;
    let token = shown_token(&shown);
    let rows = token_rows(&screen.store);
    assert_eq!(rows.len(), before + 1);
    let created = rows.last().unwrap();
    assert_eq!(
        (
            created.event.as_str(),
            created.actor.as_str(),
            created.credential.as_str(),
            created.target.as_deref(),
            created.detail.as_deref()
        ),
        (
            "token-created",
            "bob",
            "member-session",
            Some("bob"),
            Some(LAPTOP)
        )
    );

    let id = screen
        .store
        .member_tokens("bob")
        .unwrap()
        .last()
        .unwrap()
        .id;
    send(
        &screen.app,
        token_post(&format!("/tokens/{id}/revoke"), &[], &screen.bob),
    )
    .await;
    let rows = token_rows(&screen.store);
    assert_eq!(rows.len(), before + 2);
    let revoked = rows.last().unwrap();
    assert_eq!(
        (
            revoked.event.as_str(),
            revoked.actor.as_str(),
            revoked.detail.as_deref()
        ),
        ("token-revoked", "bob", Some(LAPTOP))
    );
    for row in screen.store.access_record().unwrap() {
        let text = format!("{row:?}");
        assert!(!text.contains(&token), "{text}");
        assert!(
            !text.contains(&crate::store::credential_digest(&token)),
            "{text}"
        );
        assert!(!text.contains("tdm_"), "{text}");
    }
    // The database file holds the label and the digest, never the value.
    let bytes = std::fs::read(screen.store.home().directory().join("tandem.db")).unwrap();
    assert!(
        !bytes
            .windows(token.len())
            .any(|window| window == token.as_bytes()),
        "the value is nowhere in the store"
    );
    drop(screen.sandbox);
}

/// The byte offset of `needle` in `page`, or a failure that names it.
fn at(page: &str, needle: &str) -> usize {
    page.find(needle)
        .unwrap_or_else(|| panic!("{needle} is not in the page: {page}"))
}

#[tokio::test]
async fn the_shell_shows_personal_tokens_per_mode() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let link = "<a class=\"menu__option\" href=\"/tokens\">Personal tokens</a>";

    // Loopback with no members: no link, and the screen is not there.
    let solo = Sandbox::new("tokens-shell-loopback");
    let loopback = router(context(solo.store()));
    for uri in ["/board", "/notes"] {
        let (_, _, page) = get(&loopback, uri).await;
        assert!(
            !page.contains("href=\"/tokens\"") && !page.contains("Personal tokens"),
            "{uri}"
        );
    }
    assert_eq!(get(&loopback, "/tokens").await.0, StatusCode::NOT_FOUND);

    // A single token and no members: the same.
    let single = Sandbox::new("tokens-shell-single");
    let app = remote_app(single.store(), TOKEN);
    let cookie = sign_in(&app).await;
    for uri in ["/board", "/notes"] {
        let (_, _, page) = send(&app, remote_get(uri, Some(&cookie))).await;
        assert!(
            !page.contains("href=\"/tokens\"") && !page.contains("Personal tokens"),
            "{uri}"
        );
    }
    let (status, _, _) = send(&app, remote_get("/tokens", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send(
        &app,
        remote_form(
            "/tokens",
            "key=x&name=y",
            Some(REMOTE_ORIGIN),
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Team mode: every member and the owner session see it, in order.
    let screen = token_screen("tokens-shell-team").await;
    let alice = sign_in_with(&screen.app, &issue_key(&screen.store, "alice")).await;
    let owner = sign_in(&screen.app).await;
    for (who, cookie, exports) in [
        ("an admin", &alice, true),
        ("the owner", &owner, true),
        ("a member", &screen.bob, false),
    ] {
        for uri in ["/board", "/notes", "/tokens"] {
            let (_, _, page) = send(&screen.app, remote_get(uri, Some(cookie))).await;
            assert!(page.contains(link), "{who} {uri}");
            let appearance = at(&page, "data-theme-appearance=\"dark\"");
            let tokens = at(&page, link);
            let signout = at(&page, "action=\"/logout\"");
            assert!(
                appearance < tokens && tokens < signout,
                "{who} {uri}: the order"
            );
            assert_eq!(page.contains("Export workspace"), exports, "{who} {uri}");
            if exports {
                assert!(tokens < at(&page, "Export workspace"), "{who} {uri}");
            }
        }
    }
    // The owner session has no tokens of its own, and says so.
    let (status, _, page) = send(&screen.app, remote_get("/tokens", Some(&owner))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("you have no personal tokens"), "{page}");
    assert!(!page.contains("action=\"/tokens\""));
    let (status, _, _) = send(
        &screen.app,
        remote_form("/tokens", "key=x&name=y", Some(REMOTE_ORIGIN), Some(&owner)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Loopback on a home that holds members: the owner sees the link.
    let members = Sandbox::new("tokens-shell-loopback-team");
    team(&members.store());
    let app = router(context(members.store()));
    let (_, _, page) = get(&app, "/board").await;
    assert!(page.contains(link));
}

#[tokio::test]
async fn the_tokens_screen_logs_no_token_and_no_cookie() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let screen = token_screen("tokens-logs").await;
    let bob_token = issued(&screen.store, "bob", "bob's tool");
    let captured = Captured::default();
    let _guard = captured.install();

    let (_, _, page) = send(&screen.app, cookie_get("/tokens", &screen.bob)).await;
    let key = form_key(&page);
    let (_, _, shown) = send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    let token = shown_token(&shown);
    send(&screen.app, create_form(&key, LAPTOP, &screen.bob)).await;
    send(&screen.app, create_form(&"d".repeat(32), "", &screen.bob)).await;
    let (status, _, _) = send(&screen.app, bearer_get("/tokens", &bob_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let id = screen
        .store
        .member_tokens("bob")
        .unwrap()
        .last()
        .unwrap()
        .id;
    send(
        &screen.app,
        cookie_get(&format!("/tokens/{id}/revoke"), &screen.bob),
    )
    .await;
    send(
        &screen.app,
        token_post(&format!("/tokens/{id}/revoke"), &[], &screen.bob),
    )
    .await;
    send(&screen.app, bearer_mcp(&token, None, MCP_BODY)).await;

    let text = captured.text();
    let cookie_value = screen
        .bob
        .split_once('=')
        .map(|(_, value)| value)
        .unwrap_or("");
    assert!(!text.is_empty());
    for secret in [
        token.as_str(),
        token.trim_start_matches("tdm_"),
        bob_token.as_str(),
        screen.bob_key.as_str(),
        cookie_value,
        TOKEN,
        "tandem_session",
    ] {
        assert!(
            !secret.is_empty() && !text.contains(secret),
            "{secret}: {text}"
        );
    }
    assert!(text.contains("/tokens"), "the route is logged: {text}");
}

// MARK: - The People screen (CUT-42, ADR 0052)

const LAST_ADMIN: &str = "This is the only admin. Make someone else an admin first.";

/// A form key of 32 lowercase hex characters, the way the page draws one.
fn key_for(n: u8) -> String {
    format!("{n:032x}")
}

/// The sign-in key a page shows: `tdk_` and its 43 characters.
fn shown_key(page: &str) -> String {
    let marker = "value=\"tdk_";
    let at = page.find(marker).expect("the page shows a sign-in key") + "value=\"".len();
    page[at..at + 47].to_string()
}

/// The `Location` a response names, or the empty string.
fn location(headers: &HeaderMap) -> String {
    headers
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// The `before` id a page's older-entries link names, when there is one.
fn next_before(page: &str) -> Option<String> {
    let marker = "/people?before=";
    let at = page.find(marker)? + marker.len();
    Some(
        page[at..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect(),
    )
}

/// A team behind the owner token: `alice` (an admin), signed in, and `bob` (a
/// member) with no session yet.
struct People {
    _sandbox: Sandbox,
    store: Store,
    app: Router,
    alice: String,
}

async fn people_screen(name: &str) -> People {
    let sandbox = Sandbox::new(name);
    let store = sandbox.store();
    store
        .member_add("alice", crate::store::Role::Admin, NOW)
        .unwrap();
    store
        .member_add("bob", crate::store::Role::Member, NOW)
        .unwrap();
    let app = remote_app(store.clone(), TOKEN);
    let alice_key = issue_key(&store, "alice");
    let alice = sign_in_with(&app, &alice_key).await;
    People {
        _sandbox: sandbox,
        store,
        app,
        alice,
    }
}

/// The add form, as the page sends it.
fn add_person(name: &str, role: &str, n: u8, cookie: &str) -> Request<Body> {
    let key = key_for(n);
    token_post(
        "/people",
        &[("name", name), ("role", role), ("key", key.as_str())],
        cookie,
    )
}

/// A role change, as a row's form sends it.
fn role_post(name: &str, role: &str, cookie: &str) -> Request<Body> {
    token_post(&format!("/people/{name}/role"), &[("role", role)], cookie)
}

#[tokio::test]
async fn an_admin_adds_a_person_and_the_response_shows_a_sign_in_key_once() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-add").await;
    let (status, headers, body) = send(&s.app, add_person("cy", "member", 1, &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Copy it now. You will not see it again."),
        "{body}"
    );
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let key = shown_key(&body);
    assert!(s.store.member_find("cy").unwrap().is_some());
    assert!(s.store.member_by_signin_key(&key).unwrap().is_some());
    let (_, _, list) = send(&s.app, cookie_get("/people", &s.alice)).await;
    assert!(
        !list.contains("project-switch"),
        "the people screen draws no project switcher"
    );
    assert!(!list.contains("tdk_"), "the list never shows a key");
}

#[tokio::test]
async fn the_sign_in_key_signs_the_person_in_and_the_session_belongs_to_them() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-key-login").await;
    let (_, _, body) = send(&s.app, add_person("cy", "member", 2, &s.alice)).await;
    let cy = sign_in_with(&s.app, &shown_key(&body)).await;
    let (status, _, _) = send(&s.app, cookie_get("/board", &cy)).await;
    assert_eq!(status, StatusCode::OK);
    // The session is cy's, and cy is a member: the people screen refuses her.
    let (status, _, page) = send(&s.app, cookie_get("/people", &cy)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(page.contains(NO_ACCESS), "{page}");
}

#[tokio::test]
async fn reissuing_a_key_stops_the_old_one_at_once() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-reissue").await;
    let old = issue_key(&s.store, "bob");
    let (status, _, _) = send(&s.app, cookie_get("/people/bob/key", &s.alice)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, body) = send(&s.app, token_post("/people/bob/key", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let new = shown_key(&body);
    assert_ne!(new, old);
    let (status, _, _) = send(
        &s.app,
        remote_form(
            "/login",
            &login_body(&old, "/board"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the old key is stopped");
    let _ = sign_in_with(&s.app, &new).await;
}

#[tokio::test]
async fn a_reserved_duplicate_or_ever_used_name_is_refused_and_keeps_the_input() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-names").await;
    for (n, name) in ["owner", "human", "operator", "count"].iter().enumerate() {
        let (status, _, body) =
            send(&s.app, add_person(name, "member", 10 + n as u8, &s.alice)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
        assert!(body.contains("is reserved"), "{name}: {body}");
        assert!(
            body.contains(&format!("value=\"{name}\"")),
            "{name}: {body}"
        );
    }
    let (status, _, body) = send(&s.app, add_person("Ana", "member", 20, &s.alice)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("value=\"Ana\""), "{body}");
    // A live name is taken.
    let (status, _, body) = send(&s.app, add_person("bob", "member", 21, &s.alice)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("That name is already taken."), "{body}");
    assert!(body.contains("value=\"bob\""), "{body}");
    // So is a removed one: a name is never reused.
    s.store
        .member_add("dana", crate::store::Role::Member, NOW)
        .unwrap();
    s.store.member_remove("dana", NOW).unwrap();
    let (status, _, body) = send(&s.app, add_person("dana", "member", 22, &s.alice)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("That name is already taken."), "{body}");
    assert!(s.store.member_find("dana").unwrap().is_none());
}

#[tokio::test]
async fn changing_a_role_takes_effect_on_the_next_request() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-role").await;
    let bob = sign_in_with(&s.app, &issue_key(&s.store, "bob")).await;
    let (status, _, _) = send(&s.app, cookie_get("/people", &bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a member is refused");
    let (status, _, body) = send(&s.app, role_post("bob", "admin", &s.alice)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let (status, _, _) = send(&s.app, cookie_get("/people", &bob)).await;
    assert_eq!(status, StatusCode::OK, "the next request is an admin's");
    let (status, _, body) = send(&s.app, role_post("bob", "member", &s.alice)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let (status, _, _) = send(&s.app, cookie_get("/people", &bob)).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the next request is a member's"
    );
}

#[tokio::test]
async fn the_last_admins_controls_are_disabled_with_a_reason_and_the_server_refuses_the_same_act() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-last-admin").await;
    let (status, _, page) = send(&s.app, cookie_get("/people", &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains(LAST_ADMIN), "{page}");
    assert!(page.contains("disabled"), "{page}");
    let (status, _, body) = send(&s.app, role_post("alice", "member", &s.alice)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains(LAST_ADMIN), "{body}");
    let (status, _, body) = send(&s.app, cookie_get("/people/alice/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains(LAST_ADMIN), "{body}");
    let (status, _, body) = send(&s.app, token_post("/people/alice/remove", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        s.store.member_find("alice").unwrap().unwrap().role,
        crate::store::Role::Admin
    );
}

#[tokio::test]
async fn an_admin_removing_themselves_lands_on_login_with_the_removed_notice() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-self-remove").await;
    s.store
        .member_set_role("bob", crate::store::Role::Admin, NOW)
        .unwrap();
    let (status, _, page) = send(&s.app, cookie_get("/people/alice/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Remove yourself?"), "{page}");
    let (status, headers, body) =
        send(&s.app, token_post("/people/alice/remove", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(location(&headers), "/login?notice=removed");
    let (_, _, login) = send(&s.app, remote_get("/login?notice=removed", None)).await;
    assert!(
        login.contains("You were removed from this workspace."),
        "{login}"
    );
    let (status, headers, _) = send(&s.app, cookie_get("/board", &s.alice)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).starts_with("/login"));
    assert!(s.store.member_find("alice").unwrap().is_none());
}

#[tokio::test]
async fn removing_a_person_goes_through_a_confirm_page_and_ends_their_sessions_tokens_and_stream() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-remove").await;
    let bob_key = issue_key(&s.store, "bob");
    let bob = sign_in_with(&s.app, &bob_key).await;
    let bob_token = issued(&s.store, "bob", LAPTOP);
    let mut signal = crate::store::credentials_changed();
    let (status, _, page) = send(&s.app, cookie_get("/people/bob/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("This cannot be undone."), "{page}");
    assert!(page.contains("autofocus"), "Cancel is the focused control");
    assert!(
        s.store.member_find("bob").unwrap().is_some(),
        "the confirm page changes nothing"
    );
    let (status, _, body) = send(&s.app, token_post("/people/bob/remove", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("bob was removed."), "{body}");
    assert!(s.store.member_find("bob").unwrap().is_none());
    let (status, headers, _) = send(&s.app, cookie_get("/board", &bob)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).starts_with("/login"));
    let (status, _, _) = send(&s.app, bearer_mcp(&bob_token, None, &project_call("after"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the token stops at once");
    let (status, _, _) = send(
        &s.app,
        remote_form(
            "/login",
            &login_body(&bob_key, "/board"),
            Some(REMOTE_ORIGIN),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "the key stops at once");
    assert!(signal.try_recv().is_ok(), "open streams are told at once");
}

#[tokio::test]
async fn a_removed_name_stays_on_old_tickets_and_comments_unmarked_and_cannot_be_reused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-removed-name").await;
    let bob_token = issued(&s.store, "bob", LAPTOP);
    let (status, _, body) =
        send(&s.app, bearer_mcp(&bob_token, None, &project_call("alpha"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(author_of_project(&s.store, "alpha"), "bob");
    let (status, _, _) = send(&s.app, cookie_get("/people/bob/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, body) = send(&s.app, token_post("/people/bob/remove", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        author_of_project(&s.store, "alpha"),
        "bob",
        "the history keeps the name as written"
    );
    let (status, _, body) = send(&s.app, add_person("bob", "member", 30, &s.alice)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("That name is already taken."), "{body}");
    let (_, _, list) = send(&s.app, cookie_get("/people", &s.alice)).await;
    assert!(list.contains("alice removed bob"), "{list}");
    assert!(!list.contains("(removed)"), "{list}");
}

#[tokio::test]
async fn a_member_gets_the_403_page_on_every_people_route_including_a_post_and_a_token_gets_the_403()
 {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-member-403").await;
    let bob = sign_in_with(&s.app, &issue_key(&s.store, "bob")).await;
    let bob_token = issued(&s.store, "bob", LAPTOP);
    let routes = [
        ("GET", "/people"),
        ("POST", "/people"),
        ("GET", "/people/alice"),
        ("GET", "/people/alice/key"),
        ("POST", "/people/alice/key"),
        ("POST", "/people/alice/role"),
        ("GET", "/people/alice/remove"),
        ("POST", "/people/alice/remove"),
        ("GET", "/people/alice/tokens/1/revoke"),
        ("POST", "/people/alice/tokens/1/revoke"),
    ];
    for (method, uri) in routes {
        let request = if method == "GET" {
            cookie_get(uri, &bob)
        } else {
            let key = key_for(40);
            token_post(
                uri,
                &[("name", "cy"), ("role", "admin"), ("key", key.as_str())],
                &bob,
            )
        };
        let (status, _, body) = send(&s.app, request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
        assert!(body.contains(NO_ACCESS), "{method} {uri}: {body}");
    }
    for (method, uri) in routes {
        let (status, _, _) = send(&s.app, table_request(method, uri, &bob_token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri} for a token");
    }
}

#[tokio::test]
async fn the_access_record_pages_by_fifty_newest_first_and_shows_the_empty_sentence() {
    let _serial = ONE_AT_A_TIME.lock().await;
    // An empty record reads as a sentence, not a blank table.
    let empty = Sandbox::new("people-record-empty");
    let empty_app = remote_app(empty.store(), TOKEN);
    let owner = sign_in_with(&empty_app, TOKEN).await;
    let (status, _, page) = send(&empty_app, cookie_get("/people", &owner)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("Nothing recorded yet."), "{page}");

    let s = people_screen("people-record").await;
    for _ in 0..120 {
        issue_key(&s.store, "bob");
    }
    let total = s.store.access_record().unwrap().len();
    let mut uri = "/people".to_string();
    let mut seen = 0;
    let mut pages = 0;
    loop {
        let (status, _, page) = send(&s.app, cookie_get(&uri, &s.alice)).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        let rows = page.matches("class=\"people__event\"").count();
        assert!(rows <= 50, "a page holds 50 rows at most, got {rows}");
        if pages == 0 {
            assert_eq!(rows, 50);
            let at = page.find("class=\"people__event\"").unwrap();
            assert!(page[at..at + 200].contains("for bob"), "newest first");
        }
        if next_before(&page).is_none() {
            assert!(page.contains("added alice as admin"), "the oldest is last");
        }
        seen += rows;
        pages += 1;
        match next_before(&page) {
            Some(before) => uri = format!("/people?before={before}"),
            None => break,
        }
        assert!(pages < 10, "the paging must end");
    }
    assert_eq!(seen, total);
    assert_eq!(pages, total.div_ceil(50));
}

#[tokio::test]
async fn an_admin_revokes_another_members_token_and_it_is_recorded() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-token").await;
    let bob_token = issued(&s.store, "bob", LAPTOP);
    let (status, _, body) = send(&s.app, bearer_mcp(&bob_token, None, &project_call("beta"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = s.store.member_tokens("bob").unwrap()[0].id;
    let uri = format!("/people/bob/tokens/{id}/revoke");
    let (status, _, page) = send(&s.app, cookie_get(&uri, &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains(LAPTOP), "{page}");
    let (status, headers, body) = send(&s.app, token_post(&uri, &[], &s.alice)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(location(&headers), "/people/bob?notice=token-revoked");
    let (status, _, _) = send(&s.app, bearer_mcp(&bob_token, None, &project_call("gamma"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let rows: Vec<_> = s
        .store
        .access_record()
        .unwrap()
        .into_iter()
        .filter(|row| row.event == "token-revoked")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].actor, "alice");
    assert_eq!(rows[0].target.as_deref(), Some("bob"));
}

#[tokio::test]
async fn the_owner_session_adds_the_first_admin_and_no_unauthenticated_path_does() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("people-bootstrap");
    let store = sandbox.store();
    let app = remote_app(store.clone(), TOKEN);
    // Nobody signed in: the page sends them to sign in, and the add makes nobody.
    let (status, headers, _) = send(&app, remote_get("/people", None)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).starts_with("/login"));
    let key = key_for(50);
    let body = format!("name=eve&role=admin&key={key}");
    let (status, _, _) = send(
        &app,
        remote_form("/people", &body, Some(REMOTE_ORIGIN), None),
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    assert!(store.member_find("eve").unwrap().is_none());
    // The TANDEM_TOKEN holder signs in and adds the first admin here.
    let owner = sign_in_with(&app, TOKEN).await;
    let (status, _, page) = send(&app, cookie_get("/people", &owner)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("workspace owner"), "{page}");
    let (status, _, body) = send(&app, add_person("fay", "member", 51, &owner)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body.contains("The first person must be an admin."),
        "{body}"
    );
    let (status, _, body) = send(&app, add_person("ana", "admin", 52, &owner)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(shown_key(&body).starts_with("tdk_"));
    assert_eq!(
        store.member_find("ana").unwrap().unwrap().role,
        crate::store::Role::Admin
    );
    // The owner's act is recorded as the owner, not as a member invented for it.
    let added = store
        .access_record()
        .unwrap()
        .into_iter()
        .find(|row| row.target.as_deref() == Some("ana"))
        .expect("the first admin's add is recorded");
    assert_eq!(added.actor, "owner");
    assert_eq!(added.credential, "owner");
}

#[tokio::test]
async fn people_answers_404_outside_team_mode_and_the_shell_shows_no_people_link_in_solo_modes() {
    let _serial = ONE_AT_A_TIME.lock().await;
    // Loopback with no token and no members: nothing to manage.
    let sandbox = Sandbox::new("people-404");
    let store = sandbox.store();
    let loopback = router(context(store.clone()));
    let (status, _, _) = get(&loopback, "/people").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The owner token with no members: no People link in the menu.
    let solo = remote_app(store.clone(), TOKEN);
    let owner = sign_in_with(&solo, TOKEN).await;
    let (_, _, board) = send(&solo, cookie_get("/board", &owner)).await;
    assert!(!board.contains("href=\"/people\""), "{board}");
    // Team mode: an admin and the owner see the link, a member does not.
    store
        .member_add("alice", crate::store::Role::Admin, NOW)
        .unwrap();
    store
        .member_add("bob", crate::store::Role::Member, NOW)
        .unwrap();
    let team = remote_app(store.clone(), TOKEN);
    let alice = sign_in_with(&team, &issue_key(&store, "alice")).await;
    let bob = sign_in_with(&team, &issue_key(&store, "bob")).await;
    let team_owner = sign_in_with(&team, TOKEN).await;
    for cookie in [&alice, &team_owner] {
        let (_, _, board) = send(&team, cookie_get("/board", cookie)).await;
        assert!(board.contains("href=\"/people\""), "{board}");
    }
    let (_, _, board) = send(&team, cookie_get("/board", &bob)).await;
    assert!(!board.contains("href=\"/people\""), "{board}");
}

#[tokio::test]
async fn the_owner_keeps_the_people_screen_in_team_mode_and_the_operator_key_never_reaches_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("people-owner-team");
    let store = sandbox.store();
    store
        .member_add("alice", crate::store::Role::Admin, NOW)
        .unwrap();
    let app = remote_app(store.clone(), TOKEN);
    let owner = sign_in_with(&app, TOKEN).await;
    let (status, _, page) = send(&app, cookie_get("/people", &owner)).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    // The operator key is a hosting door, not a browser credential: it gets no People page.
    let operator = operator_app(store.clone(), OPERATOR_KEY, None);
    let (status, _, _) = send(
        &operator,
        operator_request("GET", "/people", Some(OPERATOR_KEY), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_stale_people_confirm_answers_the_end_state() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let s = people_screen("people-stale").await;
    let (status, _, _) = send(&s.app, cookie_get("/people/bob/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::OK);
    // Removed through another door while the confirm page was open.
    s.store.member_remove("bob", NOW).unwrap();
    let (status, _, body) = send(&s.app, token_post("/people/bob/remove", &[], &s.alice)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("bob was already removed."), "{body}");
    let (_, _, body) = send(&s.app, cookie_get("/people/bob/key", &s.alice)).await;
    assert!(body.contains("bob was already removed."), "{body}");
    let (status, _, _) = send(&s.app, cookie_get("/people/nobody/remove", &s.alice)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_people_screens_log_no_key_token_or_cookie() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let captured = Captured::default();
    let _guard = captured.install();
    let s = people_screen("people-logs").await;
    let (_, _, body) = send(&s.app, add_person("cy", "member", 60, &s.alice)).await;
    let added = shown_key(&body);
    let (_, _, body) = send(&s.app, token_post("/people/cy/key", &[], &s.alice)).await;
    let reissued = shown_key(&body);
    let token = issued(&s.store, "bob", LAPTOP);
    let _ = send(&s.app, role_post("bob", "admin", &s.alice)).await;
    let _ = send(&s.app, role_post("bob", "member", &s.alice)).await;
    let id = s.store.member_tokens("bob").unwrap()[0].id;
    let _ = send(
        &s.app,
        token_post(&format!("/people/bob/tokens/{id}/revoke"), &[], &s.alice),
    )
    .await;
    let cookie_value = s
        .alice
        .split_once('=')
        .map(|(_, value)| value.to_string())
        .unwrap();
    let text = captured.text();
    for needle in [
        "tdk_",
        "tdm_",
        "Bearer",
        "Set-Cookie",
        added.as_str(),
        reissued.as_str(),
        token.as_str(),
        cookie_value.as_str(),
    ] {
        assert!(!text.contains(needle), "{needle} reached the log: {text}");
    }
}

fn wikilink_vault(notes: &crate::notes::Notes, text: &str) {
    for path in ["A.md", "B.md", "x/Dup.md", "y/Dup.md"] {
        notes.create(path).expect("the note is created");
    }
    notes
        .write("A.md", text, None)
        .expect("the note is written");
}

#[tokio::test]
async fn the_read_view_draws_plain_wikilinks_as_links_and_broken_ones_as_spans() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-wikilinks");
    let store = sandbox.store();
    let notes = vault(&store);
    wikilink_vault(
        &notes,
        "[[B]] [[b|Bee]] [[Missing]] [[Dup]] [[project:Tandem]] `[[B]]`",
    );
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes?path=A.md&mode=read")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("<a class=\"wikilink\" href=\"/notes?project=&amp;path=B.md\""),
        "{body}"
    );
    assert!(body.contains("title=\"B.md\">B</a>"), "{body}");
    assert!(body.contains("title=\"B.md\">Bee</a>"), "{body}");
    assert!(
        body.contains(
            "wikilink wikilink--broken\" title=\"No single note named Missing\">Missing</span>"
        ),
        "{body}"
    );
    assert!(body.contains("note named Dup\">Dup</span>"), "{body}");
    assert!(
        body.contains("wikilink wikilink--typed wikilink--broken\" title=\"No project named Tandem on the board\""),
        "{body}"
    );
    assert!(
        body.contains("<code class=\"markdown__inline-code\">[[B]]</code>"),
        "{body}"
    );
    assert_eq!(body.matches("class=\"wikilink\"").count(), 2, "{body}");
}

#[tokio::test]
async fn a_hostile_wikilink_is_escaped_in_the_read_view() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-wikilinks-hostile");
    let store = sandbox.store();
    let notes = vault(&store);
    wikilink_vault(&notes, "[[<script>x|\"><img src=x>]]");
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes?path=A.md&mode=read")).await;
    assert!(!body.contains("<script>x"), "{body}");
    assert!(!body.contains("<img src=x>"), "{body}");
    assert!(body.contains("&lt;script&gt;x"), "{body}");
}

#[tokio::test]
async fn notes_open_follows_a_resolved_target_and_banners_a_broken_one() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-open");
    let store = sandbox.store();
    let notes = vault(&store);
    wikilink_vault(&notes, "[[B]]");
    notes.write("B.md", "bee body", None).expect("written");
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes/open?target=b&from=A.md")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("bee body"), "{body}");
    assert!(!body.contains("No single note named"), "{body}");

    let (_, _, body) = send(&app, page_get("/notes/open?target=Missing&from=A.md")).await;
    assert!(
        body.contains("No single note named Missing: none exists, or two notes share the name."),
        "{body}"
    );
    assert!(body.contains("\"/notes?project=&#38;path=A.md\""), "{body}");

    let (status, _, body) = send(
        &app,
        page_get("/notes/open?target=..%2F..%2Fetc%2Fpasswd&from=A.md"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("No single note named ../../etc/passwd"),
        "{body}"
    );
    assert!(!body.contains("root:"), "{body}");
}

#[tokio::test]
async fn the_edit_view_lists_the_targets_that_resolve_broken() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-broken-links");
    let store = sandbox.store();
    let notes = vault(&store);
    wikilink_vault(&notes, "[[B]] [[Missing]] [[Dup]] [[project:Tandem]]");
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes/edit?path=A.md")).await;
    assert!(
        body.contains(
            "data-broken-links=\"[&#34;dup&#34;,&#34;missing&#34;,&#34;project:tandem&#34;]\""
        ),
        "{body}"
    );
}

#[tokio::test]
async fn serve_start_creates_the_missing_folders_and_a_label_add_creates_its_own() {
    let _one = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("serve-folders");
    let store = sandbox.store();
    setup_board(&store);
    let vault = sandbox.root.join("home").join("notes");
    assert!(
        !vault.join("projects").exists(),
        "nothing exists before serve starts"
    );
    std::fs::create_dir_all(vault.join("projects/alpha")).expect("a pre-existing folder");

    let app = router(context(store));
    let mut spelled: Vec<String> = std::fs::read_dir(vault.join("projects"))
        .expect("projects reads")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    spelled.sort();
    assert_eq!(
        spelled,
        vec!["Beta", "Elsewhere", "alpha"],
        "an existing folder keeps its spelling and is reused"
    );
    assert!(vault.join("projects/alpha/shared").is_dir());

    let (status, _, _) = send(&app, form_request("/labels", "name=gamma&project=Alpha")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(vault.join("projects/alpha/gamma").is_dir());
}

#[tokio::test]
async fn get_notes_graph_json_answers_the_typed_graph_and_narrows_by_project() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-graph");
    let store = sandbox.store();
    setup_board(&store);
    let notes = vault(&store);
    notes
        .create("projects/alpha/Inside.md")
        .expect("the note is created");
    notes
        .create("projects/beta/Outside.md")
        .expect("the note is created");
    notes.create("Other.md").expect("the note is created");
    notes
        .write("Other.md", "See [[project:Alpha]]", None)
        .expect("the note is written");
    let app = router(context(store));

    let (status, headers, body) = get(&app, "/notes/graph.json").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let whole: serde_json::Value = serde_json::from_str(&body).expect("the answer is JSON");
    for key in ["nodes", "edges", "total", "shown", "cap"] {
        assert!(whole.get(key).is_some(), "the answer carries {key}: {body}");
    }
    assert_eq!(whole["cap"], 1000);
    assert_eq!(whole["total"], whole["shown"]);
    let ids = |value: &serde_json::Value| -> Vec<String> {
        value["nodes"]
            .as_array()
            .expect("nodes is a list")
            .iter()
            .map(|node| node["id"].as_str().expect("an id").to_string())
            .collect()
    };
    let whole_ids = ids(&whole);
    assert!(whole_ids.contains(&"projects/beta/Outside.md".to_string()));
    let alpha = whole["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["id"] == "projects/alpha")
        .expect("the project node");
    assert_eq!(alpha["kind"], "project");
    assert_eq!(alpha["name"], "Alpha");

    let (status, _, body) = get(&app, "/notes/graph.json?project=Alpha").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let narrowed: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    let narrowed_ids = ids(&narrowed);
    assert!(narrowed_ids.contains(&"projects/alpha/Inside.md".to_string()));
    assert!(
        narrowed_ids.contains(&"Other.md".to_string()),
        "it links in"
    );
    assert!(!narrowed_ids.contains(&"projects/beta/Outside.md".to_string()));
}

#[tokio::test]
async fn get_notes_targets_answers_projects_labels_and_each_notes_shortest_target() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-targets");
    let store = sandbox.store();
    setup_board(&store);
    let notes = vault(&store);
    for path in [
        "Plain.md",
        "a/Port.md",
        "b/Port.md",
        "projects/alpha/Inside.md",
    ] {
        notes.create(path).expect("the note is created");
    }
    let app = router(context(store));

    let (status, headers, body) = get(&app, "/notes/targets").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let answer: serde_json::Value = serde_json::from_str(&body).expect("the answer is JSON");
    let projects: Vec<&str> = answer["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|p| p["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(projects, ["Alpha", "Beta", "Elsewhere"]);
    let labels = answer["labels"].as_array().expect("labels");
    let alpha = labels
        .iter()
        .find(|l| l["name"] == "alpha")
        .expect("the label");
    assert_eq!(alpha["project"], "Alpha");
    let target = |path: &str| {
        answer["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .find(|n| n["path"] == path)
            .unwrap_or_else(|| panic!("{path} is listed: {body}"))["target"]
            .as_str()
            .expect("a target")
            .to_string()
    };
    assert_eq!(target("Plain.md"), "Plain");
    assert_eq!(target("projects/alpha/Inside.md"), "Inside");
    assert_eq!(target("a/Port.md"), "a/Port");
    assert_eq!(target("b/Port.md"), "b/Port");
}

#[tokio::test]
async fn get_notes_graph_json_with_an_unknown_project_answers_the_error() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-graph-unknown");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (status, headers, body) = get(&app, "/notes/graph.json?project=Nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let answer: serde_json::Value = serde_json::from_str(&body).expect("the error is JSON");
    let message = answer["error"].as_str().expect("an error message");
    assert!(
        message.contains("Nope") && message.contains("Alpha"),
        "{message}"
    );
    assert!(answer.get("nodes").is_none(), "never an empty graph");
}

#[test]
fn the_notes_graph_route_needs_what_the_notes_face_needs() {
    assert_eq!(
        guard::route_need("/notes/graph.json"),
        Some(guard::Need::Permit(crate::permission::Permission::Board))
    );
    assert_eq!(
        guard::route_need("/notes/graph.json"),
        guard::route_need("/notes")
    );
}

#[tokio::test]
async fn get_notes_graph_renders_the_container_the_switch_and_keeps_the_focus() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-graph-page");
    let store = sandbox.store();
    setup_board(&store);
    let notes = vault(&store);
    notes
        .create("projects/alpha/Inside.md")
        .expect("the note is created");
    let app = router(context(store));

    let (status, _, body) = get(&app, "/notes/graph?project=Alpha").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("id=\"notes-graph\""), "the container: {body}");
    assert!(
        body.contains("data-src=\"/notes/graph.json?project=Alpha\""),
        "the JSON source carries the focus: {body}"
    );
    assert!(
        body.contains("aria-label=\"Notes view\""),
        "the Note | Graph switch: {body}"
    );
    let graph_side = body
        .split("<a")
        .find(|piece| piece.contains("data-view=\"graph\""))
        .expect("the Graph side");
    assert!(
        graph_side.contains("aria-current=\"page\""),
        "Graph is the current side: {graph_side}"
    );
    let note_side = body
        .split("<a")
        .find(|piece| piece.contains("data-view=\"note\""))
        .expect("the Note side");
    assert!(!note_side.contains("aria-current"), "{note_side}");
    assert!(
        note_side.contains("hx-push-url=\"true\"") && graph_side.contains("hx-push-url=\"true\""),
        "both switch links push the URL: {body}"
    );
    assert!(
        note_side.contains("project=Alpha"),
        "the Note side keeps the focus: {note_side}"
    );
    assert!(
        !body.contains("data-relations-toggle"),
        "no Relations toggle in the graph view: {body}"
    );
    assert!(
        !body.contains("id=\"relations\""),
        "no Relations pane in the graph view"
    );
    // The live-simulation card: Feel with Springy pressed, Unpin, Grouped
    // pressed by default, and the legend's new entries.
    assert!(
        body.contains("data-graph-feel=\"springy\" aria-pressed=\"true\""),
        "Springy is the default feel: {body}"
    );
    for feel in ["calm", "bouncy"] {
        assert!(
            body.contains(&format!(
                "data-graph-feel=\"{feel}\" aria-pressed=\"false\""
            )),
            "{feel} is offered, not pressed: {body}"
        );
    }
    assert!(
        body.contains("data-graph-unpin"),
        "the Unpin button: {body}"
    );
    assert!(
        body.contains("data-graph-daily=\"grouped\" aria-pressed=\"true\""),
        "Grouped is the default daily mode: {body}"
    );
    assert!(
        body.contains("data-graph-daily=\"free\" aria-pressed=\"false\""),
        "Free is offered, not pressed: {body}"
    );
    let legend = body
        .split("aria-label=\"Key\"")
        .nth(1)
        .and_then(|rest| rest.split("</ul>").next())
        .expect("the legend");
    for entry in ["wikilink", "in folder", "orphan", "daily note", "pinned"] {
        assert!(legend.contains(entry), "the legend names {entry}: {legend}");
    }
    // Search count, the local-graph bar (hidden at load) and the key hint.
    assert!(
        body.contains("data-graph-count"),
        "the search count element: {body}"
    );
    let bar = body
        .split("data-graph-local ")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .expect("the local-graph bar");
    assert!(bar.contains("hidden"), "the bar is hidden at load: {bar}");
    assert!(
        body.contains("aria-label=\"Back to the whole graph\""),
        "the close button's accessible name: {body}"
    );
    for hops in ["data-graph-hops=\"1\"", "data-graph-hops=\"2\""] {
        assert!(body.contains(hops), "the hop switch has {hops}: {body}");
    }
    assert!(
        body.contains("for a local graph") && body.contains("to search"),
        "the key hint: {body}"
    );
    // Wide cap: unfocused source carries no project value to match.
    let (_, _, whole) = get(&app, "/notes/graph").await;
    assert!(
        whole.contains("data-src=\"/notes/graph.json\""),
        "no focus, no query: {whole}"
    );
}

#[tokio::test]
async fn get_notes_with_a_note_renders_the_switch_with_note_pressed_and_a_graph_link() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-graph-switch");
    let store = sandbox.store();
    setup_board(&store);
    let notes = vault(&store);
    notes
        .create("projects/alpha/Inside.md")
        .expect("the note is created");
    let app = router(context(store));

    let (status, _, body) = get(&app, "/notes?path=projects/alpha/Inside.md&project=Alpha").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let note_side = body
        .split("<a")
        .find(|piece| piece.contains("data-view=\"note\""))
        .expect("the Note side");
    assert!(note_side.contains("aria-current=\"page\""), "{note_side}");
    assert!(note_side.contains("hx-push-url=\"true\""), "{note_side}");
    let graph_side = body
        .split("<a")
        .find(|piece| piece.contains("data-view=\"graph\""))
        .expect("the Graph side");
    assert!(
        graph_side.contains("/notes/graph?project=Alpha"),
        "the Graph link keeps the focus: {graph_side}"
    );
    assert!(graph_side.contains("hx-push-url=\"true\""), "{graph_side}");
    assert!(!body.contains("id=\"notes-graph\""));
}

// MARK: - Typed links: chips, project and label pages, tree tags, Today's note

/// The vault the typed-link tests share: the Alpha project with its `alpha`
/// label folder holding two notes, a project subfolder no label names, a
/// loose note, and notes that link in.
fn typed_vault(notes: &crate::notes::Notes) {
    for path in [
        "projects/Alpha/shared/CLI.md",
        "projects/Alpha/shared/Dist.md",
        "projects/Alpha/App/Thing.md",
        "projects/Alpha/Decisions.md",
        "Index.md",
        "Daily.md",
    ] {
        notes.create(path).expect("the note is created");
    }
    notes
        .write(
            "Daily.md",
            "About: [[project:Alpha]] [[label:ALPHA|the lane]] [[label:nope]]",
            None,
        )
        .expect("the note is written");
    notes
        .write("Index.md", "the door: [[label:shared]]", None)
        .expect("the note is written");
}

#[tokio::test]
async fn the_read_view_draws_a_typed_link_as_a_chip_and_a_broken_one_as_broken() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-typed-chips");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    typed_vault(&notes);
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes?path=Daily.md&mode=read")).await;
    assert!(
        body.contains(
            "wikilink wikilink--typed\" href=\"/notes/concept?kind=project&amp;name=Alpha"
        ),
        "a project link is a chip to its page: {body}"
    );
    assert!(
        body.contains("<span class=\"wikilink__kind\">project</span><span class=\"wikilink__name\">Alpha</span>"),
        "the chip carries the kind word and the name: {body}"
    );
    assert!(
        body.contains("<span class=\"wikilink__name\">the lane</span>")
            && body.contains("name=alpha"),
        "an alias is shown and the board's spelling is linked: {body}"
    );
    assert!(
        body.contains("wikilink wikilink--typed wikilink--broken"),
        "a name the board lacks is the broken chip: {body}"
    );
    assert!(
        !body.contains("href=\"/notes/concept?kind=label&amp;name=nope"),
        "the broken chip links nowhere: {body}"
    );
}

#[tokio::test]
async fn the_label_page_draws_its_summary_and_the_three_sections() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-label-page");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    typed_vault(&notes);
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes/concept?kind=label&name=SHARED")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains(
            "Label in project Alpha. 2 notes filed here, 1 note links to it, 1 open ticket."
        ),
        "the summary line: {body}"
    );
    assert!(
        body.contains("<h1 class=\"page-title page-title--static\">shared</h1>"),
        "{body}"
    );
    let section = |kind: &str| {
        let at = body
            .find(&format!("data-section=\"{kind}\""))
            .unwrap_or_else(|| panic!("section {kind}: {body}"));
        let start = body[..at].rfind("<details").expect("a details");
        let end = at + body[at..].find("</details>").expect("a close");
        body[start..end].to_string()
    };
    let filed = section("filed");
    assert!(
        filed.contains(" open") && filed.contains("CLI") && filed.contains("Dist"),
        "{filed}"
    );
    let linked = section("linked");
    assert!(
        linked.contains(" open") && linked.contains("the door: [[label:shared]]"),
        "{linked}"
    );
    assert!(linked.contains("Index"), "{linked}");
    let tickets = section("tickets");
    assert!(
        !tickets.contains("open>") && tickets.contains("Shared alpha"),
        "{tickets}"
    );
    assert!(tickets.contains("href=\"/tickets/"), "{tickets}");
}

#[tokio::test]
async fn the_project_page_opens_its_entry_note_and_creates_it_when_missing() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-project-page");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    typed_vault(&notes);
    let app = router(context(store));

    assert!(
        notes.read("projects/Alpha/Alpha.md").is_err(),
        "no entry note yet"
    );
    let (status, _, body) = send(&app, page_get("/notes/concept?kind=project&name=alpha")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        notes.read("projects/Alpha/Alpha.md").is_ok(),
        "the first visit creates the entry note"
    );
    assert!(
        body.contains("data-live"),
        "the entry note is an editable note: {body}"
    );
    assert!(
        body.contains("name=\"concept\" value=\"project\""),
        "a save keeps the page: {body}"
    );
    for marker in [
        "data-section=\"filed\"",
        "data-section=\"linked\"",
        "data-section=\"tickets\"",
    ] {
        assert!(body.contains(marker), "{marker}: {body}");
    }
    assert!(!body.contains("data-section=\"backlinks\""), "{body}");
    assert!(
        body.contains("label, 2 notes") && body.contains("folder, no label, 1 note"),
        "the label folders then the rest: {body}"
    );
    assert!(body.contains("Decisions"), "the loose notes: {body}");
    assert!(
        body.contains("About: [[project:Alpha]]"),
        "typed links merge into Linked from: {body}"
    );

    let before = notes
        .read("projects/Alpha/Alpha.md")
        .expect("read")
        .value
        .text;
    let _ = send(&app, page_get("/notes/concept?kind=project&name=Alpha")).await;
    assert_eq!(
        notes
            .read("projects/Alpha/Alpha.md")
            .expect("read")
            .value
            .text,
        before,
        "a second visit changes nothing"
    );
}

#[tokio::test]
async fn an_unknown_concept_draws_the_error_banner_and_never_a_path() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-concept-unknown");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let app = router(context(store));

    for uri in [
        "/notes/concept?kind=label&name=nope",
        "/notes/concept?kind=project&name=..%2F..%2Fetc",
        "/notes/concept?kind=thing&name=alpha",
    ] {
        let (status, _, body) = send(&app, page_get(uri)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert!(
            body.contains("class=\"banner\" role=\"alert\""),
            "{uri}: {body}"
        );
        assert!(body.contains("on the board."), "{uri}: {body}");
        assert!(
            body.contains("href=\"/notes?project=\""),
            "a way back: {body}"
        );
    }
}

#[tokio::test]
async fn the_tree_tags_the_folders_the_board_names() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-tree-tags");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    typed_vault(&notes);
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes")).await;
    assert!(
        body.contains("<span class=\"tree__tag\">project</span>"),
        "{body}"
    );
    assert!(
        body.contains("<span class=\"tree__tag\">label</span>"),
        "{body}"
    );
    assert!(
        body.contains("<span class=\"tree__tag tree__tag--warn\">no label</span>"),
        "{body}"
    );
}

#[tokio::test]
async fn a_note_lists_the_projects_and_labels_it_links_to() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-links-to");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    typed_vault(&notes);
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes?path=Daily.md")).await;
    let at = body
        .find("data-section=\"links-to\"")
        .expect("the Links to section");
    let section = &body[body[..at].rfind("<details").expect("a details")
        ..at + body[at..].find("</details>").expect("a close")];
    assert!(section.contains(" open"), "Links to starts open: {section}");
    assert!(section.contains("label in Alpha"), "{section}");
    assert!(
        section.contains("/notes/concept?kind=project&#38;name=Alpha"),
        "{section}"
    );
    assert!(
        body.contains("data-broken-links=\"[&#34;label:nope&#34;]\""),
        "a broken typed link reaches the editor: {body}"
    );
}

#[tokio::test]
async fn todays_note_starts_with_the_about_line_only_when_a_project_is_focused() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-daily-about");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    let app = router(context(store));

    let _ = send(
        &app,
        form_request("/notes/daily", "date=2026-11-14&project=alpha"),
    )
    .await;
    assert_eq!(
        notes.read("daily/2026-11-14.md").expect("read").value.text,
        "About: [[project:Alpha]]\n",
        "the board's spelling of the focused project"
    );
    notes
        .write("daily/2026-11-14.md", "mine", None)
        .expect("written");
    let _ = send(
        &app,
        form_request("/notes/daily", "date=2026-11-14&project=Beta"),
    )
    .await;
    assert_eq!(
        notes.read("daily/2026-11-14.md").expect("read").value.text,
        "mine",
        "an existing page is untouched"
    );
    let _ = send(
        &app,
        form_request("/notes/daily", "date=2026-11-15&project="),
    )
    .await;
    assert_eq!(
        notes.read("daily/2026-11-15.md").expect("read").value.text,
        "",
        "no focus, an empty page"
    );
    let _ = send(
        &app,
        form_request("/notes/daily", "date=2026-11-16&project=Unknown"),
    )
    .await;
    assert_eq!(
        notes.read("daily/2026-11-16.md").expect("read").value.text,
        "",
        "an unknown project is no focus"
    );
}

/// A request for a notes page that carries `cookie` as its `Cookie` header.
fn notes_get_with_cookie(uri: &str, cookie: &str, htmx: bool) -> Request<Body> {
    let mut request = Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::COOKIE, cookie);
    if htmx {
        request = request.header("HX-Request", "true");
    }
    request.body(Body::empty()).expect("the request builds")
}

/// The tag of the `<li>` that holds the tree row for `path`, up to its row.
fn tree_item(body: &str, path: &str) -> String {
    let marker = format!("data-path=\"{path}\"");
    let at = body
        .find(&marker)
        .unwrap_or_else(|| panic!("row {path}: {body}"));
    let start = body[..at].rfind("<li").expect("an item");
    let end = body[at..].find("</div>").expect("a row end") + at;
    body[start..end].to_string()
}

fn tree_vault(store: &Store) {
    let notes = vault(store);
    for path in ["a/b/c.md", "a/x.md", "d/e.md", "top.md"] {
        notes.create(path).expect("the note is created");
    }
}

fn tree_cookie(json: &str) -> String {
    let encoded: String = json
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("tandem_7700_tree={encoded}")
}

#[tokio::test]
async fn the_shell_names_the_ui_cookie_prefix_on_the_html_element() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-prefix");
    let app = router(context(sandbox.store()));
    let (_, _, body) = send(&app, page_get("/notes")).await;
    assert!(
        body.contains("data-ui-cookie=\"tandem_7700_\""),
        "the shell carries the cookie prefix: {body}"
    );
}

#[tokio::test]
async fn the_relations_cookie_draws_the_pane_and_its_sections_on_the_first_paint() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-relations");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes.create("Other.md").expect("the note is created");
    notes
        .write("Other.md", "See [[Index]]", None)
        .expect("the note is written");
    let app = router(context(store));
    let uri = "/notes?path=Index.md";

    // No cookie: today's page, the pane open and Backlinks open (one links here).
    let (_, _, body) = send(&app, notes_get_with_cookie(uri, "", false)).await;
    assert!(!body.contains("relations--shut"), "no cookie, open pane");
    assert!(body.contains("data-section=\"backlinks\" open>"), "{body}");
    assert!(body.contains("aria-expanded=\"true\">Relations"), "{body}");

    // Stored shut: the wide pane is hidden by class and the toggle says so.
    let shut = "tandem_7700_relations=%7B%22open%22%3Afalse%7D";
    let (_, _, body) = send(&app, notes_get_with_cookie(uri, shut, false)).await;
    assert!(body.contains("relations--shut"), "{body}");
    assert!(body.contains("aria-expanded=\"false\">Relations"), "{body}");

    // A stored section shut: Backlinks loses `open`, the pane stays open.
    let section = "tandem_7700_relations=%7B%22sections%22%3A%7B%22backlinks%22%3Afalse%7D%7D";
    let (_, _, body) = send(&app, notes_get_with_cookie(uri, section, false)).await;
    assert!(!body.contains("relations--shut"));
    assert!(body.contains("data-section=\"backlinks\">"), "{body}");

    // A damaged cookie reads as absent: the first-visit page.
    let damaged = "tandem_7700_relations=%7Bnot-json";
    let (_, _, body) = send(&app, notes_get_with_cookie(uri, damaged, false)).await;
    assert!(!body.contains("relations--shut"), "damaged reads as absent");
    assert!(body.contains("data-section=\"backlinks\" open>"), "{body}");

    // Another port's cookie is not this serve's.
    let other_port = "tandem_9999_relations=%7B%22open%22%3Afalse%7D";
    let (_, _, body) = send(&app, notes_get_with_cookie(uri, other_port, false)).await;
    assert!(
        !body.contains("relations--shut"),
        "another port reads as absent"
    );
}

#[tokio::test]
async fn the_tree_cookie_draws_the_folds_on_the_first_paint() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-tree");
    let store = sandbox.store();
    let _board = setup_board(&store);
    tree_vault(&store);
    let app = router(context(store));
    let cookie = tree_cookie(r#"{"default":"shut","except":["a"]}"#);

    let (status, _, body) = send(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
    assert_eq!(status, StatusCode::OK);
    let a = tree_item(&body, "a");
    assert!(
        a.contains("aria-expanded=\"true\"") && a.contains("tree__twisty"),
        "{a}"
    );
    assert!(!a.contains("<li role=\"none\" hidden"), "{a}");
    assert!(!tree_item(&body, "a/x.md").starts_with("<li role=\"none\" hidden"));
    let ab = tree_item(&body, "a/b");
    assert!(
        ab.contains("aria-expanded=\"false\"") && ab.contains("tree__twisty"),
        "{ab}"
    );
    assert!(
        tree_item(&body, "a/b/c.md").starts_with("<li role=\"none\" hidden"),
        "a note under a shut folder is hidden"
    );
    let d = tree_item(&body, "d");
    assert!(d.contains("aria-expanded=\"false\""), "{d}");
    assert!(tree_item(&body, "d/e.md").starts_with("<li role=\"none\" hidden"));
    assert!(!tree_item(&body, "top.md").starts_with("<li role=\"none\" hidden"));
}

#[tokio::test]
async fn a_folder_with_children_draws_an_svg_chevron_and_an_empty_one_a_spacer() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-tree-chevron");
    let store = sandbox.store();
    let _board = setup_board(&store);
    tree_vault(&store);
    vault(&store)
        .create_folder("empty")
        .expect("the folder is created");
    let app = router(context(store));
    let (_, _, body) = send(&app, page_get("/notes")).await;
    let tree = &body[body.find("<ul class=\"tree\"").expect("the tree")..];
    assert!(
        !tree.contains('\u{25b8}') && !tree.contains('\u{25be}'),
        "no glyph"
    );
    let full = tree_item(&body, "a");
    assert!(
        full.contains("<svg class=\"tree__twisty\"") && full.contains("aria-expanded"),
        "{full}"
    );
    let empty = tree_item(&body, "empty");
    assert!(
        empty.contains("<span class=\"tree__twisty\" aria-hidden=\"true\"></span>")
            && !empty.contains("<svg class=\"tree__twisty\"")
            && !empty.contains("aria-expanded"),
        "{empty}"
    );
    let (_, _, css) = get(&app, "/static/app.css").await;
    assert_eq!(css.matches("\n.tree__twisty {").count(), 1);
    assert!(css.contains(".tree__row[aria-expanded=\"true\"] .tree__twisty"));
    let (_, _, script) = get(&app, "/static/tandem.js").await;
    assert!(!script.contains("u25b8") && !script.contains("u25be"));
}

#[tokio::test]
async fn without_a_tree_cookie_every_folder_is_shut_but_the_open_notes_ancestors() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-tree-first");
    let store = sandbox.store();
    let _board = setup_board(&store);
    tree_vault(&store);
    let app = router(context(store));

    let (_, _, body) = send(&app, page_get("/notes?path=a/b/c.md")).await;
    for open in ["a", "a/b"] {
        assert!(
            tree_item(&body, open).contains("aria-expanded=\"true\""),
            "{open}"
        );
    }
    assert!(tree_item(&body, "d").contains("aria-expanded=\"false\""));
    assert!(tree_item(&body, "d/e.md").starts_with("<li role=\"none\" hidden"));
    assert!(!tree_item(&body, "a/b/c.md").starts_with("<li role=\"none\" hidden"));
}

#[tokio::test]
async fn a_foreign_damaged_or_oversized_tree_cookie_reads_as_absent() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-tree-bad");
    let store = sandbox.store();
    let _board = setup_board(&store);
    tree_vault(&store);
    let app = router(context(store));
    let good = tree_cookie(r#"{"default":"open","except":[]}"#);
    let oversized = format!("tandem_7700_tree={}", "a".repeat(4000));
    for cookie in [
        good.replace("7700", "7701"),
        "tandem_7700_tree=%7Bnot-json".to_string(),
        oversized,
    ] {
        let (_, _, body) = send(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
        assert!(
            tree_item(&body, "a").contains("aria-expanded=\"false\""),
            "{cookie:.40} reads as the first-visit layout"
        );
    }
    let (_, _, body) = send(&app, notes_get_with_cookie("/notes", &good, false)).await;
    assert!(tree_item(&body, "a").contains("aria-expanded=\"true\""));
}

#[tokio::test]
async fn an_htmx_note_swap_draws_the_same_folds() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("ui-cookie-tree-swap");
    let store = sandbox.store();
    let _board = setup_board(&store);
    tree_vault(&store);
    let app = router(context(store));
    let cookie = tree_cookie(r#"{"default":"shut","except":["a"],"open":"top.md"}"#);

    let (_, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=top.md", &cookie, true),
    )
    .await;
    assert!(!body.contains("<!doctype html>"), "a partial has no shell");
    assert!(tree_item(&body, "a").contains("aria-expanded=\"true\""));
    assert!(tree_item(&body, "d").contains("aria-expanded=\"false\""));
    assert!(tree_item(&body, "d/e.md").starts_with("<li role=\"none\" hidden"));
}

fn tabs_cookie(open: &[&str], active: &str) -> String {
    let json = serde_json::json!({ "open": open, "active": active }).to_string();
    let encoded: String = json
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("tandem_7700_tabs={encoded}")
}

/// The tab paths the strip draws, in order.
fn strip_paths(body: &str) -> Vec<String> {
    body.split("data-tab-path=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// The `tabs` state the response stores, decoded.
fn stored_tabs(headers: &HeaderMap) -> Option<serde_json::Value> {
    let line = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|line| line.starts_with("tandem_7700_tabs="))?;
    let encoded = line.split(';').next()?.split_once('=')?.1;
    let mut bytes = Vec::new();
    let raw = encoded.as_bytes();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' {
            bytes.push(u8::from_str_radix(&encoded[at + 1..at + 3], 16).ok()?);
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    serde_json::from_slice(&bytes).ok()
}

fn tabs_vault(store: &Store) {
    let notes = vault(store);
    for path in ["a.md", "b.md", "c.md", "x/n.md", "y/n.md"] {
        notes.create(path).expect("the note is created");
    }
}

fn tabs_post(uri: &str, body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, cookie)
        .body(Body::from(body.to_string()))
        .expect("the request builds")
}

#[tokio::test]
async fn a_tabs_cookie_draws_the_strip_in_order_and_the_server_answers_the_list() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-draw");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["c.md", "a.md"], "a.md");

    let (status, headers, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=b.md", &cookie, false),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(strip_paths(&body), ["c.md", "a.md", "b.md"]);
    assert!(
        body.contains("role=\"tablist\"") && !body.contains("note-tabs\" data-note-tabs hidden")
    );
    let selected = body.split("aria-selected=\"true\"").count() - 1;
    assert!(body.contains("aria-selected=\"true\"") && selected >= 1);
    let tab_b = body.split("data-tab-path=\"b.md\"").nth(1).unwrap();
    assert!(
        tab_b.contains("aria-selected=\"true\""),
        "the open note's tab is selected: {tab_b}"
    );
    let stored = stored_tabs(&headers).expect("the list is answered");
    assert_eq!(stored["open"], serde_json::json!(["c.md", "a.md", "b.md"]));
    assert_eq!(stored["active"], "b.md");

    // The htmx swap draws the same strip.
    let (_, _, partial) = send(
        &app,
        notes_get_with_cookie("/notes?path=b.md", &cookie, true),
    )
    .await;
    assert_eq!(strip_paths(&partial), ["c.md", "a.md", "b.md"]);

    // An unchanged list answers no cookie.
    let again = tabs_cookie(&["c.md", "a.md", "b.md"], "b.md");
    let (_, headers, _) = send(
        &app,
        notes_get_with_cookie("/notes?path=b.md", &again, false),
    )
    .await;
    assert!(stored_tabs(&headers).is_none());
}

#[tokio::test]
async fn no_cookie_and_no_note_draws_no_strip() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-none");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let (_, headers, body) = send(&app, page_get("/notes")).await;
    assert!(!body.contains("data-note-tabs"), "{body}");
    assert!(stored_tabs(&headers).is_none());

    let cookie = tabs_cookie(&["a.md", "gone.md"], "gone.md");
    let (_, headers, body) = send(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
    assert_eq!(
        strip_paths(&body),
        ["a.md"],
        "the empty page keeps the strip"
    );
    assert_eq!(stored_tabs(&headers).unwrap()["active"], "");
}

#[tokio::test]
async fn a_path_the_vault_lacks_is_dropped_silently() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-lacks");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["gone.md", "../../etc/passwd", "a.md"], "gone.md");
    let (status, headers, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=b.md", &cookie, false),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(strip_paths(&body), ["a.md", "b.md"]);
    assert!(!body.contains("passwd"));
    assert_eq!(
        stored_tabs(&headers).unwrap()["open"],
        serde_json::json!(["a.md", "b.md"])
    );
}

#[tokio::test]
async fn a_rename_follows_the_tab_and_a_delete_drops_it_or_a_folders_notes() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-rename-delete");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["a.md", "b.md", "x/n.md"], "c.md");
    let later = tabs_cookie(&["a.md", "Renamed.md", "x/n.md"], "c.md");

    let (_, headers, body) = send(
        &app,
        tabs_post(
            "/notes/rename",
            "path=b.md&name=Renamed&open=c.md&project=",
            &cookie,
        ),
    )
    .await;
    assert_eq!(strip_paths(&body), ["a.md", "Renamed.md", "x/n.md", "c.md"]);
    assert_eq!(
        stored_tabs(&headers).unwrap()["open"],
        serde_json::json!(["a.md", "Renamed.md", "x/n.md", "c.md"])
    );

    let (_, _, body) = send(
        &app,
        tabs_post("/notes/delete", "path=a.md&open=c.md&project=", &later),
    )
    .await;
    assert_eq!(strip_paths(&body), ["Renamed.md", "x/n.md", "c.md"]);

    let (_, headers, body) = send(
        &app,
        tabs_post("/notes/delete", "path=x&open=c.md&project=", &later),
    )
    .await;
    assert_eq!(strip_paths(&body), ["Renamed.md", "c.md"]);
    assert_eq!(stored_tabs(&headers).unwrap()["active"], "c.md");
}

#[tokio::test]
async fn closing_a_tab_lands_on_the_right_neighbour_else_the_left_else_the_empty_page() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-close");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["a.md", "b.md", "c.md"], "b.md");

    let (_, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=b.md", &cookie, false),
    )
    .await;
    let close_of = |path: &str| -> String {
        let item = body
            .split(&format!("data-tab-path=\"{path}\""))
            .nth(1)
            .unwrap();
        let rest = item.split("note-tabs__close").nth(1).unwrap();
        let href = rest
            .split("href=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        href.replace("&#38;", "&")
    };
    assert_eq!(close_of("b.md"), "/notes?project=&path=c.md&close=b.md");
    assert_eq!(close_of("a.md"), "/notes?project=&path=b.md&close=a.md");

    let (_, headers, body) = send(
        &app,
        notes_get_with_cookie("/notes?project=&path=c.md&close=b.md", &cookie, true),
    )
    .await;
    assert_eq!(strip_paths(&body), ["a.md", "c.md"]);
    assert_eq!(stored_tabs(&headers).unwrap()["active"], "c.md");
    assert_eq!(
        headers
            .get("hx-replace-url")
            .and_then(|value| value.to_str().ok()),
        Some("/notes?project=&path=c.md"),
        "the address bar drops the spent close"
    );

    // The last tab closes to the left neighbour, a lone one to the empty page.
    let last = tabs_cookie(&["a.md", "c.md"], "c.md");
    let (_, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=c.md", &last, false),
    )
    .await;
    let item = body.split("data-tab-path=\"c.md\"").nth(1).unwrap();
    assert!(item.contains("path=a.md&#38;close=c.md"), "{item}");
    let lone = tabs_cookie(&["a.md"], "a.md");
    let (_, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=a.md", &lone, false),
    )
    .await;
    let item = body.split("data-tab-path=\"a.md\"").nth(1).unwrap();
    assert!(
        item.contains("href=\"/notes?project=&#38;close=a.md\""),
        "{item}"
    );
}

#[tokio::test]
async fn two_tabs_with_one_name_show_their_folders() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-labels");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["x/n.md", "y/n.md", "a.md"], "a.md");
    let (_, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=a.md", &cookie, false),
    )
    .await;
    assert!(
        body.contains("<span class=\"note-tabs__folder\">x</span>"),
        "{body}"
    );
    assert!(
        body.contains("<span class=\"note-tabs__folder\">y</span>"),
        "{body}"
    );
    assert_eq!(body.matches("note-tabs__folder\"").count(), 2);
}

#[tokio::test]
async fn the_tabs_cookie_budget_drops_the_leftmost_inactive_tab() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("tabs-budget");
    let store = sandbox.store();
    let notes = vault(&store);
    let many: Vec<String> = (0..130)
        .map(|n| format!("note-with-a-long-name-{n:03}.md"))
        .collect();
    for path in &many {
        notes.create(path).expect("the note is created");
    }
    let app = router(context(store));
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    // 101 tabs just fit a request; the open note's tab pushes the answer past
    // the budget, so the leftmost inactive tab goes.
    let near = tabs_cookie(&refs[..101], "");
    let (_, headers, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=note-with-a-long-name-129.md", &near, false),
    )
    .await;
    let stored = stored_tabs(&headers).expect("the list is answered");
    let open = stored["open"].as_array().unwrap();
    assert!(
        open.len() < 102 && open.len() > 95,
        "trimmed: {}",
        open.len()
    );
    assert_eq!(open.last().unwrap(), "note-with-a-long-name-129.md");
    assert!(
        !open
            .iter()
            .any(|path| path == "note-with-a-long-name-000.md")
    );
    assert_eq!(strip_paths(&body).len(), open.len());
}

#[tokio::test]
async fn a_bare_notes_visit_reopens_the_active_note() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-active");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["a.md", "b.md"], "b.md");
    let (status, headers, _) =
        send_once(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let target = location(&headers);
    assert_eq!(target, "/notes?project=&path=b.md");
    assert!(headers.get(header::SET_COOKIE).is_none());

    let (status, _, body) = send(&app, notes_get_with_cookie(&target, &cookie, false)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(strip_paths(&body), ["a.md", "b.md"]);
    assert!(body.contains("aria-selected=\"true\""));
    assert!(!body.contains("banner--error"), "{body}");
}

#[tokio::test]
async fn a_bare_visit_with_nothing_to_reopen_draws_the_empty_page() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-empty");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let other_port = tabs_cookie(&["a.md"], "a.md").replace("7700", "7701");
    let cookies = [
        tabs_cookie(&["a.md"], ""),
        tabs_cookie(&["a.md"], "b.md"),
        tabs_cookie(&["gone.md", "a.md"], "gone.md"),
        "tandem_7700_tabs=%7Bbroken".to_string(),
        other_port,
        String::new(),
    ];
    for cookie in cookies {
        let (status, headers, body) =
            send(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
        assert_eq!(status, StatusCode::OK, "{cookie}");
        assert!(body.contains("No note selected"), "{cookie}");
        assert!(!body.contains("banner--error"), "{cookie}");
        assert!(headers.get(header::LOCATION).is_none(), "{cookie}");
        if cookie.contains("gone.md") {
            let stored = stored_tabs(&headers).expect("the missing path is dropped");
            assert_eq!(stored["active"], "");
            assert_eq!(stored["open"], serde_json::json!(["a.md"]));
        }
    }
}

#[tokio::test]
async fn only_a_bare_notes_visit_restores() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-bare-only");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["a.md", "b.md"], "b.md");
    let (status, _, body) = send(
        &app,
        notes_get_with_cookie("/notes?path=a.md", &cookie, false),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("data-tab-path=\"a.md\""));
    for uri in ["/notes?project=", "/notes/graph"] {
        let (status, headers, _) = send(&app, notes_get_with_cookie(uri, &cookie, false)).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(headers.get(header::LOCATION).is_none(), "{uri}");
    }
}

#[tokio::test]
async fn a_hostile_active_path_never_reaches_a_location() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-hostile");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    for evil in ["//evil.example/x", "../x"] {
        let cookie = tabs_cookie(&[evil], evil);
        let (status, headers, _) =
            send(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
        assert_eq!(status, StatusCode::OK, "{evil}");
        assert!(!location(&headers).contains("evil"), "{evil}");
    }
}

#[tokio::test]
async fn the_graphs_note_switch_is_bare_without_a_focus() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-switch");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let (_, _, body) = send(&app, page_get("/notes/graph")).await;
    assert!(
        body.contains("data-view=\"note\" href=\"/notes\""),
        "{body}"
    );
    let (_, _, body) = send(&app, page_get("/notes/graph?project=Tandem")).await;
    assert!(
        body.contains("data-view=\"note\" href=\"/notes?project=Tandem\""),
        "{body}"
    );
}

#[tokio::test]
async fn the_graph_keeps_the_active_tab_so_the_note_switch_restores_it() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("restore-graph");
    let store = sandbox.store();
    tabs_vault(&store);
    let app = router(context(store));
    let cookie = tabs_cookie(&["a.md", "b.md"], "b.md");
    let (_, headers, _) = send(&app, notes_get_with_cookie("/notes/graph", &cookie, false)).await;
    assert!(stored_tabs(&headers).is_none(), "the list is unchanged");
    let (status, headers, _) =
        send_once(&app, notes_get_with_cookie("/notes", &cookie, false)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/notes?project=&path=b.md");
}

// MARK: - The label filter takes several labels (TAN-867)

/// The ticket titles a board page draws on a card, in the order given.
fn drawn(page: &str, titles: &[&str]) -> Vec<bool> {
    titles.iter().map(|title| page.contains(*title)).collect()
}

const LABELLED: [&str; 4] = ["Café alpha", "Shared alpha", "Beta one", "Loose"];

#[tokio::test]
async fn several_checked_labels_draw_the_tickets_that_carry_any_of_them() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-union");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    // One project is open at a time, so a union is read inside one project.
    let (_, _, two) = get(&app, "/?project=Alpha&label=alpha&label=shared").await;
    assert_eq!(
        drawn(&two, &LABELLED),
        [true, true, false, false],
        "two labels draw both and hide the other project's tickets: {two}"
    );

    let (_, _, mixed) = get(&app, "/?project=Elsewhere&label=%20&label=beta").await;
    assert_eq!(
        drawn(&mixed, &LABELLED),
        [false, false, false, true],
        "No label joins the union: {mixed}"
    );

    let (_, _, one) = get(&app, "/?project=Alpha&label=shared").await;
    assert_eq!(drawn(&one, &LABELLED), [false, true, false, false]);
    let (_, _, none) = get(&app, "/?project=Elsewhere&label=%20").await;
    assert_eq!(drawn(&none, &LABELLED), [false, false, false, true]);

    let (_, _, old) = get(&app, "/?project=Alpha&label=&assignee=").await;
    assert_eq!(
        drawn(&old, &LABELLED),
        [true, true, false, false],
        "an empty label= means none: {old}"
    );
}

#[tokio::test]
async fn the_label_filter_draws_a_checkbox_per_label_and_one_chip() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-boxes");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, page) = get(&app, "/?project=Alpha&label=%20&label=beta&label=alpha").await;
    assert!(
        !page.contains("name=\"label\" aria-label=\"Label\""),
        "no Label select is drawn: {page}"
    );
    assert!(
        page.contains("<legend class=\"menu__title\">Label</legend>"),
        "the group is titled Label: {page}"
    );
    for (value, checked) in [
        (" ", true),
        ("alpha", true),
        ("shared", false),
        ("beta", true),
    ] {
        let want = format!(
            "<input type=\"checkbox\" name=\"label\" value=\"{value}\"{}>",
            if checked { " checked" } else { "" }
        );
        assert!(page.contains(&want), "{want} is drawn: {page}");
    }
    assert!(!page.contains("All labels"), "All labels is gone: {page}");
    assert_eq!(
        page.matches("data-clear=\"label\"").count(),
        1,
        "one chip names the whole filter: {page}"
    );
    assert!(
        page.contains("Label: No label, alpha, beta"),
        "the chip lists No label first, then the list order: {page}"
    );
    assert!(
        page.contains("aria-label=\"Remove label filter No label, alpha, beta\""),
        "the chip's aria-label names every label: {page}"
    );
    assert!(
        page.contains("<input type=\"hidden\" name=\"label\" value=\"beta\">"),
        "the write forms carry one hidden input per label: {page}"
    );
}

#[tokio::test]
async fn a_checked_label_outside_the_focused_project_stays_listed_and_checked() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-outside-focus");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, page) = get(&app, "/?project=Alpha&label=beta").await;
    assert!(
        page.contains("<input type=\"checkbox\" name=\"label\" value=\"beta\" checked>"),
        "beta stays listed and checked under the Alpha focus: {page}"
    );
    assert!(
        page.contains("name=\"label\" value=\"alpha\">"),
        "the focused project's own labels are offered: {page}"
    );
    let (_, _, unchecked) = get(&app, "/?project=Alpha").await;
    assert!(
        !unchecked.contains("name=\"label\" value=\"beta\""),
        "an unchecked outside label is not offered: {unchecked}"
    );
}

#[tokio::test]
async fn a_card_move_and_a_sheet_open_keep_every_checked_label() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-carry");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let beta = by_title(&board, "Beta one");
    let alpha = by_title(&board, "Café alpha");
    let revision = store.revision().expect("the revision reads");

    let body = format!(
        "status=in-progress&expected_revision={revision}&project=&label=alpha&label=beta&assignee=&text="
    );
    let (status, _, page) = send(
        &app,
        form_request(&format!("/tickets/{}/status", beta.id), &body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the move answers: {page}");
    assert_eq!(
        drawn(&page, &LABELLED),
        [true, false, true, false],
        "{page}"
    );
    assert!(
        page.contains("hx-get=\"/board?project=&#38;label=alpha&#38;label=beta&#38;"),
        "the refresh URL carries both: {page}"
    );

    let (_, _, sheet) = send(
        &app,
        sheet_get(&format!("/tickets/{}?label=alpha&label=beta", alpha.id)),
    )
    .await;
    assert!(
        sheet.contains("name=\"board_label\" value=\"alpha\"")
            && sheet.contains("name=\"board_label\" value=\"beta\""),
        "the sheet opened over two labels carries both: {sheet}"
    );
}

#[tokio::test]
async fn renaming_a_checked_label_keeps_it_checked_under_its_new_name() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-rename-filter");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (status, _, page) = send(
        &app,
        form_request(
            "/labels/beta/rename",
            "to=gamma&project=&label=alpha&label=beta",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the rename answers: {page}");
    assert!(
        page.contains("<input type=\"checkbox\" name=\"label\" value=\"gamma\" checked>"),
        "gamma is checked: {page}"
    );
    assert!(
        page.contains("<input type=\"checkbox\" name=\"label\" value=\"alpha\" checked>"),
        "alpha stays checked: {page}"
    );
    assert!(page.contains("Label: alpha, gamma"), "{page}");
}

#[tokio::test]
async fn a_two_label_filter_that_matches_nothing_draws_the_empty_state() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-empty");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, page) = get(&app, "/?project=Alpha&label=beta&label=gamma").await;
    assert!(
        page.contains("No tickets match these filters"),
        "the empty state names the filters: {page}"
    );
}

#[tokio::test]
async fn the_new_form_starts_on_the_label_only_when_one_named_label_is_checked() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-label-new-preset");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, one) = get(&app, "/?project=Alpha&label=alpha").await;
    assert!(
        one.contains(&format!(
            "<option value=\"alpha\" data-project=\"Alpha\" data-tone=\"{}\" selected",
            super::board::label_tone("alpha")
        )),
        "one named label presets the New form: {one}"
    );
    let (_, _, two) = get(&app, "/?project=Alpha&label=alpha&label=shared").await;
    assert!(
        ["alpha", "shared"]
            .iter()
            .all(|name| !two.contains(&format!(
                "<option value=\"{name}\" data-project=\"Alpha\" data-tone=\"{}\" selected",
                super::board::label_tone(name)
            ))),
        "two labels leave the New form on No label: {two}"
    );
}

#[tokio::test]
async fn a_retype_after_a_conflict_posts_with_theirs_as_was() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-retype-after-conflict");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));
    board
        .set(
            &id,
            SetFields {
                assignee: Edit::Set(Some("theirs")),
                ..SetFields::default()
            },
        )
        .expect("the field is written elsewhere");

    let (_, stale) = save_field(&app, &id, &field_body("assignee", "mine", "builder")).await;
    assert!(stale.contains("data-state=\"conflict\""), "{stale}");
    // The retype posts the conflicting value as was, as Keep mine does.
    let (_, retyped) = save_field(&app, &id, &field_body("assignee", "mine2", "theirs")).await;
    assert!(retyped.contains("data-state=\"saved\""), "{retyped}");
    assert_eq!(
        board.ticket(&id).expect("reads").assignee.as_deref(),
        Some("mine2")
    );
    // The page script uses the failure's theirs as the base of the next save.
    let (_, _, script) = get(&app, "/static/tandem.js").await;
    assert!(
        script.contains(
            "failure.theirs !== undefined ? failure.theirs : control.getAttribute('data-was')"
        ),
        "saveField bases a retype on theirs"
    );
}

#[tokio::test]
async fn the_sheet_comment_box_sends_on_enter_and_keeps_shift_enter() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-comment-enter");
    let store = sandbox.store();
    let _board = setup_board(&store);
    let app = router(context(store));
    let (_, _, script) = get(&app, "/static/tandem.js").await;
    let script = script.replace("\r\n", "\n");
    for key in [
        "closest(event.target, '.sheet__addition-field')",
        "event.shiftKey || event.altKey",
        "event.isComposing || event.keyCode === 229",
        "field.value.trim() === ''",
        "htmxPost(form, '#board-root')",
    ] {
        assert!(script.contains(key), "tandem.js carries {key}");
    }
    assert!(
        !script.contains("event.shiftKey && event.key === 'Enter'"),
        "the Cmd/Ctrl+Shift+Enter post is gone"
    );
}

#[tokio::test]
async fn the_sheet_footer_hint_names_enter_to_post() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-hint-enter");
    let store = sandbox.store();
    let board = setup_board(&store);
    let id = by_title(&board, "Café alpha").id.to_string();
    let app = router(context(store));
    let (_, _, page) = send(&app, sheet_get(&format!("/tickets/{id}"))).await;
    assert!(page.contains("Enter posts a comment"), "{page}");
    assert!(page.contains("Shift+Enter adds a line"), "{page}");
    assert!(!page.contains("Cmd/Ctrl+Shift+Enter"), "{page}");
}

// MARK: - The project switcher and the remembered project (TAN-530)

/// A GET that carries a cookie header and is answered as is.
async fn get_cookie_once(app: &Router, uri: &str, cookie: &str) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7700")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("the request builds");
    send_once(app, request).await
}

/// The `last` cookie a browser sends back, as the server writes it.
fn last_cookie_header(project: &str, face: &str) -> String {
    let json = serde_json::json!({ "project": project, "face": face }).to_string();
    format!("tandem_7700_last={}", ui_state::percent_encode(&json))
}

fn set_cookies(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn the_board_header_holds_the_project_switcher_and_tabs_that_carry_the_project() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-board");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (status, headers, page) = get_once(&app, "/board?project=Alpha").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let header_html = page
        .split("</header>")
        .next()
        .expect("the shell draws a header");
    assert!(
        header_html.contains(
            "<select class=\"project-switch__select\" name=\"project\" aria-label=\"Project\">"
        ),
        "the switcher is in the header: {header_html}"
    );
    assert!(
        header_html.contains("<option value=\"Alpha\" selected>Alpha</option>"),
        "Alpha is selected: {header_html}"
    );
    assert!(
        !header_html.contains("All projects"),
        "there is no All projects option: {header_html}"
    );
    let switcher = header_html.find("project-switch__select").unwrap();
    let tabs = header_html.find("shell__tabs").unwrap();
    assert!(switcher < tabs, "the switcher sits before the tabs");
    assert!(
        header_html.contains(
            "<a class=\"shell__tab\" href=\"/board?project=Alpha\" aria-current=\"page\">Board</a>"
        ),
        "Board is current and carries the project: {header_html}"
    );
    assert!(
        header_html.contains("<a class=\"shell__tab\" href=\"/notes?project=Alpha\">Notes</a>"),
        "Notes carries the project: {header_html}"
    );
    assert!(page.contains("Café alpha") && !page.contains("Beta one"));
    assert!(
        !page.contains("focus-picker") && !page.contains("show_all") && !page.contains("Show all"),
        "no in-face picker or show-all is left: {page}"
    );
    assert!(
        page.contains("<input type=\"hidden\" name=\"project\" value=\"Alpha\">"),
        "the filter form keeps the project: {page}"
    );
    let cookies = set_cookies(&headers);
    let last = cookies
        .iter()
        .find(|cookie| cookie.starts_with("tandem_7700_last="))
        .expect("the page remembers the project");
    assert!(
        last.contains("HttpOnly") && last.contains("SameSite=Lax") && last.contains("Path=/"),
        "{last}"
    );
    assert!(last.contains("Max-Age=31536000"), "{last}");
}

#[tokio::test]
async fn the_notes_header_keeps_the_open_note_across_a_project_switch() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-notes");
    let store = sandbox.store();
    setup_board(&store);
    vault(&store)
        .create("Index.md")
        .expect("the note is created");
    let app = router(context(store));

    let (status, headers, page) = get_once(&app, "/notes?project=Beta&path=Index.md").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let header_html = page.split("</header>").next().unwrap();
    assert!(
        header_html.contains("<option value=\"Beta\" selected>Beta</option>"),
        "{header_html}"
    );
    assert!(
        header_html.contains("<input type=\"hidden\" name=\"path\" value=\"Index.md\">"),
        "the switcher form keeps the open note: {header_html}"
    );
    assert!(
        header_html.contains("href=\"/notes?project=Beta\" aria-current=\"page\">Notes</a>")
            && header_html.contains("href=\"/board?project=Beta\">Board</a>"),
        "{header_html}"
    );
    assert!(
        !page.contains("notes-focus") && !page.contains("show_all"),
        "the notes aside has no project picker: {page}"
    );
    let cookie = set_cookies(&headers)
        .into_iter()
        .find(|cookie| cookie.starts_with("tandem_7700_last="))
        .expect("the notes face remembers the project");
    assert!(cookie.contains("notes"), "{cookie}");
}

#[tokio::test]
async fn a_bare_board_or_notes_visit_goes_to_the_first_project() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-first");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (status, headers, _) = get_once(&app, "/board").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/board?project=Alpha");
    let (status, headers, _) = get_once(&app, "/notes").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/notes?project=Alpha");
    let (status, headers, _) = get_once(&app, "/").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/board?project=Alpha");
}

#[tokio::test]
async fn the_remembered_project_and_face_decide_where_a_bare_visit_lands() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-remember");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let notes_cookie = last_cookie_header("Beta", "notes");
    let (status, headers, _) = get_cookie_once(&app, "/", &notes_cookie).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/notes?project=Beta");
    let (_, headers, _) = get_cookie_once(&app, "/board", &notes_cookie).await;
    assert_eq!(
        location(&headers),
        "/board?project=Beta",
        "a face route keeps its face and takes the project"
    );

    // An explicit project wins over the cookie, and is remembered in turn.
    let (status, headers, _) =
        get_cookie_once(&app, "/board?project=Elsewhere", &notes_cookie).await;
    assert_eq!(status, StatusCode::OK);
    let written = set_cookies(&headers).join(";");
    assert!(written.contains("Elsewhere"), "{written}");

    // The page's own cookie sends a later bare visit to the same project.
    let (_, headers, _) = get_once(&app, "/board?project=Beta").await;
    let written = set_cookies(&headers)
        .into_iter()
        .find(|cookie| cookie.starts_with("tandem_7700_last="))
        .expect("the cookie is written");
    let pair = written.split(';').next().unwrap();
    let (status, headers, _) = get_cookie_once(&app, "/board", pair).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/board?project=Beta");
}

#[tokio::test]
async fn an_unknown_project_or_a_stale_cookie_redirects_to_one_that_exists() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-unknown");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (status, headers, _) = get_once(&app, "/board?project=Ghost&text=x").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/board?project=Alpha&text=x");

    let beta = last_cookie_header("Beta", "board");
    let (_, headers, _) = get_cookie_once(&app, "/board?project=Ghost", &beta).await;
    assert_eq!(location(&headers), "/board?project=Beta");

    let deleted = last_cookie_header("Deleted", "notes");
    let (_, headers, _) = get_cookie_once(&app, "/", &deleted).await;
    assert_eq!(
        location(&headers),
        "/board?project=Alpha",
        "a cookie naming a deleted project is ignored, face and all"
    );

    // The retired show-all flag is ignored: the named project is drawn.
    let (status, _, page) = get_once(&app, "/board?project=Alpha&show_all=1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!page.contains("Beta one"), "{page}");
}

#[tokio::test]
async fn with_no_projects_both_faces_draw_a_disabled_switcher_and_do_not_redirect() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("switch-none");
    let app = router(context(sandbox.store()));

    for uri in ["/board", "/notes"] {
        let (status, _, page) = get_once(&app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {page}");
        assert!(
            page.contains("name=\"project\" aria-label=\"Project\" disabled"),
            "{uri} draws a disabled switcher: {page}"
        );
        assert!(page.contains("No projects"), "{uri}: {page}");
    }
}
