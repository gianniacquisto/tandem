//! The one guard layer (ADR 0047): who may reach a route, and in what shape.
//!
//! Every route crosses [`access_guard`] by construction (`layered`), and the
//! route chooses the policy inside it, so a route cannot opt out and a later
//! route cannot copy a carve-out:
//!
//! - **The bind decides who is let in.** A loopback bind admits only a request
//!   whose `Host` is one of the three loopback authorities at the port actually
//!   bound (the DNS-rebinding stop, which `Origin` alone cannot be: an
//!   attacker's page chooses both). A non-loopback bind admits every request
//!   when the human passed `--allow-unauthenticated`, and only a request
//!   carrying the bearer token when `TANDEM_TOKEN` is set, `/health` excepted.
//! - **The route decides the shape.** `/mcp` takes JSON from no foreign origin;
//!   every other write takes a same-origin form.
//!
//! Nothing here logs a header value, and nothing answers a CORS header.

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

/// The route that carries the MCP transport.
pub(super) const MCP_ROUTE: &str = "/mcp";

/// The route the liveness probe reads: exempt from the token, by name.
pub(super) const HEALTH_ROUTE: &str = "/health";

/// The three names a loopback authority takes.
const LOOPBACK_NAMES: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// The SHA-256 of the bearer token. Only the digest is kept, so a comparison
/// is over fixed-length values and the token itself is never held or printed.
#[derive(Clone)]
struct TokenDigest([u8; 32]);

impl std::fmt::Debug for TokenDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TokenDigest(<redacted>)")
    }
}

/// How one serve run lets a request in: the port it bound, and the token when
/// the bind is not loopback.
#[derive(Debug, Clone)]
pub struct Access {
    port: u16,
    mode: Mode,
}

/// How the bind lets a request in.
#[derive(Debug, Clone)]
enum Mode {
    /// The Host check: only a loopback authority at the bound port.
    Loopback,
    /// A non-loopback bind the human opened with `--allow-unauthenticated`:
    /// no Host check and no token. The network is the boundary (a tailnet, a
    /// private LAN); real authentication is a later decision (ADR 0047).
    Open,
    /// A non-loopback bind with a token: every route but `/health` wants it.
    Token(TokenDigest),
}

impl Access {
    /// A loopback bind at `port`: the Host check, no token.
    pub fn loopback(port: u16) -> Self {
        Self {
            port,
            mode: Mode::Loopback,
        }
    }

    /// A non-loopback bind with no authentication at all, for a network the
    /// human trusts (`--allow-unauthenticated`).
    pub fn open(port: u16) -> Self {
        Self {
            port,
            mode: Mode::Open,
        }
    }

    /// A non-loopback bind at `port`: the bearer token gates every route but
    /// `/health`, and the Host check is the fronting proxy's to make.
    pub fn token(port: u16, secret: &str) -> Self {
        let digest: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        Self {
            port,
            mode: Mode::Token(TokenDigest(digest)),
        }
    }

    fn authority(&self, name: &str) -> String {
        format!("{name}:{}", self.port)
    }

    fn host_allowed(&self, host: &str) -> bool {
        LOOPBACK_NAMES
            .iter()
            .any(|name| host.eq_ignore_ascii_case(&self.authority(name)))
    }

    /// An `Origin` is allowed when it is exactly `http://` and a loopback
    /// authority. `null`, any other scheme and any other authority are not.
    fn origin_allowed(&self, origin: &str) -> bool {
        origin
            .strip_prefix("http://")
            .is_some_and(|authority| self.host_allowed(authority))
    }

    /// Whether the request carries the token: `Authorization: Bearer`, compared
    /// as fixed-length digests without an early exit. A missing, malformed or
    /// wrong header takes the same path and answers the same.
    fn authorized(&self, headers: &HeaderMap) -> bool {
        let Mode::Token(expected) = &self.mode else {
            return true;
        };
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        let digest: [u8; 32] = Sha256::digest(presented.unwrap_or("").as_bytes()).into();
        let mut difference = 0u8;
        for (left, right) in digest.iter().zip(expected.0.iter()) {
            difference |= left ^ right;
        }
        (difference == 0) & presented.is_some()
    }
}

/// The guard as a layer: every request crosses it before a handler runs.
pub(super) async fn access_guard(request: Request, next: Next) -> Response {
    let Some(access) = request.extensions().get::<Access>().cloned() else {
        // No access policy was installed: refuse rather than serve unguarded.
        tracing::error!("request refused: the router carries no access policy");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    if let Some(denied) = admit(&access, &route, request.method(), request.headers()) {
        return denied;
    }
    next.run(request).await
}

/// Admission and shape, in that order: `None` lets the request through.
fn admit(access: &Access, route: &str, method: &Method, headers: &HeaderMap) -> Option<Response> {
    if matches!(access.mode, Mode::Loopback) {
        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !access.host_allowed(host) {
            return Some(refuse(
                StatusCode::FORBIDDEN,
                "the request names a host this server does not answer for",
                "the host is not a loopback authority",
            ));
        }
    } else if matches!(access.mode, Mode::Token(_))
        && route != HEALTH_ROUTE
        && !access.authorized(headers)
    {
        return Some(refuse(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "the token is missing or refused",
        ));
    }
    if route == HEALTH_ROUTE {
        return None;
    }
    if route == MCP_ROUTE {
        json_policy(access, method, headers)
    } else {
        form_policy(method, headers)
    }
}

/// `/mcp`: a loopback or absent `Origin` and, for the one method it answers, a
/// JSON body. Any other method falls through to the router's own `405`.
fn json_policy(access: &Access, method: &Method, headers: &HeaderMap) -> Option<Response> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let allowed = origin
            .to_str()
            .ok()
            .is_some_and(|origin| access.origin_allowed(origin));
        if !allowed {
            return Some(refuse(
                StatusCode::FORBIDDEN,
                "the request came from another origin",
                "the request came from another origin",
            ));
        }
    }
    if *method == Method::POST && !media_type_is(headers, "application/json") {
        return Some(refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "the request must be application/json",
            "the content type is not json",
        ));
    }
    None
}

/// Every other route: reads pass; a write is a same-origin form.
fn form_policy(method: &Method, headers: &HeaderMap) -> Option<Response> {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return None;
    }
    if !media_type_is(headers, "application/x-www-form-urlencoded") {
        return Some(refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "the request must be application/x-www-form-urlencoded",
            "the content type is not form-encoded",
        ));
    }
    if cross_origin(headers) {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            "the request came from another origin",
            "the request came from another origin",
        ));
    }
    None
}

/// Whether the request's `Content-Type` is exactly this media type, parameters
/// allowed. A prefix is not a match: `application/jsonx` is another type.
fn media_type_is(headers: &HeaderMap, wanted: &str) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case(wanted))
}

/// A form write's `Origin` must match the `Host` the request names, when one is
/// sent. The Host itself was admitted above on a loopback bind.
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

/// One refusal: the status the client gets and a reason that names no value.
fn refuse(status: StatusCode, body: &str, reason: &str) -> Response {
    tracing::warn!(reason, "request refused");
    (status, format!("{body}\n")).into_response()
}
