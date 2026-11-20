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
//! - **Who the request is, decided once** (ADR 0052): the guard resolves every
//!   request into one [`Principal`] and leaves it in the request's extensions.
//!   `ResolveContext::resolve` only reads it. A personal token (`tdm_`) is
//!   admitted on `/mcp` and `/health` only; a sign-in key (`tdk_`) is accepted
//!   at `POST /login` and never as a bearer; `TANDEM_TOKEN` is the owner.
//! - **What each route needs, in one table** (CUT-39): [`ROUTE_NEEDS`] maps
//!   every route to the permission it asks, and [`permits`] decides. A route with
//!   no entry is refused, so a later route cannot opt out. A refused page
//!   answers a `403` page; a refused `/mcp` call answers a JSON-RPC error.
//! - **The route decides the shape.** `/mcp` takes JSON from no foreign origin;
//!   every other write takes a same-origin form.
//!
//! Nothing here logs a header value, and nothing answers a CORS header.

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::permission::{Permission, PrincipalKind, permits};
use crate::store::{
    Member, SESSION_IDLE_SECONDS, SessionCheck, SessionLookup, Store, credential_kind, unix_now,
};

use super::login::LoginThrottle;

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

/// The operator interface's prefix (CUT-40): the version is in the path.
pub(super) const OPERATOR_PREFIX: &str = "/operator/v1/";
pub(super) const OPERATOR_COUNT_ROUTE: &str = "/operator/v1/members/count";
pub(super) const OPERATOR_MEMBERS_ROUTE: &str = "/operator/v1/members";
pub(super) const OPERATOR_MEMBER_ROUTE: &str = "/operator/v1/members/{name}";

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
    /// The one configured public `https` origin (CUT-37), normalized, when the
    /// server sits behind a TLS proxy on a public name. `None` leaves every
    /// check exactly as it is without the setting.
    public_origin: Option<String>,
    /// The hosting provider's operator keys (CUT-40): the current one and the
    /// next, held as digests only. Empty leaves `/operator/v1` answering `404`.
    operator_keys: Vec<TokenDigest>,
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
            operator_keys: Vec::new(),
        }
    }

    /// A non-loopback bind with no authentication at all, for a network the
    /// human trusts (`--allow-unauthenticated`).
    pub fn open(port: u16) -> Self {
        Self {
            port,
            mode: Mode::Open,
            public_origin: None,
            operator_keys: Vec::new(),
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
            operator_keys: Vec::new(),
        }
    }

    /// Hold the operator key, and the next one beside it so a key rotates
    /// without downtime (CUT-40). Only digests are kept. The caller has checked
    /// the length and that neither is the owner token.
    pub fn with_operator_keys(mut self, current: &str, next: Option<&str>) -> Self {
        self.operator_keys = std::iter::once(current)
            .chain(next)
            .map(|key| TokenDigest(Sha256::digest(key.as_bytes()).into()))
            .collect();
        self
    }

    /// Whether an operator key is configured: without one the whole
    /// `/operator/v1` family answers `404`.
    fn has_operator_keys(&self) -> bool {
        !self.operator_keys.is_empty()
    }

    /// Whether `presented` is the operator key or the next one: fixed-length
    /// digests, every key compared with no early exit.
    fn operator_matches(&self, presented: Option<&str>) -> bool {
        let Some(presented) = presented else {
            return false;
        };
        let digest: [u8; 32] = Sha256::digest(presented.as_bytes()).into();
        let mut matched = false;
        for expected in &self.operator_keys {
            let mut difference = 0u8;
            for (left, right) in digest.iter().zip(expected.0.iter()) {
                difference |= left ^ right;
            }
            matched |= difference == 0;
        }
        matched
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

    /// How a request got in, as the export permission check names it.
    pub(super) fn export_access(&self) -> crate::archive::Access {
        match self.mode {
            Mode::Loopback => crate::archive::Access::Loopback,
            Mode::Open => crate::archive::Access::Unauthenticated,
            Mode::Token(_) => crate::archive::Access::Token,
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

    /// The bearer value the request presents, whatever it is.
    fn bearer(headers: &HeaderMap) -> Option<&str> {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
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

/// Who a request is. The one closed kind the guard resolves and everything
/// after reads (ADR 0052): no handler or tool looks at `Authorization` or the
/// cookie.
#[derive(Debug, Clone)]
pub(super) enum Principal {
    /// Whoever holds the server: loopback, `--allow-unauthenticated`, the
    /// `TANDEM_TOKEN` holder, and an owner browser session.
    Owner,
    /// A browser session that belongs to a member.
    MemberSession(Member),
    /// A personal token: acts as its member, on `/mcp` only.
    MemberToken(Member),
    /// The hosting provider's narrow key (CUT-40): it opens `/operator/v1`
    /// and nothing else, and never acts as a member.
    Operator,
}

impl Principal {
    /// The closed kind the one permission function reads (CUT-39).
    pub(super) fn kind(&self) -> PrincipalKind {
        match self {
            Self::Owner => PrincipalKind::Owner,
            Self::MemberSession(member) => PrincipalKind::MemberSession(member.role),
            Self::MemberToken(member) => PrincipalKind::MemberToken(member.role),
            Self::Operator => PrincipalKind::Operator,
        }
    }
}

/// What a route asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Need {
    /// Exempt by name from every credential: the liveness probe, the login
    /// pages and the static assets. They reach no workspace data.
    Public,
    /// The caller must hold this permission.
    Permit(Permission),
    /// The operator key only (CUT-40): the `/operator/v1` family. No other
    /// credential holds it, and the operator holds nothing else.
    Operator,
}

/// Every route and the permission it needs: the one map (CUT-39). The guard
/// refuses a route that is not here, and a test reads `routes()` against this
/// table, so adding a route without a line fails the build.
pub(super) const ROUTE_NEEDS: &[(&str, Need)] = &[
    ("/", Need::Permit(Permission::Board)),
    ("/board", Need::Permit(Permission::Board)),
    ("/events", Need::Permit(Permission::Board)),
    ("/export", Need::Permit(Permission::Export)),
    (HEALTH_ROUTE, Need::Public),
    (LOGIN_ROUTE, Need::Public),
    (LOGOUT_ROUTE, Need::Public),
    (MCP_ROUTE, Need::Permit(Permission::Board)),
    (STATIC_ROUTE, Need::Public),
    ("/tickets", Need::Permit(Permission::Board)),
    ("/tickets/{id}", Need::Permit(Permission::Board)),
    ("/tickets/{id}/status", Need::Permit(Permission::Board)),
    ("/tickets/{id}/comments", Need::Permit(Permission::Board)),
    ("/tickets/{id}/label", Need::Permit(Permission::Board)),
    ("/tokens", Need::Permit(Permission::ManageOwnTokens)),
    (
        "/tokens/{id}/revoke",
        Need::Permit(Permission::ManageOwnTokens),
    ),
    ("/people", Need::Permit(Permission::ManagePeople)),
    ("/people/{name}", Need::Permit(Permission::ManagePeople)),
    ("/people/{name}/key", Need::Permit(Permission::ManagePeople)),
    (
        "/people/{name}/role",
        Need::Permit(Permission::ManagePeople),
    ),
    (
        "/people/{name}/remove",
        Need::Permit(Permission::ManagePeople),
    ),
    (
        "/people/{name}/tokens/{id}/revoke",
        Need::Permit(Permission::ManagePeople),
    ),
    ("/labels", Need::Permit(Permission::Board)),
    ("/labels/{name}/rename", Need::Permit(Permission::Board)),
    ("/notes", Need::Permit(Permission::Board)),
    ("/notes/view", Need::Permit(Permission::Board)),
    ("/notes/edit", Need::Permit(Permission::Board)),
    ("/notes/open", Need::Permit(Permission::Board)),
    ("/notes/preview", Need::Permit(Permission::Board)),
    ("/notes/save", Need::Permit(Permission::Board)),
    ("/notes/create", Need::Permit(Permission::Board)),
    ("/notes/create-folder", Need::Permit(Permission::Board)),
    ("/notes/daily", Need::Permit(Permission::Board)),
    ("/notes/new", Need::Permit(Permission::Board)),
    ("/notes/move", Need::Permit(Permission::Board)),
    ("/notes/rename", Need::Permit(Permission::Board)),
    ("/notes/delete", Need::Permit(Permission::Board)),
    ("/notes/restore", Need::Permit(Permission::Board)),
    (OPERATOR_COUNT_ROUTE, Need::Operator),
    (OPERATOR_MEMBERS_ROUTE, Need::Operator),
    (OPERATOR_MEMBER_ROUTE, Need::Operator),
];

/// The need a route has, or `None` for a route the table does not name.
pub(super) fn route_need(route: &str) -> Option<Need> {
    // The routes the router tests add beside the real ones; the table above
    // is the only map a running serve has.
    #[cfg(test)]
    if matches!(route, "/unguarded" | "/context") {
        return Some(Need::Permit(Permission::Board));
    }
    ROUTE_NEEDS
        .iter()
        .find(|(path, _)| *path == route)
        .map(|(_, need)| *need)
}

/// Whether `principal` may reach `route`: `None` lets it through. An unmapped
/// route, a refused permission and an unknown principal all refuse (fail
/// closed), and the reason names the permission, never a value.
pub(super) fn authorize(principal: &Principal, route: &str, matched: bool) -> Option<Response> {
    match route_need(route) {
        // A path no route matched falls through to the router's own `404`;
        // a route that matched and has no line is the one refused.
        None if !matched => None,
        None => Some(refuse_access(route, "the route has no permission mapped")),
        Some(Need::Public) => None,
        Some(Need::Operator) => {
            if matches!(principal, Principal::Operator) {
                None
            } else {
                Some(refuse_access(route, "the route needs the operator key"))
            }
        }
        Some(Need::Permit(permission)) => {
            if permits(principal.kind(), permission) {
                None
            } else {
                Some(refuse_access(
                    route,
                    match permission {
                        Permission::Export => "the credential does not hold the export permission",
                        Permission::Board => "the credential does not hold the board permission",
                        _ => "the credential does not hold the permission the route needs",
                    },
                ))
            }
        }
    }
}

/// The body of a refused page: the `ux` wording, and a way back.
const ACCESS_REFUSED_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>No access</title></head><body><h1>You do not have access to this page</h1><p>Only admins can manage people. Ask an admin if you need a change.</p><p><a href=\"/\">Back to the board</a></p></body></html>\n";

/// The `401` body a tool gets for a bearer the server does not accept.
pub(super) const TOKEN_REFUSED: &str = "This token has been revoked or does not exist.";

/// What a refused `/mcp` call answers, as a JSON-RPC error.
pub(super) const ACCESS_REFUSED_MCP: &str =
    "A personal token cannot manage people or export the workspace.";

/// A permission refusal: a `403` page for a page request, the JSON-RPC error
/// for `/mcp`, and a log line with the reason and no value.
pub(super) fn refuse_access(route: &str, reason: &str) -> Response {
    tracing::warn!(reason, "request refused");
    if route == MCP_ROUTE {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": -32001, "message": ACCESS_REFUSED_MCP},
        });
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            body.to_string(),
        )
            .into_response();
    }
    (
        StatusCode::FORBIDDEN,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        ACCESS_REFUSED_PAGE,
    )
        .into_response()
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
async fn lookup(access: &Access, store: &Store, cookie: &str, touch: bool) -> SessionLookup {
    let missing = SessionLookup {
        check: SessionCheck::Missing,
        member: None,
    };
    let Some(id) = decode_session_id(cookie) else {
        return missing;
    };
    let Some(digest) = access.session_digest(&id) else {
        return missing;
    };
    let store = store.clone();
    match tokio::task::spawn_blocking(move || {
        store.browser_session_lookup(&digest, unix_now(), touch)
    })
    .await
    {
        Ok(Ok(found)) => found,
        Ok(Err(error)) => {
            tracing::warn!(error = ?error, "the session lookup failed; the request is signed out");
            missing
        }
        Err(_) => missing,
    }
}

/// Whether the session is live and, for a member's, still has the role it had
/// when the stream opened (`opened`): a removal, a sign-out, an expiry and a
/// change of role all end it. Read from the store each time, never remembered.
pub(super) async fn session_still(
    store: &Store,
    digest: &str,
    opened: Option<crate::store::Role>,
) -> bool {
    let store = store.clone();
    let digest = digest.to_string();
    match tokio::task::spawn_blocking(move || {
        store.browser_session_lookup(&digest, unix_now(), false)
    })
    .await
    {
        Ok(Ok(found)) => {
            matches!(found.check, SessionCheck::Valid | SessionCheck::Refreshed)
                && found.member.map(|member| member.role) == opened
        }
        _ => false,
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
pub(super) async fn access_guard(mut request: Request, next: Next) -> Response {
    let Some(access) = request.extensions().get::<Access>().cloned() else {
        // No access policy was installed: refuse rather than serve unguarded.
        tracing::error!("request refused: the router carries no access policy");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let matched = request.extensions().get::<MatchedPath>().is_some();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let method = request.method().clone();

    // The operator key (CUT-40) is a credential of its own: it is looked for
    // first, opens `/operator/v1` and nothing else (`admit` refuses it
    // elsewhere), and the family opens to no other credential.
    let family = route.starts_with(OPERATOR_PREFIX);
    if family && !access.has_operator_keys() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if access.operator_matches(Access::bearer(request.headers())) {
        let principal = Principal::Operator;
        if family && let Some(denied) = authorize(&principal, &route, matched) {
            return denied;
        }
        request.extensions_mut().insert(principal);
        return finish(access, route, method, request, next, false, None).await;
    }
    if family {
        return refuse_json(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "the operator key is missing or refused",
        );
    }

    // Who is this request, in token mode: the owner's bearer, a personal token,
    // a live session cookie, or a route that is exempt by name.
    let mut cookie_authenticated = false;
    let mut refreshed: Option<String> = None;
    let mut principal = Principal::Owner;
    if access.is_token() && route != HEALTH_ROUTE {
        let bearer = access.authorized(request.headers());
        if !bearer
            && let Some(presented) = Access::bearer(request.headers())
            && credential_kind(presented).is_some()
        {
            let store = request.extensions().get::<Store>().cloned();
            let throttle = request.extensions().get::<LoginThrottle>().cloned();
            match member_token(store, throttle, presented.to_string()).await {
                TokenCheck::Member(member) => {
                    // The credential kind comes first: a token is admitted on
                    // /mcp only (/health is public and never reaches here), so
                    // it holds `board` over /mcp and nothing on a page.
                    let principal = Principal::MemberToken(member);
                    if route != MCP_ROUTE {
                        return refuse_access(
                            &route,
                            "a personal token reaches /mcp and /health only",
                        );
                    }
                    if let Some(denied) = authorize(&principal, &route, matched) {
                        return denied;
                    }
                    request.extensions_mut().insert(principal);
                    return finish(access, route, method, request, next, false, None).await;
                }
                TokenCheck::TooMany => {
                    return refuse(
                        StatusCode::TOO_MANY_REQUESTS,
                        "too many attempts",
                        "too many wrong member credentials waiting",
                    );
                }
                // Unknown, revoked, removed or unreadable: one path, the same
                // answer as no credential at all.
                TokenCheck::Refused => {}
            }
        }
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
            let found = lookup(&access, &store, &cookie, true).await;
            if found.check != SessionCheck::Missing
                && let Some(id) = decode_session_id(&cookie)
                && let Some(digest) = access.session_digest(&id)
            {
                session = Some(Session { digest });
                if !bearer && let Some(member) = found.member {
                    principal = Principal::MemberSession(member);
                }
                if found.check == SessionCheck::Refreshed {
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
    if let Some(denied) = authorize(&principal, &route, matched) {
        return denied;
    }
    request.extensions_mut().insert(principal);
    finish(
        access,
        route,
        method,
        request,
        next,
        cookie_authenticated,
        refreshed,
    )
    .await
}

/// The outcome of presenting a member-shaped bearer.
enum TokenCheck {
    Member(Member),
    Refused,
    TooMany,
}

/// Look a well-formed personal token up. A wrong one waits on the same
/// throttle as a wrong login and is then refused like any other credential; a
/// right one is never delayed. A store that cannot be read refuses (fail closed).
async fn member_token(
    store: Option<Store>,
    throttle: Option<LoginThrottle>,
    value: String,
) -> TokenCheck {
    let Some(store) = store else {
        return TokenCheck::Refused;
    };
    let found =
        tokio::task::spawn_blocking(move || store.member_by_token(&value, unix_now(), true)).await;
    match found {
        Ok(Ok(Some(member))) => TokenCheck::Member(member),
        Ok(Ok(None)) => {
            let Some(throttle) = throttle else {
                return TokenCheck::Refused;
            };
            if throttle.wrong_attempt().await {
                tracing::warn!("a member credential was not accepted");
                TokenCheck::Refused
            } else {
                TokenCheck::TooMany
            }
        }
        Ok(Err(error)) => {
            tracing::warn!(error = ?error, "the member lookup failed; the request is refused");
            TokenCheck::Refused
        }
        Err(_) => TokenCheck::Refused,
    }
}

/// The bind's host check and the route's shape, then the handler, then the
/// response's own headers.
async fn finish(
    access: Access,
    route: String,
    method: Method,
    request: Request,
    next: Next,
    cookie_authenticated: bool,
    refreshed: Option<String>,
) -> Response {
    let require_origin = cookie_authenticated || (access.is_token() && route == LOGIN_ROUTE);
    let operator = matches!(
        request.extensions().get::<Principal>(),
        Some(Principal::Operator)
    );
    if let Some(denied) = admit(
        &access,
        &route,
        &method,
        request.headers(),
        require_origin,
        operator,
    ) {
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
        // A tool that presented a personal token on /mcp is told what to do:
        // one body for a revoked, a removed, an unknown and a malformed one, so
        // the answer says nothing about which it was. The prefix is public, so
        // reading it tells nothing; every other refusal keeps the plain body.
        let body = if route == MCP_ROUTE
            && Access::bearer(request.headers())
                .is_some_and(|presented| presented.starts_with(crate::store::TOKEN_PREFIX))
        {
            TOKEN_REFUSED
        } else {
            "unauthorized"
        };
        return refuse(
            StatusCode::UNAUTHORIZED,
            body,
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
    operator: bool,
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
    // Default deny: the operator key admits the family's names and no other
    // route, so a later route cannot inherit it.
    if operator && !route.starts_with(OPERATOR_PREFIX) {
        return Some(refuse_json(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "the operator key admits only /operator/v1",
        ));
    }
    if route.starts_with(OPERATOR_PREFIX) {
        return operator_policy(method, headers);
    }
    if route == HEALTH_ROUTE {
        return None;
    }
    if route == MCP_ROUTE {
        json_policy(access, method, headers)
    } else {
        form_policy(access, method, headers, require_origin)
    }
}

/// `/operator/v1` (CUT-40): not a browser door. An `Origin` header of any
/// value is refused, a body is JSON, and no CORS header is ever answered.
fn operator_policy(method: &Method, headers: &HeaderMap) -> Option<Response> {
    if headers.contains_key(header::ORIGIN) {
        return Some(refuse_json(
            StatusCode::FORBIDDEN,
            "the operator interface is not a browser door",
            "an operator request carried an Origin header",
        ));
    }
    if matches!(*method, Method::POST | Method::PATCH)
        && !media_type_is(headers, "application/json")
    {
        return Some(refuse_json(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "the request must be application/json",
            "the content type is not json",
        ));
    }
    None
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
fn form_policy(
    access: &Access,
    method: &Method,
    headers: &HeaderMap,
    require_origin: bool,
) -> Option<Response> {
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
    let origin = access.origin_check(headers, OriginRoute::Form);
    if origin == OriginCheck::Mismatch || (require_origin && origin == OriginCheck::Absent) {
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

/// One refusal as JSON, for the operator interface: the status, a short reason
/// the client may read, and a log line that names no value.
pub(super) fn refuse_json(status: StatusCode, body: &str, reason: &str) -> Response {
    tracing::warn!(reason, "request refused");
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::json!({ "error": body }).to_string(),
    )
        .into_response()
}

/// One refusal: the status the client gets and a reason that names no value.
pub(super) fn refuse(status: StatusCode, body: &str, reason: &str) -> Response {
    tracing::warn!(reason, "request refused");
    (status, format!("{body}\n")).into_response()
}
