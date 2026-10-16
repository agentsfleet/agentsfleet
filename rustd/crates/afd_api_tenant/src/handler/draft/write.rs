//! Native draft writes share the same service and replay semantics for both clients.

use super::{DraftPath, access, attach, id, input, location, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::Services,
};
#[cfg(feature = "openapi")]
use afd_api_wire::authoring::CreateDraftRequest;
use afd_api_wire::authoring::Created;
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

/// Creates the draft and optional first message in one durable transaction.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/v1/workspaces/{workspace_id}/drafts", operation_id = "create_workspace_draft",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Start a private workspace conversation",
    description = "Requires fleet:write. Use only body operation_id; Idempotency-Key is refused. Native sources pin their immutable revisions.",
    params(afd_http::openapi::path::Workspace), request_body = CreateDraftRequest,
    responses((status = 201, body = Created, description = "Created or exact replay", headers(("Location" = String), ("ETag" = String))),
        (status = 400, description = "Invalid native setup or input"), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = "Source unavailable"),
        (status = 409, description = "Changed operation or unavailable model"), (status = 410, description = "Accepted resource purged"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    input::body_key(&headers)?;
    let mut created = services
        .authoring()
        .create(
            access(&owned, &person),
            input::create(&body)?,
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    created.draft = services
        .authoring_setup()
        .read_draft(access(&owned, &person), &created.draft.id)
        .await
        .map_err(authoring_refusal)?;
    let id = created.draft.id.clone();
    let tag = created.draft.etag.clone();
    let payload = Created {
        draft: render::draft(created.draft),
        accepted: created.accepted.map(render::accepted),
    };
    let mut response = (StatusCode::CREATED, Json(payload)).into_response();
    attach(&mut response, &tag);
    location(&mut response, &owned.workspace, &id, "");
    Ok(response)
}

/// Deletes creator-private text and retains bounded content-free replay identity.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}", operation_id = "delete_workspace_draft",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Delete your private draft",
    params(afd_http::openapi::path::Draft),
    responses((status = 204, description = "Private durable text deleted"), (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED), (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = super::DRAFT_UNAVAILABLE), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL), (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
) -> Result<Response, Refusal> {
    services
        .authoring()
        .delete(access(&owned, &person), &id(&path)?, services.now())
        .await
        .map_err(authoring_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
