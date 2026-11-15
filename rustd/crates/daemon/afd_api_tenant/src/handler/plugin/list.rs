//! Account-owned entries and history share bounded keyset pagination.

use super::{PluginPath, access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, paging},
    services::Services,
};
use afd_api_wire::{authoring::CursorPage, plugin as wire};
use afd_authoring::types::PluginQuery;
use afd_http::handler::authoring_refusal;
use axum::{
    Json,
    extract::{Path, Query, RawQuery, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

const INVALID_QUERY: &str = "Invalid Plugin list query";

/// The Plugin family's page query, its `limit` refused under the family's
/// own code before the rest of the query is read.
fn page(
    raw: Option<&str>,
    query: Result<Query<PluginQuery>, QueryRejection>,
) -> Result<PluginQuery, Refusal> {
    paging::requested_limit(raw.unwrap_or_default())?;
    let Query(query) = query.map_err(|_invalid| Refusal::malformed(INVALID_QUERY))?;
    Ok(query)
}

/// The 400 every Plugin collection answers for a page it cannot read.
#[cfg(feature = "openapi")]
const INVALID_PAGE: &str = "Invalid query or cursor";

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/plugins",operation_id="list_workspace_plugins",tag=afd_http::openapi::tag::PLUGIN,
    summary="List available Plugin entries",params(afd_http::openapi::path::Workspace,
        afd_http::openapi::query::PluginPage),
    responses((status=200,body=CursorPage<wire::Card>,description="Account-owned entries"),
        (status=400,description=INVALID_PAGE),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=afd_http::openapi::NOT_FOUND),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn gallery<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    RawQuery(raw): RawQuery,
    query: Result<Query<PluginQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let query = page(raw.as_deref(), query)?;
    let page = services
        .authoring_setup()
        .plugins(access(&owned, &person), query)
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(CursorPage {
        items: page.items.into_iter().map(render::card).collect::<Vec<_>>(),
        total: page.total,
        next_cursor: page.next_cursor,
    })
    .into_response())
}

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/plugins/{plugin_id}/revisions",operation_id="list_plugin_revisions",tag=afd_http::openapi::tag::PLUGIN,
    summary="List immutable Plugin revisions",params(afd_http::openapi::path::Plugin,afd_http::openapi::query::PluginPage),
    responses((status=200,body=CursorPage<wire::RevisionSummary>,description="Revision metadata"),
        (status=400,description=INVALID_PAGE),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description="Plugin unavailable"),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn history<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<PluginPath>,
    RawQuery(raw): RawQuery,
    query: Result<Query<PluginQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let query = page(raw.as_deref(), query)?;
    let page = services
        .authoring_setup()
        .plugin_revisions(
            access(&owned, &person),
            &crate::handler::input::uuid(&path.plugin_id)?,
            query,
        )
        .await
        .map_err(authoring_refusal)?;
    let items = page
        .items
        .into_iter()
        .map(|row| wire::RevisionSummary {
            id: row.id.to_string(),
            revision: row.revision,
            canonical_digest: row.canonical_digest,
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();
    Ok(Json(CursorPage {
        items,
        total: page.total,
        next_cursor: page.next_cursor,
    })
    .into_response())
}
