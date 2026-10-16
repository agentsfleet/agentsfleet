//! A new immutable revision of an owned entry, published from one private candidate.

#[cfg(feature = "openapi")]
use super::publish::BUSY_OR_CHANGED;
use super::{LibraryPath, access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, input},
    services::Services,
};
use afd_api_wire::library as wire;
use afd_authoring::types;
use afd_http::handler::authoring_refusal;
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use http::HeaderMap;
use std::sync::Arc;

/// Publishes the candidate as the entry's next revision, under the entry's tag.
#[cfg_attr(feature="openapi",utoipa::path(
    post,path="/v1/workspaces/{workspace_id}/fleet-libraries/{library_id}/revisions",operation_id="publish_library_revision",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Publish a revision to an owned Library",params(afd_http::openapi::path::Library,afd_http::openapi::header::IdempotencyKey,
        afd_http::openapi::header::IfMatch),
    request_body=wire::PublishRevision,responses((status=201,body=wire::Published,description="Explicit immutable revision",headers(("ETag"=String),("Location"=String))),
        (status=400,description=afd_http::openapi::BAD_REQUEST),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),(status=404,description="Entry or candidate unavailable"),
        (status=409,description=BUSY_OR_CHANGED),(status=410,description=afd_http::openapi::GONE),(status=412,description="Stale Library tag"),(status=428,description="Missing Library tag"),
        (status=413,description=afd_http::openapi::PAYLOAD_TOO_LARGE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn revision<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<LibraryPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let key = input::operation(&headers)?;
    let request: wire::PublishRevision = input::body(&body)?;
    let result = services
        .authoring_setup()
        .publish_revision(
            access(&owned, &person),
            &path.library_id,
            input::expected(&headers)?,
            &key,
            types::PublishRevision {
                draft_candidate_id: input::uuid(&request.draft_candidate_id)?,
            },
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    Ok(render::published(&owned.workspace, result))
}
