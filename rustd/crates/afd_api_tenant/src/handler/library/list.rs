//! Gallery, owned entries, and history share bounded keyset pagination.

use super::{LibraryPath, access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, paging},
    services::Services,
};
use afd_api_wire::{authoring::CursorPage, library as wire};
use afd_authoring::types::LibraryQuery;
use afd_http::handler::authoring_refusal;
use axum::{
    Json,
    extract::{Path, Query, RawQuery, State, rejection::QueryRejection},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

const INVALID_QUERY: &str = "Invalid Library list query";

/// The Library family's page query, its `limit` refused under the family's
/// own code before the rest of the query is read.
fn page(
    raw: Option<&str>,
    query: Result<Query<LibraryQuery>, QueryRejection>,
) -> Result<LibraryQuery, Refusal> {
    paging::requested_limit(raw.unwrap_or_default())?;
    let Query(query) = query.map_err(|_invalid| Refusal::malformed(INVALID_QUERY))?;
    Ok(query)
}

/// The 400 every Library collection answers for a page it cannot read.
#[cfg(feature = "openapi")]
const INVALID_PAGE: &str = "Invalid query or cursor";

#[cfg_attr(feature="openapi",utoipa::path(
    get,path="/v1/workspaces/{workspace_id}/fleet-libraries",operation_id="list_workspace_libraries",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="List available Library entries",params(afd_http::openapi::path::Workspace,
        afd_http::openapi::query::LibraryPage),
    responses((status=200,body=CursorPage<wire::Card>,description="Platform and owned entries"),
        (status=400,description=INVALID_PAGE),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=afd_http::openapi::NOT_FOUND),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn gallery<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    RawQuery(raw): RawQuery,
    query: Result<Query<LibraryQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let query = page(raw.as_deref(), query)?;
    let page = services
        .authoring_setup()
        .libraries(access(&owned, &person), query, false)
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
    get,path="/v1/workspaces/{workspace_id}/fleet-libraries/owned",operation_id="list_owned_workspace_libraries",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="List owned Library entries",params(afd_http::openapi::path::Workspace,
        afd_http::openapi::query::LibraryPage),
    responses((status=200,body=CursorPage<wire::Card>,description="Owned entries"),
        (status=400,description=INVALID_PAGE),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description=afd_http::openapi::NOT_FOUND),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn owned<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    RawQuery(raw): RawQuery,
    query: Result<Query<LibraryQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let query = page(raw.as_deref(), query)?;
    let page = services
        .authoring_setup()
        .libraries(access(&owned, &person), query, true)
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
    get,path="/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}/revisions",operation_id="list_library_revisions",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="List immutable Library revisions",params(afd_http::openapi::path::Library,afd_http::openapi::query::LibraryPage),
    responses((status=200,body=CursorPage<wire::RevisionSummary>,description="Revision metadata"),
        (status=400,description=INVALID_PAGE),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description="Library unavailable"),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn history<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<LibraryPath>,
    RawQuery(raw): RawQuery,
    query: Result<Query<LibraryQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let query = page(raw.as_deref(), query)?;
    let page = services
        .authoring_setup()
        .library_revisions(access(&owned, &person), &path.library_id, query)
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
