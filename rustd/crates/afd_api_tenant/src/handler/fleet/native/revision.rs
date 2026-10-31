//! Apply accepts only the native domain's checked, replayable replacement.

use super::{access, render::attach};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::fleet::detail::FleetPath,
    handler::{Refusal, input},
    services::Services,
};
use afd_api_wire::fleet as wire;
use afd_authoring::types::{AppliedRevision, ApplyRevision};
use afd_http::handler::authoring_refusal;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode, header};
use std::sync::Arc;

#[cfg_attr(feature="openapi",utoipa::path(
    post,path="/v1/workspaces/{workspace_id}/fleets/{fleet_id}/revisions",operation_id="apply_fleet_revision",tag=afd_http::openapi::tag::FLEETS,
    summary="Apply a reviewed fleet improvement",params(afd_http::openapi::path::Fleet,
        ("If-Match"=String,Header,description="Current setup tag"),("Idempotency-Key"=String,Header,description="Required UUIDv7 key")),
    request_body=wire::ApplyRevision,responses((status=201,body=wire::AppliedRevision,description="Accepted setup revision",headers(("ETag"=String),("Location"=String))),
        (status=400,description="Invalid input"),(status=401,description="Unauthorized"),(status=403,description="Forbidden"),(status=404,description="Fleet or private candidate unavailable"),
        (status=409,description="Changed source, review, state or operation"),(status=410,description="Accepted revision purged"),(status=412,description="Stale setup tag"),(status=428,description="Missing setup tag"),
        (status=413,description="Body too large"),(status=429,description="Too many requests"),(status=500,description="Internal failure"),(status=503,description="Dependency unavailable"))
))]
pub(crate) async fn apply<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: wire::ApplyRevision = input::body(&body)?;
    let fleet = input::uuid(&path.fleet_id)?;
    let applied = services
        .creations()
        .apply_revision(
            access(&owned, &person),
            &fleet,
            input::expected(&headers)?,
            &input::operation(&headers)?,
            ApplyRevision {
                origin: request.origin.map(super::input::origin),
                draft_candidate_id: input::uuid(&request.draft_candidate_id)?,
                review_digest: request.review_digest,
            },
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    let current = services
        .creations()
        .read(access(&owned, &person), &fleet)
        .await
        .map_err(authoring_refusal)?;
    let schedules = super::schedules::after_activation(
        services.as_ref(),
        &owned.workspace,
        &fleet,
        current.status == "active",
    )
    .await?;
    let location = format!("/v1/workspaces/{}/fleets/{fleet}", owned.workspace);
    Ok(created(applied, &schedules, &location, services.now()))
}

/// The `201` for an accepted revision: the setup it settled on and the
/// schedules active after it, tagged with the fleet's new resource tag.
fn created(
    applied: AppliedRevision,
    schedules: &[afd_cron::Schedule],
    location: &str,
    now: jiff::Timestamp,
) -> Response {
    let tag = applied.etag.clone();
    let value = wire::AppliedRevision {
        fleet_id: applied.fleet_id.to_string(),
        setup_revision: applied.setup_revision,
        setup_etag: applied.setup_etag,
        etag: applied.etag,
        review_digest: applied.review_digest,
        schedules: crate::handler::schedule::views(schedules, now),
    };
    let mut response = (StatusCode::CREATED, Json(value)).into_response();
    attach(&mut response, header::ETAG, &tag);
    attach(&mut response, header::LOCATION, location);
    response
}
