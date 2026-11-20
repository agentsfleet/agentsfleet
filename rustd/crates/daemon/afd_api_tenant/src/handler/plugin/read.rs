//! Immutable selected content is returned with current contextual readiness.

use super::{PluginPath, RevisionPath, access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, input},
    services::Services,
};
use afd_api_wire::plugin as wire;
use afd_http::handler::authoring_refusal;
use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use serde::Deserialize;
use std::sync::Arc;

/// The 404 both reads answer: the entry, or the pinned revision, is not one
/// this caller may see.
#[cfg(feature = "openapi")]
const SELECTION_UNAVAILABLE: &str = "Plugin or revision unavailable";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    revision_id: Option<String>,
}

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/plugins/{plugin_id}",operation_id="read_workspace_plugin",tag=afd_http::openapi::tag::PLUGIN,
    summary="Inspect a Plugin revision",params(afd_http::openapi::path::Plugin,
        ("revision_id"=Option<String>,Query,format="uuid",description="Pinned revision UUIDv7; defaults to current head")),
    responses((status=200,body=wire::Plugin,description="Native setup and current readiness",headers(("ETag"=String))),
        (status=400,description=afd_http::openapi::BAD_REQUEST),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=SELECTION_UNAVAILABLE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<PluginPath>,
    query: Result<Query<Selection>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(selection) =
        query.map_err(|_invalid| Refusal::malformed("Invalid Plugin selection"))?;
    let selected = selection
        .revision_id
        .as_deref()
        .map(input::uuid)
        .transpose()?;
    let plugin = services
        .authoring_setup()
        .plugin(
            access(&owned, &person),
            &input::uuid(&path.plugin_id)?,
            selected.as_ref(),
        )
        .await
        .map_err(authoring_refusal)?;
    Ok(render::plugin(plugin))
}

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/plugins/{plugin_id}/revisions/{revision_id}",operation_id="read_plugin_revision",tag=afd_http::openapi::tag::PLUGIN,
    summary="Inspect an immutable Plugin revision",params(afd_http::openapi::path::PluginRevision),
    responses((status=200,body=wire::RevisionRead,description="Immutable content and live readiness"),
        (status=400,description=afd_http::openapi::BAD_REQUEST),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=SELECTION_UNAVAILABLE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn revision<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<RevisionPath>,
) -> Result<Response, Refusal> {
    let plugin = services
        .authoring_setup()
        .plugin(
            access(&owned, &person),
            &input::uuid(&path.plugin_id)?,
            Some(&input::uuid(&path.revision_id)?),
        )
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(wire::RevisionRead {
        revision: render::revision(plugin.head),
        review: plugin.review,
    })
    .into_response())
}
