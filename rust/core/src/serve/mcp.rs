//! `POST /mcp` and `GET /health` (ADR 0047): the agent door and the liveness
//! probe on the one router.
//!
//! The handler is a transport and nothing else: the body goes to
//! [`crate::mcp::handle_mcp`] once, and its frame comes back as the response
//! body. No JSON-RPC is parsed here and no tool is named here, so the door has
//! one dispatch (ADR 0009). Streamable HTTP, JSON responses only and
//! stateless: no `Mcp-Session-Id` is issued, `GET` and `DELETE` answer `405`.

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{Context, bad_request};

/// The header a client names its protocol version in, after `initialize`.
const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// `POST /mcp`: one frame in, one frame out, or `202` for a notification.
pub(super) async fn mcp_post(context: Context, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(value) = headers.get(PROTOCOL_VERSION_HEADER) {
        let known = value
            .to_str()
            .ok()
            .is_some_and(|version| crate::mcp::PROTOCOL_VERSIONS.contains(&version));
        if !known {
            return bad_request("the MCP-Protocol-Version is not one this server speaks");
        }
    }
    let Ok(frame) = String::from_utf8(body.to_vec()) else {
        return bad_request("the body is not UTF-8");
    };
    // `handle_mcp` is synchronous and a write can wait out the store's busy
    // window; every session shares this one runtime, so it runs off it.
    let answer = tokio::task::spawn_blocking(move || {
        crate::mcp::handle_mcp(&frame, &context.store, &context.actor, &context.environment)
    })
    .await;
    match answer {
        Ok(Some(frame)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            frame,
        )
            .into_response(),
        Ok(None) => StatusCode::ACCEPTED.into_response(),
        Err(_) => {
            tracing::error!("the MCP dispatch panicked");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /health`: a fixed body. It carries no version, path, actor or store
/// state, so it is safe to leave open on any bind.
pub(super) async fn health() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "ok\n",
    )
        .into_response()
}
