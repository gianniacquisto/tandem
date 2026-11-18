//! `GET /export`: the home as one streamed `.tar.gz` (CUT-38, ADR 0050).
//!
//! The route is a face over [`crate::archive`]: the one permission check
//! decides, the export record is written when an export starts, and the
//! synchronous writer runs under `spawn_blocking`, feeding a bounded channel
//! that is the response body. The archive is never held whole, a slow client
//! applies back-pressure through the channel, and the store copy is staged
//! before the first byte, so no lock spans the download.

use std::fmt;
use std::io::{self, Write};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use super::Context;
use super::guard::refuse;
use crate::archive::{self, ArchiveError, Door, Principal};

/// The bytes a chunk carries before it is handed to the body.
const CHUNK: usize = 64 * 1024;

/// Chunks in flight between the writer and the body: the back-pressure bound.
const IN_FLIGHT: usize = 4;

type Check = dyn Fn(&Principal) -> Result<(), ArchiveError> + Send + Sync;

/// The export permission check, as router state: the one core check by default
/// and an injected one in a test. Nothing else decides who may export.
#[derive(Clone)]
pub struct ExportCheck(Arc<Check>);

impl ExportCheck {
    /// The check a running serve uses: [`archive::may_export`].
    pub fn standard() -> Self {
        Self(Arc::new(archive::may_export))
    }

    /// A check of the caller's own, for a test that needs a refusal.
    pub fn new(
        check: impl Fn(&Principal) -> Result<(), ArchiveError> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(check))
    }

    pub(super) fn allows(&self, principal: &Principal) -> Result<(), ArchiveError> {
        (self.0)(principal)
    }
}

impl fmt::Debug for ExportCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExportCheck")
    }
}

impl Default for ExportCheck {
    fn default() -> Self {
        Self::standard()
    }
}

type Chunk = Result<Bytes, String>;

/// A `Write` over the bounded channel: it batches into [`CHUNK`]s and blocks the
/// blocking thread when the client is slow. A gone client is a broken pipe, which
/// stops the export.
struct ChannelWriter {
    sender: mpsc::Sender<Chunk>,
    buffer: Vec<u8>,
}

impl ChannelWriter {
    fn send(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::take(&mut self.buffer));
        self.sender
            .blocking_send(Ok(chunk))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the client went away"))
    }
}

impl Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(data);
        if self.buffer.len() >= CHUNK {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send()
    }
}

/// Whether a `Sec-Fetch-Site` value may download: the page's own origin, or no
/// page at all (a typed address). An absent header passes (curl, the bearer).
fn site_allowed(headers: &HeaderMap) -> bool {
    match headers.get("sec-fetch-site") {
        None => true,
        Some(value) => matches!(value.to_str(), Ok("same-origin" | "none")),
    }
}

pub(super) async fn export(context: Context, headers: HeaderMap) -> Response {
    if !site_allowed(&headers) {
        return refuse(
            StatusCode::FORBIDDEN,
            "The export must be asked for from the board itself. Open the board and use Export workspace.",
            "the export request came from another site",
        );
    }
    let principal = context.export_principal();
    if let Err(error) = context.export_check.allows(&principal) {
        return refuse(
            StatusCode::FORBIDDEN,
            &error.to_string(),
            "the export permission refused",
        );
    }
    archive::record_export(&principal.actor, Door::Board);

    let home = context.store.home().clone();
    let (sender, mut receiver) = mpsc::channel::<Chunk>(IN_FLIGHT);
    tokio::task::spawn_blocking(move || {
        let mut writer = ChannelWriter {
            sender: sender.clone(),
            buffer: Vec::new(),
        };
        let outcome = archive::export(&home, &mut writer)
            .map_err(|error| error.to_string())
            .and_then(|_| writer.flush().map_err(|error| error.to_string()));
        if let Err(message) = outcome {
            tracing::warn!(error = %message, "the export stopped");
            // Before the first byte this is the page's answer; after it, the
            // body's error, which aborts the connection.
            let _ = sender.blocking_send(Err(message));
        }
    });

    // Wait for the first message: the staging has happened by then, so a
    // failure before the first byte is still a readable answer.
    let first = match receiver.recv().await {
        Some(Ok(bytes)) => bytes,
        Some(Err(message)) => {
            tracing::warn!("the export failed before its first byte");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("The export could not be made: {message}\n"),
            )
                .into_response();
        }
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "The export could not be made: the writer stopped. Try again.\n",
            )
                .into_response();
        }
    };
    let rest = ReceiverStream::new(receiver);
    let stream = tokio_stream::once(Ok::<Bytes, String>(first))
        .chain(rest)
        .map(|chunk| chunk.map_err(io::Error::other));

    let date: String = crate::store::now().chars().take(10).collect();
    let disposition = format!("attachment; filename=\"tandem-export-{date}.tar.gz\"");
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
