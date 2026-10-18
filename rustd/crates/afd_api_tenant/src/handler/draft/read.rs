//! Creator-only draft collection and current candidate reads.

use super::{DraftPath, access, attach, id, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::Services,
};
#[cfg(feature = "openapi")]
use afd_api_wire::authoring::{CursorPage, Draft, DraftCard};
use afd_authoring::types::ListQuery;
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

/// Lists only the authenticated creator's drafts in this workspace.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/drafts", operation_id = "list_workspace_drafts",
    tag = afd_http::openapi::tag::DRAFTS, summary = "List your private workspace drafts",
    params(afd_http::openapi::path::Workspace,
        ("limit" = Option<u32>, Query, description = "Page size, 1..100; default 50."),
        ("starting_after" = Option<String>, Query, description = "Opaque creator/filter-bound cursor."),
        ("state" = Option<String>, Query, description = "editing, saving or closed."),
        ("linked_fleet_id" = Option<String>, Query, description = "Related fleet UUIDv7."),
        ("q" = Option<String>, Query, description = "Case-insensitive search of your chat titles and message text, 1..200 characters after trimming. Bound into the cursor.")),
    responses((status = 200, body = CursorPage<DraftCard>, description = "Creator-only page"),
        (status = 400, description = "Invalid input or cursor"), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = "Workspace unavailable"),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(query) = query.map_err(|_invalid| Refusal::malformed("Invalid draft list query"))?;
    let page = services
        .authoring()
        .list(access(&owned, &person), query)
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(render::page(page, render::card)).into_response())
}

/// Reads only the authenticated creator's private candidate.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}", operation_id = "read_workspace_draft",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Read your private workspace draft",
    params(afd_http::openapi::path::Draft),
    responses((status = 200, body = Draft, description = "Private draft", headers(("ETag" = String))),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = super::DRAFT_UNAVAILABLE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
) -> Result<Response, Refusal> {
    let draft = services
        .authoring_setup()
        .read_draft(access(&owned, &person), &id(&path)?)
        .await
        .map_err(authoring_refusal)?;
    let tag = draft.etag.clone();
    let mut response = Json(render::draft(draft)).into_response();
    attach(&mut response, &tag);
    Ok(response)
}
