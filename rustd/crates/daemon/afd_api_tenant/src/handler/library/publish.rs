//! Publication headers and private candidate identity reach one domain transaction.

use super::{access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{Refusal, input},
    services::Services,
};
use afd_api_wire::library as wire;
use afd_authoring::types;
use afd_http::handler::authoring_refusal;
use axum::{body::Bytes, extract::State, response::Response};
use http::HeaderMap;
use std::sync::Arc;

/// The 409 both publications answer: the candidate is busy, or its key was
/// accepted with a different body.
#[cfg(feature = "openapi")]
pub(super) const BUSY_OR_CHANGED: &str = "Busy or changed operation";

#[cfg_attr(feature="openapi",utoipa::path(
    post,path="/v1/workspaces/{workspace_id}/fleet-libraries",operation_id="publish_workspace_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Publish a chat candidate to Library",params(afd_http::openapi::path::Workspace,afd_http::openapi::header::IdempotencyKey),
    request_body=wire::Publish,responses((status=201,body=wire::Published,description="Explicit publication",headers(("ETag"=String),("Location"=String))),
        (status=400,description="Invalid candidate or label"),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=afd_http::openapi::FORBIDDEN),
        (status=404,description="Private candidate unavailable"),(status=409,description=BUSY_OR_CHANGED),(status=410,description=afd_http::openapi::GONE),
        (status=413,description=afd_http::openapi::PAYLOAD_TOO_LARGE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let key = input::operation(&headers)?;
    let request: wire::Publish = input::body(&body)?;
    let result = services
        .authoring_setup()
        .publish(
            access(&owned, &person),
            &key,
            types::PublishLibrary {
                draft_candidate_id: input::uuid(&request.draft_candidate_id)?,
                label: request.label,
            },
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    let library = result.library_id.clone();
    let mut response = render::published(&owned.workspace, result);
    render::attach(
        &mut response,
        http::header::LOCATION,
        &format!(
            "/v1/workspaces/{}/fleet-libraries/{library}",
            owned.workspace
        ),
    );
    Ok(response)
}
