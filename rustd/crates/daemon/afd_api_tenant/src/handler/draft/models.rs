//! Model choices resolve from the workspace billing owner after fresh membership.

use super::access;
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::TenantPlane,
};
#[cfg(feature = "openapi")]
use afd_api_wire::setup::{CursorPage, WorkspaceModel};
use afd_http::handler::{Refusal, setup_refusal};
use afd_setup::types::ModelsQuery;
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/models", operation_id = "list_workspace_models",
    tag = afd_http::openapi::tag::DRAFTS, summary = "List workspace model choices",
    description = concat!(
        "Lists models available to this workspace's billing account. Requires `fleet:read` ",
        "and current membership. Credentials and their storage locations are never ",
        "returned.",
    ),
    params(afd_http::openapi::path::Workspace,
        ("purpose" = Option<String>, Query, description = "setup or runtime; omit for either"),
        ("limit" = Option<u32>, Query, description = "Rows scanned per page, 1..100; default 50"),
        ("starting_after" = Option<String>, Query, description = concat!(
            "Use the previous `next_cursor` unchanged, for the same owner, workspace, person, ",
            "purpose, and page size.",
        ))),
    responses((status = 200, body = CursorPage<WorkspaceModel>, description = "Configured priced choices"),
        (status = 400, description = "Invalid input or cursor"), (status = 404, description = "Workspace unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn list<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    query: Result<Query<ModelsQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(query) =
        query.map_err(|_invalid| Refusal::malformed("Invalid model selection query"))?;
    let page = services
        .drafts()
        .models(access(&owned, &person), query)
        .await
        .map_err(setup_refusal)?;
    Ok(Json(page).into_response())
}
