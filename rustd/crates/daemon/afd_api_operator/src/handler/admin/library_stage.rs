//! Stages a creator-owned native candidate under the separate platform scope.

use crate::{auth::PersonIdentity, services::Services};
use afd_api_wire::admin_library_native::Stage;
use afd_authoring::{Access, types::PublishLibrary};
use afd_http::handler::{Refusal, authoring_refusal, setup_input as input};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode, header};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path="/v1/admin/fleet-libraries", operation_id="stage_platform_fleet_library",
    tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Stage a private chat candidate in the platform Library",
    params(("Idempotency-Key"=String, Header, description="Required UUIDv7 replay key")),
    request_body=Stage,
    responses((status=201, body=afd_api_wire::library::Published, description="Native revision staged; explicit publication is separate"),
        (status=400,description="Invalid candidate or label"),(status=401,description="Unauthorized"),(status=403,description="Platform scope required"),
        (status=404,description="Creator-owned source unavailable"),(status=409,description="Busy draft, occupied slug or changed replay"),
        (status=410,description="Staged entry purged"),(status=413,description="Body too large"),(status=429,description="Too many requests"),
        (status=500,description="Internal failure"),(status=503,description="Dependency unavailable"))
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let key = input::operation(&headers)?;
    let request: Stage = input::body(&body)?;
    let workspace = input::uuid(&request.workspace_id)?;
    let result = services
        .authoring_setup()
        .stage_platform_library(
            Access {
                workspace: &workspace,
                subject: identity.subject(),
            },
            &key,
            PublishLibrary {
                draft_candidate_id: input::uuid(&request.draft_candidate_id)?,
                label: request.label,
            },
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    let location = format!("/v1/admin/fleet-libraries/{}", result.library_id);
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, result.etag.clone()),
            (header::LOCATION, location),
        ],
        Json(result),
    )
        .into_response())
}
