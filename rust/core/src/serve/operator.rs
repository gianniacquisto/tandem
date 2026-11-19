//! The instance's operator interface (CUT-40): `/operator/v1`, the hosting
//! service's narrow door. It adds, changes the role of, removes and counts
//! members, and nothing else. Every handler calls the same store member
//! functions every other door calls, as [`AccessActor::operator`], so there is
//! one writer, one last-admin rule and one access-record row per act, written in
//! the act's own transaction. The guard has already decided the caller holds the
//! operator key: no handler reads a header, a cookie or the actor header.
//!
//! Answers are JSON with a short reason and never a secret. The add response
//! carries the member's id, name and role and no credential.

// The rejection is the response the request gets, the shape every axum
// extractor answers a refusal in; a box would only move the error a layer out.
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::store::{AccessActor, MemberError, Role, unix_now};

use super::SessionContext;

fn answer(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn fail(status: StatusCode, reason: &str) -> Response {
    answer(status, json!({ "error": reason }))
}

/// A store that cannot be read or written: the reason is generic, never a path.
fn store_failure(error: &dyn std::fmt::Debug) -> Response {
    tracing::warn!(error = ?error, "an operator request could not be served");
    fail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "the store could not be used",
    )
}

fn member_failure(error: MemberError) -> Response {
    match error {
        MemberError::Invalid(_) => fail(StatusCode::BAD_REQUEST, &error.to_string()),
        MemberError::Taken | MemberError::LastAdmin | MemberError::NoAdmin => {
            fail(StatusCode::CONFLICT, &error.to_string())
        }
        MemberError::Store(error) => store_failure(&error),
    }
}

/// Run a blocking store call off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Response> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| store_failure(&error))
}

/// `GET /operator/v1/members/count`: the human seat count, a number and nothing
/// else. Read-only: it writes no row of the access record.
pub(super) async fn count(State(session): State<Arc<SessionContext>>) -> Response {
    let store = session.store.clone();
    match blocking(move || store.members()).await {
        Ok(Ok(members)) => answer(StatusCode::OK, json!({ "members": members.len() })),
        Ok(Err(error)) => store_failure(&error),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewMember {
    name: String,
    role: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleChange {
    role: String,
}

fn parse_role(value: &str) -> Result<Role, Response> {
    Role::parse(value).ok_or_else(|| {
        fail(
            StatusCode::BAD_REQUEST,
            "role must be \"member\" or \"admin\"",
        )
    })
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, Response> {
    serde_json::from_slice(body).map_err(|_| {
        fail(
            StatusCode::BAD_REQUEST,
            "the body must be the JSON object this route documents",
        )
    })
}

/// `POST /operator/v1/members`: add a member. The answer holds no credential: a
/// new member signs in by an admin's sign-in key or a signed assertion.
pub(super) async fn add(State(session): State<Arc<SessionContext>>, body: Bytes) -> Response {
    let new: NewMember = match parse_body(&body) {
        Ok(new) => new,
        Err(response) => return response,
    };
    let role = match parse_role(&new.role) {
        Ok(role) => role,
        Err(response) => return response,
    };
    let store = session.store.clone();
    let name = new.name;
    match blocking(move || store.member_add_by(&name, role, unix_now(), &AccessActor::operator()))
        .await
    {
        Ok(Ok(member)) => answer(
            StatusCode::CREATED,
            json!({ "id": member.id, "name": member.name, "role": member.role.as_str() }),
        ),
        Ok(Err(error)) => member_failure(error),
        Err(response) => response,
    }
}

/// `PATCH /operator/v1/members/{name}`: change a member's role.
pub(super) async fn set_role(
    State(session): State<Arc<SessionContext>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    let change: RoleChange = match parse_body(&body) {
        Ok(change) => change,
        Err(response) => return response,
    };
    let role = match parse_role(&change.role) {
        Ok(role) => role,
        Err(response) => return response,
    };
    let store = session.store.clone();
    let target = name.clone();
    match blocking(move || {
        store.member_set_role_by(&target, role, unix_now(), &AccessActor::operator())
    })
    .await
    {
        Ok(Ok(true)) | Ok(Ok(false)) if role_is_current(&session, &name, role) => answer(
            StatusCode::OK,
            json!({ "name": name, "role": role.as_str() }),
        ),
        Ok(Ok(_)) => fail(StatusCode::NOT_FOUND, "there is no such member"),
        Ok(Err(error)) => member_failure(error),
        Err(response) => response,
    }
}

/// Whether the live member `name` now holds `role`: a change that matched
/// nothing is a `404`, and one that set the role it already had is a `200`.
fn role_is_current(session: &SessionContext, name: &str, role: Role) -> bool {
    matches!(session.store.member_find(name), Ok(Some(member)) if member.role == role)
}

/// `DELETE /operator/v1/members/{name}`: remove a member. Their sessions,
/// tokens and sign-in key end in the same write, and the name is never reused.
pub(super) async fn remove(
    State(session): State<Arc<SessionContext>>,
    Path(name): Path<String>,
) -> Response {
    let store = session.store.clone();
    let target = name.clone();
    match blocking(move || store.member_remove_by(&target, unix_now(), &AccessActor::operator()))
        .await
    {
        Ok(Ok(true)) => answer(StatusCode::OK, json!({ "name": name, "removed": true })),
        Ok(Ok(false)) => fail(StatusCode::NOT_FOUND, "there is no such member"),
        Ok(Err(error)) => member_failure(error),
        Err(response) => response,
    }
}
