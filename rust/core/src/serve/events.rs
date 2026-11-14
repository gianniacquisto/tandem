//! The change stream and the static assets the shell loads.

use super::*;

/// Logs the stream's close. It lives in the stream's closure, so it drops with
/// the response when the client leaves.
struct StreamClosed;

impl Drop for StreamClosed {
    fn drop(&mut self) {
        tracing::debug!("events stream closed");
    }
}

pub(super) async fn events(
    context: Context,
    Extension(changes): Extension<broadcast::Sender<i64>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    // The stream opens on the revision the board was drawn from, so a client
    // that attaches after a write (a reload, a restored store) still reads the
    // current number before any new change.
    let current = context.revision();
    tracing::debug!(revision = current, "events stream opened");
    let closed = StreamClosed;
    let initial = tokio_stream::once(Ok::<_, Infallible>(
        SseEvent::default()
            .event("changed")
            .data(current.to_string()),
    ));
    let lagged = context.clone();
    let updates = BroadcastStream::new(changes.subscribe()).filter_map(move |item| {
        let _ = &closed;
        match item {
            Ok(revision) => Some(Ok(SseEvent::default()
                .event("changed")
                .data(revision.to_string()))),
            // A slow reader lagged past the channel: the honest answer is the
            // store's current revision, which makes it re-fetch the board.
            Err(_) => {
                tracing::debug!("events stream lagged");
                let revision = lagged.revision();
                Some(Ok(SseEvent::default()
                    .event("changed")
                    .data(revision.to_string())))
            }
        }
    });
    Sse::new(initial.chain(updates)).keep_alive(KeepAlive::default())
}

pub(super) async fn static_asset(Path(path): Path<String>) -> Response {
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
        "theme.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("static/theme.js"),
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
