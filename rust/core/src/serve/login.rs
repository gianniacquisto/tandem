//! The browser sign-in (ADR 0049): `/login`, `/logout`, and the throttle on a
//! wrong token. All of it exists in token mode only; every other mode answers
//! `404`, because there is nothing to sign in to.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use super::guard::{self, Session};
use super::*;
use crate::store::{
    CredentialKind, Member, SESSION_IDLE_SECONDS, Store, credential_kind, unix_now,
};

/// How long a wrong token waits before it is answered.
#[cfg(not(test))]
const WRONG_TOKEN_DELAY: Duration = Duration::from_secs(1);
#[cfg(test)]
const WRONG_TOKEN_DELAY: Duration = Duration::from_millis(600);

/// How many wrong tokens may wait at once; the excess is turned away with `429`.
/// Global, not per address: the peer is the proxy and `X-Forwarded-For` is
/// untrusted until the public origin setting (CUT-37) says otherwise.
const WRONG_TOKENS_AT_ONCE: usize = 4;

const WRONG_TOKEN: &str = "That token was not accepted.";
const TRY_AGAIN: &str = "Too many attempts at once. Try again in a moment.";
const SIGN_IN_FAILED: &str = "Signing in failed on the server. Try again in a moment.";
const SIGNED_OUT: &str = "You are signed out.";
const REMOVED: &str = "You were removed from this workspace.";
const ROLE_CHANGED: &str = "Your role changed. Sign in again to continue.";

/// The shared wait list for wrong tokens.
#[derive(Debug, Clone)]
pub(super) struct LoginThrottle(Arc<Semaphore>);

impl LoginThrottle {
    pub(super) fn new() -> Self {
        Self(Arc::new(Semaphore::new(WRONG_TOKENS_AT_ONCE)))
    }

