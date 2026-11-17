//! The browser sign-in (ADR 0049): `/login`, `/logout`, and the throttle on a
//! wrong token. All of it exists in token mode only; every other mode answers
//! `404`, because there is nothing to sign in to.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use super::guard::{self, Session};
use super::*;
use crate::store::{SESSION_IDLE_SECONDS, unix_now};

/// How long a wrong token waits before it is answered.
#[cfg(not(test))]
const WRONG_TOKEN_DELAY: Duration = Duration::from_secs(1);
#[cfg(test)]
const WRONG_TOKEN_DELAY: Duration = Duration::from_millis(150);

/// How many wrong tokens may wait at once; the excess is turned away with `429`.
/// Global, not per address: the peer is the proxy and `X-Forwarded-For` is
/// untrusted until the public origin setting (CUT-37) says otherwise.
const WRONG_TOKENS_AT_ONCE: usize = 4;

const WRONG_TOKEN: &str = "That token was not accepted.";
const TRY_AGAIN: &str = "Too many attempts at once. Try again in a moment.";
const SIGN_IN_FAILED: &str = "Signing in failed on the server. Try again in a moment.";
const SIGNED_OUT: &str = "You are signed out.";

/// The shared wait list for wrong tokens.
#[derive(Debug, Clone)]
pub(super) struct LoginThrottle(Arc<Semaphore>);

impl LoginThrottle {
    pub(super) fn new() -> Self {
        Self(Arc::new(Semaphore::new(WRONG_TOKENS_AT_ONCE)))
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
    let notice = (field(&params, "notice") == "signed-out").then_some(SIGNED_OUT);
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
    // The token is trimmed: a paste from a phone carries a trailing space.
    let presented = field(&params, "password").trim();
    if !access.token_matches(presented) {
        // A wrong token waits, a few at a time; a right one never does.
        let Ok(permit) = throttle.0.clone().try_acquire_owned() else {
            tracing::warn!("login refused: too many wrong tokens waiting");
            return login_page(&next, Some(TRY_AGAIN), None, StatusCode::TOO_MANY_REQUESTS);
        };
        tokio::time::sleep(WRONG_TOKEN_DELAY).await;
        drop(permit);
        tracing::warn!("login refused: the token was not accepted");
        return login_page(&next, Some(WRONG_TOKEN), None, StatusCode::UNAUTHORIZED);
    }

    let mut id = [0u8; 32];
    if getrandom::fill(&mut id).is_err() {
        tracing::error!("login failed: the system random source is unavailable");
        return login_page(
            &next,
            Some(SIGN_IN_FAILED),
            None,
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }
    let Some(digest) = access.session_digest(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let replacing = session.map(|Extension(session)| session.digest);
    let store = context.store.clone();
    let stored = tokio::task::spawn_blocking(move || {
        store.browser_session_open(&digest, replacing.as_deref(), unix_now())
    })
    .await;
    match stored {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "login failed: the session could not be stored");
            return login_page(
                &next,
                Some(SIGN_IN_FAILED),
                None,
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
        Err(_) => {
            return login_page(
                &next,
                Some(SIGN_IN_FAILED),
                None,
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    }
    tracing::info!("login accepted");
    let mut response = see_other(&guard::return_target(&next));
    response.headers_mut().append(
        header::SET_COOKIE,
        guard::session_cookie(&guard::hex(&id), SESSION_IDLE_SECONDS),
    );
    response
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
