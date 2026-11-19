//! The change stream.

use super::guard::{self, Session};
use super::*;

/// How often a stream of a signed-in browser re-reads its session, so a sign
/// out or an expiry closes it without waiting for the next change.
#[cfg(not(test))]
const SESSION_RECHECK: Duration = Duration::from_secs(5);
#[cfg(test)]
const SESSION_RECHECK: Duration = Duration::from_millis(40);

/// Logs the stream's close. It lives in the stream's closure, so it drops with
/// the response when the client leaves.
struct StreamClosed;

impl Drop for StreamClosed {
    fn drop(&mut self) {
        tracing::debug!("events stream closed");
    }
}

/// One step of the stream: a frame to send, nothing, or the end.
enum Step {
    Send(SseEvent),
    Skip,
    End,
}

/// What wakes the stream.
enum Wake {
    Revision(Option<i64>),
    Recheck,
    /// A credential changed in this process: a removal or a revocation.
    Credentials,
}

pub(super) async fn events(
    context: Context,
    session: Option<Extension<Session>>,
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
    // A stream a session opened ends with that session: it re-reads the
    // session before every frame and on a timer, and delivers nothing more
    // once the session is gone. A bearer stream has no session to lose.
    let digest = session.map(|Extension(session)| session.digest);
    let store = context.store.clone();
    let revisions = BroadcastStream::new(subscription).map(|item| match item {
        Ok(revision) => Wake::Revision(Some(revision)),
        // A slow reader lagged past the channel: the honest answer is the
        // store's current revision, which makes it re-fetch the board.
        Err(_) => Wake::Revision(None),
    });
    let ticks = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(SESSION_RECHECK))
        .map(|_| Wake::Recheck);
    // A removal or a revocation in this process closes a stream at once; one
    // made by another process (a local CLI) is caught by the next re-check.
    let credentials =
        BroadcastStream::new(crate::store::credentials_changed()).map(|_| Wake::Credentials);
    let updates = revisions
        .merge(ticks)
        .merge(credentials)
        .then(move |wake| {
            let digest = digest.clone();
            let store = store.clone();
            let lagged = lagged.clone();
            async move {
                if let Some(digest) = &digest
                    && !guard::session_live(&store, digest).await
                {
                    return Step::End;
                }
                match wake {
                    Wake::Recheck | Wake::Credentials => Step::Skip,
                    Wake::Revision(Some(revision)) => Step::Send(
                        SseEvent::default()
                            .event("changed")
                            .data(revision.to_string()),
                    ),
                    Wake::Revision(None) => {
                        tracing::debug!("events stream lagged");
                        Step::Send(
                            SseEvent::default()
                                .event("changed")
                                .data(lagged.revision().to_string()),
                        )
                    }
                }
            }
        })
        .take_while(|step| !matches!(step, Step::End))
        .filter_map(move |step| {
            let _ = &closed;
            match step {
                Step::Send(event) => Some(Ok(event)),
                Step::Skip | Step::End => None,
            }
        });
    Sse::new(initial.chain(updates)).keep_alive(KeepAlive::default())
}
