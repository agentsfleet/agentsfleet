//! Model choices resolve from the workspace billing owner after fresh membership.

use super::access;
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::Services,
};
#[cfg(feature = "openapi")]
use afd_api_wire::authoring::{CursorPage, WorkspaceModel};
use afd_authoring::types::ModelsQuery;
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/models", operation_id = "list_workspace_models",
    tag = afd_http::openapi::tag::DRAFTS, summary = "List the workspace owner's supported model choices",
    description = "Requires fleet:read and current membership. References and prices belong to the workspace billing owner; no key or key location is returned.",
    params(afd_http::openapi::path::Workspace,
        ("purpose" = Option<String>, Query, description = "authoring or runtime; omit for either"),
        ("limit" = Option<u32>, Query, description = "Rows scanned per page, 1..100; default 50"),
        ("starting_after" = Option<String>, Query, description = "Opaque owner/workspace/person/purpose/page-size bound cursor")),
    responses((status = 200, body = CursorPage<WorkspaceModel>, description = "Configured priced choices"),
        (status = 400, description = "Invalid input or cursor"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Workspace unavailable"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    query: Result<Query<ModelsQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(query) =
        query.map_err(|_invalid| Refusal::malformed("Invalid model selection query"))?;
    let page = services
        .authoring()
        .models(access(&owned, &person), query)
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(page).into_response())
}
