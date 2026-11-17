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
//! - **A token-mode browser signs in once** (ADR 0049): `/login` turns the token
//!   into a session cookie, and the guard accepts that cookie where it accepts
//!   the bearer header, except on `/mcp`. The login routes and `GET /static/*`
//!   are exempt from the token by name, like `/health`.
//! - **The route decides the shape.** `/mcp` takes JSON from no foreign origin;
//!   every other write takes a same-origin form.
//!
//! Nothing here logs a header value, and nothing answers a CORS header.

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::store::{SESSION_IDLE_SECONDS, SessionCheck, Store, unix_now};

/// The route that carries the MCP transport.
pub(super) const MCP_ROUTE: &str = "/mcp";

/// The route the liveness probe reads: exempt from the token, by name.
pub(super) const HEALTH_ROUTE: &str = "/health";

/// The login page and its form post: exempt from the token, by name.
pub(super) const LOGIN_ROUTE: &str = "/login";

/// The sign-out form post: behind the guard like any write.
pub(super) const LOGOUT_ROUTE: &str = "/logout";

/// The static assets: the login page needs its CSS, and the assets hold no
/// workspace data. Exempt for `GET` only.
const STATIC_ROUTE: &str = "/static/{*path}";

/// The change stream: a refused one answers 401, never a redirect.
const EVENTS_ROUTE: &str = "/events";

/// The session cookie. The `__Host-` prefix is the browser's own promise that
/// it is `Secure`, has `Path=/` and names no `Domain`.
pub(super) const SESSION_COOKIE: &str = "__Host-tandem_session";

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

    /// Whether this run signs browsers in with a token.
    pub(super) fn is_token(&self) -> bool {
        matches!(self.mode, Mode::Token(_))
    }

    /// Whether `presented` is the token, on the same constant-time digest path
    /// as the bearer header.
    pub(super) fn token_matches(&self, presented: &str) -> bool {
        self.digest_matches(Some(presented))
    }

    /// The fingerprint a session id is stored under: SHA-256 over the in-memory
    /// token digest and the id, hex. A changed token matches no stored row, and
    /// nothing derived from the token alone reaches disk. `None` off token mode.
    pub(super) fn session_digest(&self, id: &[u8]) -> Option<String> {
        let Mode::Token(key) = &self.mode else {
            return None;
        };
        let mut hasher = Sha256::new();
        hasher.update(key.0);
        hasher.update(b"tandem browser session");
        hasher.update(id);
        Some(hex(&hasher.finalize()))
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
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        self.digest_matches(presented)
    }

    fn digest_matches(&self, presented: Option<&str>) -> bool {
        let Mode::Token(expected) = &self.mode else {
            return true;
        };
        let digest: [u8; 32] = Sha256::digest(presented.unwrap_or("").as_bytes()).into();
        let mut difference = 0u8;
        for (left, right) in digest.iter().zip(expected.0.iter()) {
            difference |= left ^ right;
        }
        (difference == 0) & presented.is_some()
    }
}

/// Lowercase hex.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A session id from its cookie text: exactly 64 hex characters, else `None`.
fn decode_session_id(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut id = [0u8; 32];
    for (index, slot) in id.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(id)
}

/// The session cookie's value in a `Cookie` header, when it is there.
fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_string())
}

/// The `Set-Cookie` line that sets (or, with an empty value and `0`, clears)
/// the session cookie.
pub(super) fn session_cookie(value: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Max-Age={max_age}; Path=/; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("a hex value is a header value")
}

/// A browser session the guard found for this request, left in the request's
/// extensions for the handlers that read it (login, logout, the change stream).
#[derive(Debug, Clone)]
pub(super) struct Session {
    pub(super) digest: String,
}

/// A same-origin path to return to after sign-in, else `/`: one leading `/`, a
/// second character that is neither `/` nor a backslash, no control character.
pub(super) fn return_target(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let safe = bytes.first() == Some(&b'/')
        && !matches!(bytes.get(1), Some(b'/') | Some(b'\\'))
        && !raw.chars().any(char::is_control)
        && raw.is_ascii()
        && raw != LOGIN_ROUTE
        && !raw.starts_with("/login?")
        && !raw.starts_with(LOGOUT_ROUTE);
    if safe {
        raw.to_string()
    } else {
        "/".to_string()
    }
}

/// Look a cookie's session up: fail closed, so a locked store, a missing table
/// or a join error is a signed-out request.
async fn lookup(access: &Access, store: &Store, cookie: &str, touch: bool) -> SessionCheck {
    let Some(id) = decode_session_id(cookie) else {
        return SessionCheck::Missing;
    };
    let Some(digest) = access.session_digest(&id) else {
        return SessionCheck::Missing;
    };
    let store = store.clone();
    match tokio::task::spawn_blocking(move || {
        store.browser_session_check(&digest, unix_now(), touch)
    })
    .await
    {
        Ok(Ok(check)) => check,
        Ok(Err(error)) => {
            tracing::warn!(error = ?error, "the session lookup failed; the request is signed out");
            SessionCheck::Missing
        }
        Err(_) => SessionCheck::Missing,
    }
}

