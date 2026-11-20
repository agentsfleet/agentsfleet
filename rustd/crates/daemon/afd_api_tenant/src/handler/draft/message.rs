//! Durable private message acceptance and role paging.

use super::{DraftPath, access, id, input, location, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::TenantPlane,
};
#[cfg(feature = "openapi")]
use afd_api_wire::setup::{Accepted, CursorPage, Message, MessageRequest};
use afd_http::handler::{Refusal, setup_refusal};
use afd_setup::types::MessageQuery;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

/// Accepts a user request; execution and terminal answers happen separately.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}/messages", operation_id = "send_workspace_draft_message",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Send a private setup message",
    description = concat!(
        "Sends a private setup message. Requires `fleet:write`. Use the body's ",
        "`operation_id` for retries; `Idempotency-Key` is refused. A 202 response confirms ",
        "the message is saved for processing.",
    ),
    params(afd_http::openapi::path::Draft),
    request_body = MessageRequest,
    responses((status = 202, body = Accepted, description = "Durably queued or exact replay", headers(("Location" = String))),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 404, description = super::DRAFT_UNAVAILABLE),
        (status = 409, description = "State or replay conflict"), (status = 410, description = "Accepted resource purged"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn send<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    input::body_key(&headers)?;
    let id = id(&path)?;
    let accepted = services
        .drafts()
        .send(
            access(&owned, &person),
            &id,
            input::send(&body)?,
            services.now(),
        )
        .await
        .map_err(setup_refusal)?;
    let mut response = (StatusCode::ACCEPTED, Json(render::accepted(accepted))).into_response();
    location(&mut response, &owned.workspace, &id, "/messages");
    Ok(response)
}

/// Reads a bounded role-ordered conversation after current creator authorization.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}/messages", operation_id = "list_workspace_draft_messages",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Read your private setup messages",
    params(afd_http::openapi::path::Draft,
        ("limit" = Option<u32>, Query, description = "Page size, 1..25; default 20."),
        ("starting_after" = Option<String>, Query, description = "Use the previous `next_cursor` unchanged, for the same creator, draft, and page size.")),
    responses((status = 200, body = CursorPage<Message>, description = "Sequence then role order"),
        (status = 400, description = "Invalid input or cursor"), (status = 404, description = super::DRAFT_UNAVAILABLE),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn list<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
    query: Result<Query<MessageQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(query) =
        query.map_err(|_invalid| Refusal::malformed("Invalid private message query"))?;
    let messages = services
        .drafts()
        .messages(access(&owned, &person), &id(&path)?, query)
        .await
        .map_err(setup_refusal)?;
    Ok(Json(render::page(messages, render::message)).into_response())
}
