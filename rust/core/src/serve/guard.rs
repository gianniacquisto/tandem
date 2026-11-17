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
    /// The one configured public `https` origin (CUT-37), normalized, when the
    /// server sits behind a TLS proxy on a public name. `None` leaves every
    /// check exactly as it is without the setting.
    public_origin: Option<String>,
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
            public_origin: None,
        }
    }

    /// A non-loopback bind with no authentication at all, for a network the
    /// human trusts (`--allow-unauthenticated`).
    pub fn open(port: u16) -> Self {
        Self {
            port,
            mode: Mode::Open,
            public_origin: None,
        }
    }

    /// A non-loopback bind at `port`: the bearer token gates every route but
    /// `/health`, and the Host check is the fronting proxy's to make.
    pub fn token(port: u16, secret: &str) -> Self {
        let digest: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        Self {
            port,
            mode: Mode::Token(TokenDigest(digest)),
            public_origin: None,
        }
    }

    /// Name the one public origin this server answers for behind a TLS proxy.
    /// The value is normalized by [`normalize_public_origin`]; a refused value
    /// is the reason it was refused, never the value. Only a non-loopback
    /// bind takes it: the caller refuses the loopback case before this.
    pub fn with_public_origin(mut self, origin: &str) -> Result<Self, &'static str> {
        self.public_origin = Some(normalize_public_origin(origin)?);
        Ok(self)
    }

    /// Whether `Origin` on this request is our own: the one rule both `/mcp`
    /// and the form writes (and, later, the cookie write of CUT-34) route
    /// through, so the loopback names and the origin parse live once.
    ///
    /// With no public origin configured it is today's rule, byte for byte:
    /// `/mcp` wants a loopback `http://` authority, a form write wants the
    /// `Origin` authority to equal the request's `Host`. With one configured it
    /// is exact string equality of the whole header against the stored
    /// normalized origin (plus the loopback names on `/mcp`); the request
    /// header is never normalized and more than one `Origin` is a mismatch.
    /// `Host` and forwarded headers are never read on the configured path, and
    /// no absolute URL may be built from `Host`: a redirect is relative.
    ///
    /// The caller decides what [`OriginCheck::Absent`] means.
    pub(super) fn origin_check(&self, headers: &HeaderMap, route: OriginRoute) -> OriginCheck {
        let mut values = headers.get_all(header::ORIGIN).iter();
        let Some(first) = values.next() else {
            return OriginCheck::Absent;
        };
        let matched = match &self.public_origin {
            Some(configured) => {
                values.next().is_none()
                    && first.to_str().ok().is_some_and(|origin| {
                        origin == configured
                            || (route == OriginRoute::Mcp && self.origin_allowed(origin))
                    })
            }
            None => match (route, first.to_str()) {
                (OriginRoute::Mcp, Ok(origin)) => self.origin_allowed(origin),
                (OriginRoute::Mcp, Err(_)) => false,
                // An unreadable `Origin` on a form write passes today.
                (OriginRoute::Form, Err(_)) => return OriginCheck::Absent,
                (OriginRoute::Form, Ok(origin)) => !cross_origin(origin, headers),
            },
        };
        if matched {
            OriginCheck::Matches
        } else {
            OriginCheck::Mismatch
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

/// What an `Origin` header says about a request: none, ours, or not ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OriginCheck {
    Absent,
    Matches,
    Mismatch,
}

/// Which route's `Origin` rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OriginRoute {
    Mcp,
    Form,
}