/// Whether the session behind this digest is still live, without touching it.
pub(super) async fn session_live(store: &Store, digest: &str) -> bool {
    let store = store.clone();
    let digest = digest.to_string();
    matches!(
        tokio::task::spawn_blocking(move || store.browser_session_check(
            &digest,
            unix_now(),
            false
        ))
        .await,
        Ok(Ok(SessionCheck::Valid | SessionCheck::Refreshed))
    )
}

/// The guard as a layer: every request crosses it before a handler runs.
pub(super) async fn access_guard(mut request: Request, next: Next) -> Response {
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
    let method = request.method().clone();

    // Who is this request, in token mode: a bearer, a live session cookie, or a
    // route that is exempt by name.
    let mut cookie_authenticated = false;
    let mut refreshed: Option<String> = None;
    if access.is_token() && route != HEALTH_ROUTE {
        let bearer = access.authorized(request.headers());
        let login_route = route == LOGIN_ROUTE || route == LOGOUT_ROUTE;
        let static_asset = route == STATIC_ROUTE && method == Method::GET;
        let cookie = if route == MCP_ROUTE || static_asset {
            None
        } else {
            session_cookie_value(request.headers())
        };
        let mut session = None;
        if let Some(cookie) = cookie.filter(|_| !bearer || login_route)
            && let Some(store) = request.extensions().get::<Store>().cloned()
        {
            let check = lookup(&access, &store, &cookie, true).await;
            if check != SessionCheck::Missing
                && let Some(id) = decode_session_id(&cookie)
                && let Some(digest) = access.session_digest(&id)
            {
                session = Some(Session { digest });
                if check == SessionCheck::Refreshed {
                    refreshed = Some(cookie);
                }
            }
        }
        cookie_authenticated = !bearer && session.is_some();
        if let Some(session) = session {
            request.extensions_mut().insert(session);
        }
        let exempt = (route == LOGIN_ROUTE) || static_asset;
        if !bearer && !cookie_authenticated && !exempt {
            return signed_out(&route, &method, &request);
        }
    }

    let require_origin = cookie_authenticated || (access.is_token() && route == LOGIN_ROUTE);
    if let Some(denied) = admit(&access, &route, &method, request.headers(), require_origin) {
        return denied;
    }
    let mut response = next.run(request).await;
    if cookie_authenticated && route != STATIC_ROUTE {
        // A page drawn for a signed-in browser is not kept: Back after sign-out
        // asks the server again.
        response
            .headers_mut()
            .entry(header::CACHE_CONTROL)
            .or_insert(HeaderValue::from_static("no-store"));
    }
    if let Some(cookie) = refreshed {
        response.headers_mut().append(
            header::SET_COOKIE,
            session_cookie(&cookie, SESSION_IDLE_SECONDS),
        );
    }
    response
}

/// A request with no credential: a page load goes to the login page carrying
/// where it was going, and every other request is a bare `401`.
fn signed_out(route: &str, method: &Method, request: &Request) -> Response {
    let page = *method == Method::GET
        && route != EVENTS_ROUTE
        && route != MCP_ROUTE
        && request.headers().get("hx-request").is_none();
    if !page {
        return refuse(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "the token is missing or refused",
        );
    }
    let target = request
        .uri()
        .path_and_query()
        .map(|path| return_target(path.as_str()))
        .unwrap_or_else(|| "/".to_string());
    let location = if target == "/" {
        LOGIN_ROUTE.to_string()
    } else {
        let query = serde_urlencoded::to_string([("next", target.as_str())]).unwrap_or_default();
        format!("{LOGIN_ROUTE}?{query}")
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The bind's host check and the route's shape, in that order: `None` lets the
/// request through. Who the request is was decided by [`access_guard`].
fn admit(
    access: &Access,
    route: &str,
    method: &Method,
    headers: &HeaderMap,
    require_origin: bool,
) -> Option<Response> {
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
    }
    if route == HEALTH_ROUTE {
        return None;
    }
    if route == MCP_ROUTE {
        json_policy(access, method, headers)
    } else {
        form_policy(method, headers, require_origin)
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
fn form_policy(method: &Method, headers: &HeaderMap, require_origin: bool) -> Option<Response> {
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
    if cross_origin(headers, require_origin) {
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
/// sent. A write a cookie authenticates, and the login post, must send one: the
/// cookie rides every request a browser makes, so the header is the proof. The
/// Host itself was admitted above on a loopback bind.
fn cross_origin(headers: &HeaderMap, require_origin: bool) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return require_origin;
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
