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
            body.contains("+ Add a card"),
            "every column carries its add"
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
