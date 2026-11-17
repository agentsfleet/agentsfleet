//! Deleting the shared entry leaves fleet-owned immutable snapshots intact.

use super::{PluginPath, access};
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
    delete,path="/v1/workspaces/{workspace_id}/plugins/{plugin_id}",operation_id="delete_workspace_plugin",tag=afd_http::openapi::tag::PLUGIN,
    summary="Delete an owned Plugin entry",params(afd_http::openapi::path::Plugin),
    responses((status=204,description="Entry removed; installed snapshots retained"),
        (status=400,description="Invalid input"),
        (status=404,description="Owned Plugin unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<PluginPath>,
) -> Result<Response, Refusal> {
    services
        .authoring_setup()
        .delete_plugin(
            access(&owned, &person),
            &crate::handler::input::uuid(&path.plugin_id)?,
        )
        .await
        .map_err(authoring_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