/// The one strict normalizer for the configured public origin, run once at
/// startup. Accepts `https://host[:port]` only; lowercases the host and drops
/// `:443`, to match how a browser serializes an `Origin`. The error is the rule
/// broken, never the value (it could carry userinfo pasted by mistake).
pub fn normalize_public_origin(value: &str) -> Result<String, &'static str> {
    if value.is_empty() {
        return Err("is empty.");
    }
    if !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(
            "must be plain ASCII with no spaces (write an international name as punycode).",
        );
    }
    if value.contains('\\') || value.contains('%') {
        return Err("must not contain a backslash or a percent escape.");
    }
    let lowered = value.to_ascii_lowercase();
    let Some(rest) = lowered.strip_prefix("https://") else {
        return Err(if lowered.starts_with("http://") {
            "must be https: a token over cleartext http is not a supported shape."
        } else {
            "must start with https://."
        });
    };
    if rest.contains('@') {
        return Err("must not carry userinfo.");
    }
    if rest.ends_with('/') && !rest[..rest.len() - 1].contains(['/', '?', '#']) {
        return Err(
            "must not end with a slash: write https://workspace.example, not https://workspace.example/.",
        );
    }
    if rest.contains(['/', '?', '#']) {
        return Err("must not carry a path, query or fragment.");
    }
    let (host, port) = if let Some(bracketed) = rest.strip_prefix('[') {
        let Some((inside, after)) = bracketed.split_once(']') else {
            return Err("has an unclosed IPv6 bracket.");
        };
        let Ok(address) = inside.parse::<std::net::Ipv6Addr>() else {
            return Err("has a host that is not a valid IPv6 literal.");
        };
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port),
            None if after.is_empty() => None,
            None => return Err("has text after the IPv6 literal."),
        };
        (format!("[{address}]"), port)
    } else {
        let (host, port) = match rest.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        let valid = !host.is_empty()
            && !host.starts_with('.')
            && !host.ends_with('.')
            && !host.contains("..")
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
        if !valid {
            return Err(
                "has a host that is empty, ends in a dot or has a character a host cannot.",
            );
        }
        (host.to_string(), port)
    };
    if LOOPBACK_NAMES.contains(&host.as_str()) {
        return Err("names a loopback host: it is for a public name behind a proxy.");
    }
    let port = match port {
        None => None,
        Some(digits) => {
            let well_formed = !digits.is_empty()
                && !digits.starts_with('0')
                && digits.bytes().all(|byte| byte.is_ascii_digit());
            match digits.parse::<u16>() {
                Ok(number) if well_formed => Some(number),
                _ => return Err("has a port that is not 1 to 65535 without leading zeros."),
            }
        }
    };
    Ok(match port {
        None | Some(443) => format!("https://{host}"),
        Some(port) => format!("https://{host}:{port}"),
    })
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
        form_policy(access, method, headers)
    }
}

/// `/mcp`: a loopback or absent `Origin` and, for the one method it answers, a
/// JSON body. Any other method falls through to the router's own `405`.
fn json_policy(access: &Access, method: &Method, headers: &HeaderMap) -> Option<Response> {
    if access.origin_check(headers, OriginRoute::Mcp) == OriginCheck::Mismatch {
        return Some(refuse_origin(
            access,
            "the /mcp origin rule refused the request",
        ));
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
fn form_policy(access: &Access, method: &Method, headers: &HeaderMap) -> Option<Response> {
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
    if access.origin_check(headers, OriginRoute::Form) == OriginCheck::Mismatch {
        return Some(refuse_origin(
            access,
            "the form origin rule refused the request",
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

/// Today's form rule, with no public origin configured: the `Origin` must
/// match the `Host` the request names. The Host itself was admitted above on a
/// loopback bind. Returns whether the origin is another one.
fn cross_origin(origin: &str, headers: &HeaderMap) -> bool {
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

/// The origin refusal: one body, a reason naming which check refused, and
/// whether a public origin is configured, never a value.
fn refuse_origin(access: &Access, reason: &str) -> Response {
    tracing::warn!(
        reason,
        public_origin_configured = access.public_origin.is_some(),
        "request refused: the request came from another origin"
    );
    (
        StatusCode::FORBIDDEN,
        "the request came from another origin\n",
    )
        .into_response()
}

/// One refusal: the status the client gets and a reason that names no value.
fn refuse(status: StatusCode, body: &str, reason: &str) -> Response {
    tracing::warn!(reason, "request refused");
    (status, format!("{body}\n")).into_response()
}