    /// A wrong credential waits its turn: `true` once it has waited, `false`
    /// when too many already wait and it is turned away at once. A right one
    /// never comes here, so it is never delayed.
    pub(super) async fn wrong_attempt(&self) -> bool {
        let Ok(permit) = self.0.clone().try_acquire_owned() else {
            return false;
        };
        tokio::time::sleep(WRONG_TOKEN_DELAY).await;
        drop(permit);
        true
    }
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginView {
    assets: assets::Urls,
    next: String,
    error: Option<&'static str>,
    notice: Option<&'static str>,
}

fn login_page(
    next: &str,
    error: Option<&'static str>,
    notice: Option<&'static str>,
    status: StatusCode,
) -> Response {
    let view = LoginView {
        assets: assets::urls(),
        next: guard::return_target(next),
        error,
        notice,
    };
    let mut response = match view.render() {
        Ok(html) => html_response(html, status),
        Err(error) => {
            tracing::error!(error = ?error, "the login page could not be drawn");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let value = HeaderValue::from_str(location).unwrap_or_else(|_| HeaderValue::from_static("/"));
    response.headers_mut().insert(header::LOCATION, value);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) async fn login_page_get(
    Extension(access): Extension<Access>,
    session: Option<Extension<Session>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !access.is_token() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let next = field(&params, "next");
    if session.is_some() {
        return see_other(&guard::return_target(next));
    }
    let notice = match field(&params, "notice") {
        "signed-out" => Some(SIGNED_OUT),
        "removed" => Some(REMOVED),
        "role-changed" => Some(ROLE_CHANGED),
        _ => None,
    };
    login_page(next, None, notice, StatusCode::OK)
}

pub(super) async fn login_post(
    context: Context,
    Extension(access): Extension<Access>,
    Extension(throttle): Extension<LoginThrottle>,
    session: Option<Extension<Session>>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if !access.is_token() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(params) = form_params(form) else {
        return login_page("/", Some(WRONG_TOKEN), None, StatusCode::BAD_REQUEST);
    };
    let next = field(&params, "next").to_string();
    // The credential is trimmed: a paste from a phone carries a trailing space.
    let presented = field(&params, "password").trim();
    // The owner token, or a member's sign-in key. Never a personal token: it is
    // a different kind and has no way into a browser session.
    let member = if access.token_matches(presented) {
        None
    } else {
        match sign_in_key_member(&context.store, presented).await {
            Some(member) => Some(member),
            None => {
                // A wrong credential waits, a few at a time; a right one never does.
                if !throttle.wrong_attempt().await {
                    tracing::warn!("login refused: too many wrong credentials waiting");
                    return login_page(&next, Some(TRY_AGAIN), None, StatusCode::TOO_MANY_REQUESTS);
                }
                tracing::warn!("login refused: the credential was not accepted");
                return login_page(&next, Some(WRONG_TOKEN), None, StatusCode::UNAUTHORIZED);
            }
        }
    };

    let replacing = session.map(|Extension(session)| session.digest);
    let id = match open_session(&context.store, &access, member.as_ref(), replacing).await {
        Ok(Some(id)) => id,
        // The member was removed between the key check and the write: signed in
        // as nobody, the same answer as a wrong key.
        Ok(None) => {
            if !throttle.wrong_attempt().await {
                return login_page(&next, Some(TRY_AGAIN), None, StatusCode::TOO_MANY_REQUESTS);
            }
            return login_page(&next, Some(WRONG_TOKEN), None, StatusCode::UNAUTHORIZED);
        }
        Err(SignInFailure::NotTokenMode) => return StatusCode::NOT_FOUND.into_response(),
        Err(SignInFailure::Server) => {
            return login_page(
                &next,
                Some(SIGN_IN_FAILED),
                None,
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };
    tracing::info!("login accepted");
    let mut response = see_other(&guard::return_target(&next));
    response.headers_mut().append(
        header::SET_COOKIE,
        guard::session_cookie(&guard::hex(&id), SESSION_IDLE_SECONDS),
    );
    response
}

/// The member a sign-in key belongs to, when the value is one that is live.
async fn sign_in_key_member(store: &Store, presented: &str) -> Option<Member> {
    if credential_kind(presented) != Some(CredentialKind::SigninKey) {
        return None;
    }
    let store = store.clone();
    let value = presented.to_string();
    match tokio::task::spawn_blocking(move || store.member_by_signin_key(&value)).await {
        Ok(Ok(member)) => member,
        Ok(Err(error)) => {
            tracing::warn!(error = ?error, "the sign-in key lookup failed; the key is refused");
            None
        }
        Err(_) => None,
    }
}

/// Why a session could not be opened.
pub(crate) enum SignInFailure {
    /// Sessions exist in token mode only.
    NotTokenMode,
    /// The random source or the store failed.
    Server,
}

/// Open a browser session and return the id the cookie carries. `member` is the
/// member it belongs to, or `None` for the owner. `Ok(None)` means the member is
/// not live. The one function that creates a session, so the sign-in a hosted
/// assertion will do (HOS-16) is this call and nothing here verifies it.
pub(crate) async fn open_session(
    store: &Store,
    access: &Access,
    member: Option<&Member>,
    replacing: Option<String>,
) -> Result<Option<[u8; 32]>, SignInFailure> {
    let mut id = [0u8; 32];
    if getrandom::fill(&mut id).is_err() {
        tracing::error!("login failed: the system random source is unavailable");
        return Err(SignInFailure::Server);
    }
    let Some(digest) = access.session_digest(&id) else {
        return Err(SignInFailure::NotTokenMode);
    };
    let store = store.clone();
    let name = member.map(|member| member.name.clone());
    let stored = tokio::task::spawn_blocking(move || {
        store.browser_session_open_for(&digest, replacing.as_deref(), unix_now(), name.as_deref())
    })
    .await;
    match stored {
        Ok(Ok(true)) => Ok(Some(id)),
        Ok(Ok(false)) => Ok(None),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "login failed: the session could not be stored");
            Err(SignInFailure::Server)
        }
        Err(_) => Err(SignInFailure::Server),
    }
}

pub(super) async fn logout_post(
    context: Context,
    Extension(access): Extension<Access>,
    session: Option<Extension<Session>>,
) -> Response {
    if !access.is_token() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(Extension(session)) = session {
        let store = context.store.clone();
        let outcome =
            tokio::task::spawn_blocking(move || store.browser_session_close(&session.digest)).await;
        if !matches!(outcome, Ok(Ok(()))) {
            tracing::error!("sign out failed: the session could not be deleted");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "the session could not be ended; try again\n",
            )
                .into_response();
        }
    }
    let mut response = see_other("/login?notice=signed-out");
    response
        .headers_mut()
        .append(header::SET_COOKIE, guard::session_cookie("", 0));
    response
}
