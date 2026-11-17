//! The static assets the shell loads, each under a content-hashed URL.
//!
//! The bytes are compiled in with `include_bytes!`, so a hash taken once from
//! them is the bytes' identity for the life of the process. The shell links
//! `/static/<name>?v=<hash>`: a request carrying the current hash is immutable
//! for a year, and a request without it (or with a stale one) revalidates
//! against the same hash as its ETag. A changed asset changes its hash, so a
//! new binary never serves an old URL's bytes under a year-long cache.

use std::sync::LazyLock;

use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

/// A hashed asset's cache policy: its URL changes with its bytes.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// An unhashed or stale request: the browser must ask before reusing it.
const REVALIDATE: &str = "no-cache";

struct Asset {
    name: &'static str,
    kind: &'static str,
    bytes: &'static [u8],
    hash: String,
}

/// Every asset the shell can load, hashed once on first use (at startup, as
/// the shell is first drawn).
static ASSETS: LazyLock<Vec<Asset>> = LazyLock::new(|| {
    [
        (
            "tokens.css",
            "text/css; charset=utf-8",
            include_bytes!("static/tokens.css").as_slice(),
        ),
        (
            "app.css",
            "text/css; charset=utf-8",
            include_bytes!("static/app.css").as_slice(),
        ),
        (
            "htmx.min.js",
            "text/javascript; charset=utf-8",
            include_bytes!("static/htmx.min.js").as_slice(),
        ),
        (
            "sse.js",
            "text/javascript; charset=utf-8",
            include_bytes!("static/sse.js").as_slice(),
        ),
        (
            "theme.js",
            "text/javascript; charset=utf-8",
            include_bytes!("static/theme.js").as_slice(),
        ),
        (
            "tandem.js",
            "text/javascript; charset=utf-8",
            include_bytes!("static/tandem.js").as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, kind, bytes)| Asset {
        name,
        kind,
        bytes,
        hash: digest(bytes),
    })
    .collect()
});

/// The first 12 hex characters of an asset's SHA-256.
pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn find(name: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|asset| asset.name == name)
}

/// The URL the shell links for each asset, carrying its current hash.
pub(super) struct Urls {
    pub(super) tokens_css: String,
    pub(super) app_css: String,
    pub(super) htmx: String,
    pub(super) sse: String,
    pub(super) theme: String,
    pub(super) tandem: String,
}

pub(super) fn urls() -> Urls {
    let url = |name: &str| {
        let hash = &find(name).expect("the shell names a known asset").hash;
        format!("/static/{name}?v={hash}")
    };
    Urls {
        tokens_css: url("tokens.css"),
        app_css: url("app.css"),
        htmx: url("htmx.min.js"),
        sse: url("sse.js"),
        theme: url("theme.js"),
        tandem: url("tandem.js"),
    }
}

pub(super) async fn static_asset(
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    let Some(asset) = find(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // The hash is plain hex, so the raw `v=` value needs no decoding.
    let current = query.as_deref().is_some_and(|query| {
        query
            .split('&')
            .any(|pair| pair.strip_prefix("v=") == Some(asset.hash.as_str()))
    });
    let etag = format!("\"{}\"", asset.hash);
    let cache = if current { IMMUTABLE } else { REVALIDATE };
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|tag| tag.trim() == etag));
    let mut response = if matches {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (
            [(header::CONTENT_TYPE, HeaderValue::from_static(asset.kind))],
            asset.bytes,
        )
            .into_response()
    };
    let response_headers = response.headers_mut();
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).expect("a hex hash is a header value"),
    );
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
