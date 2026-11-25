//! The workspace's model choices, resolved from its billing owner after a
//! fresh membership check.

use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::TenantPlane,
};
#[cfg(feature = "openapi")]
use afd_api_wire::workspace_model::{CursorPage, WorkspaceModel};
use afd_http::handler::{Refusal, setup_refusal};
use afd_setup::{Access, types::ModelsQuery};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
    routing::MethodRouter,
};
use http::{HeaderValue, header};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/models", operation_id = "list_workspace_models",
    tag = afd_http::openapi::tag::FLEETS, summary = "List workspace model choices",
    description = concat!(
        "Lists models available to this workspace's billing account. Requires `fleet:read` ",
        "and current membership. Credentials and their storage locations are never ",
        "returned.",
    ),
    params(afd_http::openapi::path::Workspace,
        ("purpose" = Option<String>, Query, description = "runtime keeps only models a fleet can run on; omit for every model"),
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
    let access = Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    };
    let page = services
        .creations()
        .models(access, query)
        .await
        .map_err(setup_refusal)?;
    Ok(Json(page).into_response())
}

/// Keeps a person's model page out of shared response caches: its cursor is
/// bound to the person who read it.
pub(crate) fn private<D: TenantPlane>(router: MethodRouter<Arc<D>>) -> MethodRouter<Arc<D>> {
    router.layer(axum::middleware::map_response(mark_private))
}

async fn mark_private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}
