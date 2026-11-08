//! Deleting the shared entry leaves fleet-owned immutable snapshots intact.

use super::{LibraryPath, access};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::Refusal,
    services::Services,
};
use afd_http::handler::authoring_refusal;
use axum::{
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::StatusCode;
use std::sync::Arc;

#[cfg_attr(feature="openapi",utoipa::path(
    delete,path="/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}",operation_id="delete_workspace_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Delete an owned Library entry",params(afd_http::openapi::path::Workspace,("library_id"=String,Path,description="Library identifier")),
    responses((status=204,description="Entry removed; installed snapshots retained"),
        (status=400,description="Invalid input"),(status=401,description="Unauthorized"),(status=403,description="Forbidden"),
        (status=404,description="Owned Library unavailable"),(status=429,description="Too many requests"),(status=500,description="Internal failure"),(status=503,description="Dependency unavailable"))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<LibraryPath>,
) -> Result<Response, Refusal> {
    services
        .authoring_setup()
        .delete_library(access(&owned, &person), &path.library_id)
        .await
        .map_err(authoring_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
