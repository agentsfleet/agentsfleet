//! Immutable selected content is returned with current contextual readiness.

use super::{LibraryPath, RevisionPath, access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, input},
    services::Services,
};
use afd_api_wire::library as wire;
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
const SELECTION_UNAVAILABLE: &str = "Library or revision unavailable";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    revision_id: Option<String>,
}

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}",operation_id="read_workspace_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Inspect a Library revision",params(afd_http::openapi::path::Library,
        ("revision_id"=Option<String>,Query,description="Pinned revision UUIDv7; defaults to current head")),
    responses((status=200,body=wire::Library,description="Native setup and current readiness",headers(("ETag"=String))),
        (status=400,description=afd_http::openapi::BAD_REQUEST),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=SELECTION_UNAVAILABLE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<LibraryPath>,
    query: Result<Query<Selection>, QueryRejection>,
) -> Result<Response, Refusal> {
    let Query(selection) =
        query.map_err(|_invalid| Refusal::malformed("Invalid Library selection"))?;
    let selected = selection
        .revision_id
        .as_deref()
        .map(input::uuid)
        .transpose()?;
    let library = services
        .authoring_setup()
        .library(access(&owned, &person), &path.library_id, selected.as_ref())
        .await
        .map_err(authoring_refusal)?;
    Ok(render::library(library))
}

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}/revisions/{revision_id}",operation_id="read_library_revision",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Inspect an immutable Library revision",params(afd_http::openapi::path::LibraryRevision),
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
    let library = services
        .authoring_setup()
        .library(
            access(&owned, &person),
            &path.library_id,
            Some(&input::uuid(&path.revision_id)?),
        )
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(wire::RevisionRead {
        revision: render::revision(library.head),
        review: library.review,
    })
    .into_response())
}
