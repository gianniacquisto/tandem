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
    }
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
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
        ("in-review", "In Review"),
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
    board.add_project("Alpha").expect("the project is added");
    board.add_project("Beta").expect("the project is added");
    board
        .add_lane("alpha", Some("ALP"), Some("Alpha"))
        .expect("the lane is added");
    board
        .add_lane("shared", Some("SHA"), Some("Alpha"))
        .expect("the lane is added");
    board
        .add_lane("beta", Some("BET"), Some("Beta"))
        .expect("the lane is added");
    board
        .add(
            "Café alpha",
            AddTicket {
                lane: Some("alpha"),
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
                lane: Some("beta"),
                status: Status::Backlog,
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
        .add(
            "Loose",
            AddTicket {
                status: Status::Backlog,
                ..AddTicket::default()
            },
        )
        .expect("the ticket is added");
    board
        .add(
            "Shared alpha",
            AddTicket {
                lane: Some("shared"),
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
        ("in-review", "In Review"),
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

    let (status, _, body) = get(&app, "/").await;
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
async fn get_board_filters_by_project_focus_lane_assignee_and_accent_folded_search() {
    let sandbox = Sandbox::new("board-filters");
    let store = sandbox.store();
    setup_board(&store);
    let app = router(context(store));

    let (_, _, focus) = get(&app, "/?project=Alpha").await;
    assert!(focus.contains("Café alpha"), "the focus draws its lane");
    assert!(
        focus.contains("Shared alpha"),
        "the focus draws every lane of the project"
    );
    assert!(
        !focus.contains("Beta one"),
        "the focus hides the other project: {focus}"
    );

    let (_, _, every) = get(&app, "/?project=Alpha&show_all=1").await;
    assert!(
        every.contains("Beta one"),
        "show all escapes the focus without clearing it: {every}"
    );

    let (_, _, lane) = get(&app, "/?lane=beta").await;
    assert!(lane.contains("Beta one"));
    assert!(
        !lane.contains("Café alpha"),
        "the lane filter narrows: {lane}"
    );

    let (_, _, loose) = get(&app, "/?lane=%20").await;
    assert!(loose.contains("Loose"), "the default area is selectable");
    assert!(
        !loose.contains("Beta one"),
        "the default area is one lane: {loose}"
    );

    let (_, _, assigned) = get(&app, "/?assignee=builder").await;
    assert!(assigned.contains("Café alpha"));
    assert!(
        !assigned.contains("Beta one"),
        "the assignee filter narrows: {assigned}"
    );

    let (_, _, searched) = get(&app, "/?text=cafe").await;
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
    let sandbox = Sandbox::new("board-add");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let before = store.revision().expect("the revision reads");

    let body = format!(
        "title=First&status=ready&lane=&expected_revision={before}&project=&show_all=&assignee=&text="
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
        "title=Second&status=backlog&lane=&expected_revision={before}&project=&show_all=&assignee=&text="
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

/// The board refreshes itself over the change stream, so the client must be the
/// htmx 2 extension: the htmx 1 build defines its `hx-trigger="sse:*"` listener
/// and never registers it, reads the whole attribute as the event name, and
/// swaps through `api.selectAndSwap`, which htmx 2 does not expose.
#[tokio::test]
async fn the_change_stream_client_is_the_htmx_two_extension() {
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
/// project, lane, assignee and search rather than clearing them.
#[tokio::test]
async fn a_card_move_keeps_the_filters_the_board_was_drawn_with() {
    let sandbox = Sandbox::new("board-move-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let ticket = by_title(&board, "Beta one");
    let revision = store.revision().expect("the revision reads");
    let uri = format!("/tickets/{}/status", ticket.id);

    let body = format!(
        "status=in-progress&expected_revision={revision}&project=Beta&show_all=&lane=beta&assignee=&text=one"
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
        page.contains("<option value=\"beta\" selected>beta</option>"),
        "the lane filter survives the drop: {page}"
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
}

#[tokio::test]
async fn post_routes_reject_missing_form_content_type_and_cross_origin_header() {
    let sandbox = Sandbox::new("board-guards");
    let store = sandbox.store();
    let app = router(context(store.clone()));
    let revision = store.revision().expect("the revision reads");
    let body = format!("title=Guarded&status=backlog&expected_revision={revision}");

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
    layered(extra, changes).with_state(Arc::new(context(store)))
}

#[tokio::test]
async fn write_guard_is_a_layer_so_a_route_cannot_skip_it_by_omission() {
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
    let sandbox = Sandbox::new("board-static");
    let app = router(context(sandbox.store()));

    for (path, kind) in [
        ("/static/tokens.css", "text/css; charset=utf-8"),
        ("/static/app.css", "text/css; charset=utf-8"),
        ("/static/htmx.min.js", "text/javascript; charset=utf-8"),
        ("/static/sse.js", "text/javascript; charset=utf-8"),
        ("/static/tandem.js", "text/javascript; charset=utf-8"),
    ] {
        let (status, headers, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::OK, "{path} is served");
        assert_eq!(
            headers[header::CONTENT_TYPE],
            kind,
            "{path} carries its type"
        );
        assert!(!body.is_empty(), "{path} carries its bytes");
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

    let (status, _, _) = get(&app, "/static/unknown.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "an unknown asset is a 404");
}

// MARK: - The watch

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

// MARK: - Seam 1b: the ticket sheet and the bikeshed notes

use crate::bikeshed::Bikeshed;
use crate::domain::{Edit, SetFields};

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

fn vault(store: &Store) -> Bikeshed {
    Bikeshed::new(store.home().directory().join("bikeshed"))
}

#[tokio::test]
async fn get_ticket_sheet_renders_markdown_blocks_metadata_row_and_comments() {
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
        "name=\"lane\"",
        "Café alpha",
        "sheet__description-field",
        "## Why",
        "A first comment",
        "Blocked by",
        "Blocks",
    ] {
        assert!(body.contains(marker), "the sheet draws {marker}: {body}");
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
async fn post_ticket_sheet_saves_changes_and_refuses_stale_revision() {
    let sandbox = Sandbox::new("sheet-save");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let revision = store.revision().expect("the revision reads");
    let app = router(context(store));

    let body =
        format!("title=Renamed&status=in-review&assignee=reviewer&expected_revision={revision}");
    let (status, _, answer) = send(&app, form_request(&format!("/tickets/{id}"), &body)).await;
    assert_eq!(status, StatusCode::OK, "the save answers: {answer}");
    let saved = board.ticket(&id).expect("the ticket reads");
    assert_eq!(saved.title, "Renamed");
    assert_eq!(saved.status, Status::InReview);
    assert_eq!(saved.assignee.as_deref(), Some("reviewer"));
    assert!(
        answer.contains("Renamed"),
        "the sheet redraws the save: {answer}"
    );

    let stale =
        format!("title=Overwritten&status=done&assignee=human&expected_revision={revision}");
    let (status, _, answer) = send(&app, form_request(&format!("/tickets/{id}"), &stale)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        answer.contains("sheet__notice"),
        "a stale save draws a notice: {answer}"
    );
    assert_eq!(
        board.ticket(&id).expect("the ticket reads").title,
        "Renamed",
        "a stale save overwrites nothing"
    );
}

#[tokio::test]
async fn saving_the_sheet_replaces_the_description_with_the_edited_box() {
    let sandbox = Sandbox::new("sheet-description");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    board
        .set(
            &id,
            SetFields {
                description: Edit::Set(Some("First")),
                ..SetFields::default()
            },
        )
        .expect("the description is set");
    let app = router(context(store));

    let revision = board.store().revision().expect("the revision reads");
    let (status, _, answer) = send(
        &app,
        form_request(
            &format!("/tickets/{id}"),
            &format!(
                "title=Caf%C3%A9+alpha&status={}&assignee=&description=Rewritten+body&expected_revision={revision}",
                ticket.status.as_str()
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the save answers: {answer}");
    assert_eq!(
        board
            .ticket(&id)
            .expect("the ticket reads")
            .description
            .as_deref(),
        Some("Rewritten body"),
        "the box replaces the stored body"
    );
    assert!(
        answer.contains("sheet__description-field"),
        "the sheet draws the description as one editable box: {answer}"
    );
}

#[tokio::test]
async fn post_ticket_comment_appends_as_human_and_refuses_empty_body() {
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
async fn post_ticket_lane_moves_immediately_and_lane_add_and_rename_update_choices() {
    let sandbox = Sandbox::new("sheet-lane");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let app = router(context(store));

    let revision = board.store().revision().expect("the revision reads");
    let (status, _, answer) = send(
        &app,
        form_request(
            &format!("/tickets/{id}/lane"),
            &format!("lane=shared&expected_revision={revision}"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the lane move answers: {answer}");
    assert_eq!(
        board.ticket(&id).expect("the ticket reads").lane.as_deref(),
        Some("shared"),
        "the lane moves at once"
    );

    let (status, _, answer) = send(&app, form_request("/lanes", "name=gamma&abbr=GAM")).await;
    assert_eq!(status, StatusCode::OK, "the lane add answers: {answer}");
    assert!(
        board
            .lanes()
            .expect("the lanes read")
            .iter()
            .any(|lane| lane == "gamma"),
        "the lane is declared"
    );
    assert!(
        answer.contains("value=\"gamma\""),
        "the board's lane menu carries the new lane: {answer}"
    );

    let (status, _, answer) = send(
        &app,
        form_request("/lanes/gamma/rename", "to=delta&abbr=DEL"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the lane rename answers: {answer}");
    let lanes = board.lanes().expect("the lanes read");
    assert!(
        lanes.iter().any(|lane| lane == "delta"),
        "the lane is renamed"
    );
    assert!(
        !lanes.iter().any(|lane| lane == "gamma"),
        "the old name is gone"
    );
    assert!(
        answer.contains("delta"),
        "the board redraws the rename: {answer}"
    );

    let (status, _, answer) = send(&app, form_request("/lanes", "name=alpha&abbr=ALP")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        answer.contains("shell__notice"),
        "a duplicate lane draws a notice: {answer}"
    );
    assert!(
        answer.contains("already exists"),
        "the notice carries the domain's wording: {answer}"
    );
}

#[tokio::test]
async fn get_notes_renders_tree_focus_breadcrumb_and_markdown_preview_with_footer() {
    let sandbox = Sandbox::new("notes-render");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes
        .write("Index.md", "## Hello\n\nBody text")
        .expect("the note is written");
    notes.create("Other.md").expect("the note is created");
    notes
        .write("Other.md", "See [[Index]]")
        .expect("the note is written");
    notes
        .create("projects/tandem/inside.md")
        .expect("the note is created");
    notes
        .create("projects/other/outside.md")
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
        "bikeshed__breadcrumb",
        "Bikeshed",
        "markdown__heading",
        "Hello",
        "Body text",
        "Backlinks",
        "Other",
        "Tickets",
        "Café alpha",
        "data-live",
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

    let (_, _, focused) = send(&app, page_get("/notes?project=tandem&path=Index.md")).await;
    assert!(
        focused.contains("inside"),
        "the focused tree keeps the project's folder: {focused}"
    );
    assert!(
        !focused.contains("outside"),
        "the focused tree drops another project's folder: {focused}"
    );
}

#[tokio::test]
async fn the_edit_page_keeps_a_leading_blank_line_in_the_note_text() {
    let sandbox = Sandbox::new("notes-leading-newline");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.create("Lead.md").expect("the note is created");
    notes
        .write("Lead.md", "\n\nfirst line")
        .expect("the note is written");
    let app = router(context(store));

    let (status, _, body) = send(&app, page_get("/notes/edit?path=Lead.md")).await;
    assert_eq!(status, StatusCode::OK, "the edit page answers: {body}");
    // The HTML parser drops one newline right after the opening tag, so the
    // template emits a spare one and the text keeps its own.
    assert!(
        body.contains("hidden>\n\n\nfirst line</textarea>"),
        "the textarea keeps the leading blank lines: {body}"
    );
}

#[tokio::test]
async fn post_notes_preview_renders_markdown_blocks_without_writing_disk() {
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
    let sandbox = Sandbox::new("notes-save");
    let store = sandbox.store();
    let notes = vault(&store);
    notes.create("Index.md").expect("the note is created");
    notes.write("Index.md", "one").expect("the note is written");
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
        .write("Index.md", "theirs")
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
}

#[tokio::test]
async fn post_notes_new_files_untitled_in_the_current_folder_with_the_title_focused() {
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

#[tokio::test]
async fn post_notes_create_create_folder_and_move_update_vault_and_refuse_invalid_reparent() {
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
/// them, so the project, lane, assignee and search survive the click.
#[tokio::test]
async fn get_ticket_sheet_keeps_the_board_filters_behind_it() {
    let sandbox = Sandbox::new("sheet-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let app = router(context(store));

    let (status, _, body) = send(
        &app,
        sheet_get(&format!(
            "/tickets/{}?project=Alpha&show_all=&lane=alpha&assignee=builder&text=Caf",
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

/// A write from the sheet answers with the board it was opened over: the
/// filters the form carried come back, so a save never silently clears them.
#[tokio::test]
async fn a_sheet_write_keeps_the_filters_its_form_carried() {
    let sandbox = Sandbox::new("sheet-write-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let revision = store.revision().expect("the revision reads");
    let app = router(context(store));

    let body = format!(
        "title=Caf%C3%A9+alpha&status={}&assignee=&description=A+line&expected_revision={revision}&board_project=Alpha&board_show_all=&board_lane=alpha&board_assignee=builder&board_text=Caf",
        ticket.status.as_str()
    );
    let (status, _, page) = send(
        &app,
        form_request(&format!("/tickets/{}", ticket.id), &body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the save answers: {page}");
    assert!(page.contains("A line"), "the description landed: {page}");
    assert!(
        page.contains("<option value=\"Alpha\" selected>Alpha</option>"),
        "the project filter survives a sheet write: {page}"
    );
    assert!(
        page.contains("href=\"/board?project=Alpha"),
        "the close link still lands on the filtered board: {page}"
    );
}

/// A write form carries the board's filter under `board_` names and the
/// ticket's own fields under theirs, and the server keeps them apart. A save
/// reads its `assignee` as the ticket's, and the board behind comes back
/// unfiltered; before the split both lived under `assignee`, the last value won
/// and closing the sheet landed on the board filtered to that ticket's owner.
#[tokio::test]
async fn a_sheet_write_reads_the_board_filter_from_its_own_fields_not_the_tickets() {
    let sandbox = Sandbox::new("sheet-write-filter-names");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let id = ticket.id.to_string();
    let revision = store.revision().expect("the revision reads");
    let app = router(context(store));

    // The browser encodes a form in document order: the filter's hidden fields
    // first, then the ticket's title, revision, status and assignee.
    let body = format!(
        "board_project=&board_show_all=&board_lane=&board_assignee=&board_text=\
         &title=Café alpha&expected_revision={revision}&status=backlog&assignee=builder"
    );
    let (status, _, page) = send(&app, form_request(&format!("/tickets/{id}"), &body)).await;
    assert_eq!(status, StatusCode::OK, "the save answers: {page}");
    assert_eq!(
        board
            .ticket(&id)
            .expect("the ticket reads")
            .assignee
            .as_deref(),
        Some("builder"),
        "the save still sets the ticket's assignee"
    );
    assert!(
        page.contains("href=\"/board?project=&#38;show_all=&#38;lane=&#38;assignee=&#38;text=\""),
        "the board behind keeps the filter the form carried, empty: {page}"
    );
    assert!(
        !page.contains("<option value=\"builder\" selected>builder</option>"),
        "the ticket's assignee is not the board's filter: {page}"
    );

    // The lane form's way round: its select is the ticket's lane, its hidden
    // field the board's filter.
    let revision = board.store().revision().expect("the revision reads");
    let body = format!(
        "expected_revision={revision}&board_project=&board_show_all=&board_lane=\
         &board_assignee=&board_text=&lane=shared"
    );
    let (status, _, page) = send(&app, form_request(&format!("/tickets/{id}/lane"), &body)).await;
    assert_eq!(status, StatusCode::OK, "the lane move answers: {page}");
    assert_eq!(
        board.ticket(&id).expect("the ticket reads").lane.as_deref(),
        Some("shared"),
        "the lane still moves"
    );
    assert!(
        page.contains("href=\"/board?project=&#38;show_all=&#38;lane=&#38;assignee=&#38;text=\""),
        "the moved lane is not the board's lane filter: {page}"
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
