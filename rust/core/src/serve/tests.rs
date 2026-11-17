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
        .add_label("alpha", Some("ALP"), Some("Alpha"))
        .expect("the label is added");
    board
        .add_label("shared", Some("SHA"), Some("Alpha"))
        .expect("the label is added");
    board
        .add_label("beta", Some("BET"), Some("Beta"))
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

    let (_, _, every) = get(&app, "/?project=Alpha&show_all=1").await;
    assert!(
        every.contains("Beta one"),
        "show all escapes the focus without clearing it: {every}"
    );

    let (_, _, label) = get(&app, "/?label=beta").await;
    assert!(label.contains("Beta one"));
    assert!(
        !label.contains("Café alpha"),
        "the label filter narrows: {label}"
    );

    let (_, _, loose) = get(&app, "/?label=%20").await;
    assert!(loose.contains("Loose"), "the default area is selectable");
    assert!(
        !loose.contains("Beta one"),
        "the default area is one label: {loose}"
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
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("board-add");
    let store = sandbox.store();
    let board = setup_board(&store);
    let app = router(context(store.clone()));
    let before = store.revision().expect("the revision reads");

    let body = format!(
        "title=First&status=ready&label=&expected_revision={before}&project=&show_all=&assignee=&text="
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
        "title=Second&status=backlog&label=&expected_revision={before}&project=&show_all=&assignee=&text="
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
        page.contains("<option value=\"beta\" selected>beta</option>"),
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
        script.contains("tandem.noteTabs") && script.contains("data-note-tabs"),
        "the script carries the note tabs"
    );
    let (_, _, css) = get(&app, "/static/app.css").await;
    assert!(
        css.contains(".note-tabs__tab[aria-selected=\"true\"]"),
        "the stylesheet draws the active note tab"
    );
}

#[tokio::test]
async fn post_routes_reject_missing_form_content_type_and_cross_origin_header() {
    let _serial = ONE_AT_A_TIME.lock().await;
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
    layered(extra, changes, Access::loopback(7700)).with_state(Arc::new(context(store)))
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
        sheet.contains("data-sheet-label"),
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
    let _serial = ONE_AT_A_TIME.lock().await;
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
    let _serial = ONE_AT_A_TIME.lock().await;
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

    let revision = board.store().revision().expect("the revision reads");
    let (status, _, answer) = send(
        &app,
        form_request(
            &format!("/tickets/{id}/label"),
            &format!("label=shared&expected_revision={revision}"),
        ),
    )
    .await;
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

    let (status, _, answer) = send(&app, form_request("/labels", "name=gamma&abbr=GAM")).await;
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

    let (status, _, answer) = send(
        &app,
        form_request("/labels/gamma/rename", "to=delta&abbr=DEL"),
    )
    .await;
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
        !focused.contains("data-path=\"projects/other/outside.md\""),
        "the focused tree drops another project's folder: {focused}"
    );
}

#[tokio::test]
async fn the_edit_page_keeps_a_leading_blank_line_in_the_note_text() {
    let _serial = ONE_AT_A_TIME.lock().await;
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
        body.contains("data-gone=\"projects/tandem/Final.md\""),
        "the delete names the path for the tab strip: {body}"
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
async fn the_notes_page_carries_the_tab_strip_hooks_and_rename_names_both_paths() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("notes-tabs");
    let store = sandbox.store();
    let app = router(context(store));

    send(
        &app,
        form_request(
            "/notes/create",
            "path=projects/tandem/Draft.md&project=tandem",
        ),
    )
    .await;
    send(
        &app,
        form_request("/notes/create", "path=Other.md&project=tandem"),
    )
    .await;

    let (status, _, page) = get(&app, "/notes?path=projects/tandem/Draft.md&project=tandem").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("data-note-tabs"), "the strip mounts: {page}");
    assert!(
        page.contains("role=\"tablist\""),
        "the strip is a tablist: {page}"
    );
    assert!(
        page.contains("data-current=\"projects/tandem/Draft.md\"")
            && page.contains("data-current-name=\"Draft\""),
        "the open note's path and name are hooks: {page}"
    );
    assert!(
        page.contains("Other.md") && page.contains("data-notes="),
        "every note is listed for pruning, whatever the focus: {page}"
    );

    let (_, _, empty) = get(&app, "/notes?project=tandem").await;
    assert!(
        empty.contains("data-note-tabs") && empty.contains("data-current=\"\""),
        "the strip mounts with no note open: {empty}"
    );

    let (_, _, renamed) = send(
        &app,
        form_request(
            "/notes/rename",
            "path=projects/tandem/Draft.md&name=Final&open=Other.md&project=tandem&mode=edit",
        ),
    )
    .await;
    assert!(
        renamed.contains("data-from=\"projects/tandem/Draft.md\"")
            && renamed.contains("data-to=\"projects/tandem/Final.md\""),
        "a rename of another row still names both paths: {renamed}"
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

/// A write from the sheet answers with the board it was opened over: the
/// filters the form carried come back, so a save never silently clears them.
#[tokio::test]
async fn a_sheet_write_keeps_the_filters_its_form_carried() {
    let _serial = ONE_AT_A_TIME.lock().await;
    let sandbox = Sandbox::new("sheet-write-filters");
    let store = sandbox.store();
    let board = setup_board(&store);
    let ticket = by_title(&board, "Café alpha");
    let revision = store.revision().expect("the revision reads");
    let app = router(context(store));

    let body = format!(
        "title=Caf%C3%A9+alpha&status={}&assignee=&description=A+line&expected_revision={revision}&board_project=Alpha&board_show_all=&board_label=alpha&board_assignee=builder&board_text=Caf",
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
    let _serial = ONE_AT_A_TIME.lock().await;
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
        "board_project=&board_show_all=&board_label=&board_assignee=&board_text=\
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
        page.contains("href=\"/board?project=&#38;show_all=&#38;label=&#38;assignee=&#38;text=\""),
        "the board behind keeps the filter the form carried, empty: {page}"
    );
    assert!(
        !page.contains("<option value=\"builder\" selected>builder</option>"),
        "the ticket's assignee is not the board's filter: {page}"
    );

    // The label form's way round: its select is the ticket's label, its hidden
    // field the board's filter.
    let revision = board.store().revision().expect("the revision reads");
    let body = format!(
        "expected_revision={revision}&board_project=&board_show_all=&board_label=\
         &board_assignee=&board_text=&label=shared"
    );
    let (status, _, page) = send(&app, form_request(&format!("/tickets/{id}/label"), &body)).await;
    assert_eq!(status, StatusCode::OK, "the label move answers: {page}");
    assert_eq!(
        board
            .ticket(&id)
            .expect("the ticket reads")
            .label
            .as_deref(),
        Some("shared"),
        "the label still moves"
    );
    assert!(
        page.contains("href=\"/board?project=&#38;show_all=&#38;label=&#38;assignee=&#38;text=\""),
        "the moved label is not the board's label filter: {page}"
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
        "title=SecretTitleWords&status=ready&label=&expected_revision={before}&project=&show_all=&assignee=&text="
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
            "name=a%0Ab%1B%5B31m&abbr=&project=&show_all=&assignee=&text=",
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
    notes.write("Index.md", "one").expect("the note is written");
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
