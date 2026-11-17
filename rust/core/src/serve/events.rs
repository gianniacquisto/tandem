//! The change stream.

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
    // current number before any new change. The subscription comes first: a
    // write broadcast between reading the revision and subscribing would
    // otherwise reach neither the initial event nor the stream.
    let subscription = changes.subscribe();
    let current = context.revision();
    tracing::debug!(revision = current, "events stream opened");
    let closed = StreamClosed;
    let initial = tokio_stream::once(Ok::<_, Infallible>(
        SseEvent::default()
            .event("changed")
            .data(current.to_string()),
    ));
    let lagged = context.clone();
    let updates = BroadcastStream::new(subscription).filter_map(move |item| {
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
