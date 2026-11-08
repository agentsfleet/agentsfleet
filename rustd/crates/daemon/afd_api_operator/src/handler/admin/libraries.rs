//! Conditional platform curation over native Library revisions.

use crate::services::Services;
use afd_api_wire::admin_library_native::Patch;
use afd_authoring::types::PlatformPatch;
use afd_http::handler::{Refusal, authoring_refusal, setup_input as input};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{HeaderMap, StatusCode, header};
use std::sync::Arc;

/// The 403 every curation route answers: an authenticated caller without the
/// platform scope.
#[cfg(feature = "openapi")]
const PLATFORM_SCOPE: &str = "Platform scope required";

#[cfg_attr(feature="openapi", utoipa::path(
    get,path="/v1/admin/fleet-libraries",operation_id="list_platform_fleet_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="List staged and published native platform entries",
    responses((status=200,body=afd_api_wire::admin_library_native::List,description="Platform metadata"),
        (status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=PLATFORM_SCOPE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),
        (status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn list<D: Services>(State(services): State<Arc<D>>) -> Result<Response, Refusal> {
    #[derive(serde::Serialize)]
    struct List {
        entries: Vec<afd_authoring::types::PlatformLibrary>,
    }
    let entries = services
        .authoring_setup()
        .platform_libraries()
        .await
        .map_err(authoring_refusal)?;
    Ok(Json(List { entries }).into_response())
}

#[cfg_attr(feature="openapi", utoipa::path(
    patch,path="/v1/admin/fleet-libraries/{id}",operation_id="update_platform_fleet_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Curate, publish or withdraw native platform setup",request_body=Patch,
    params(afd_http::openapi::path::PlatformLibrary,afd_http::openapi::header::IfMatch),
    responses((status=200,body=afd_api_wire::admin_library_native::Item,description="Current platform metadata",headers(("ETag"=String))),
        (status=400,description="Invalid patch"),(status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=PLATFORM_SCOPE),
        (status=404,description="Platform entry unavailable"),(status=412,description=afd_http::openapi::PRECONDITION_FAILED),(status=428,description=afd_http::openapi::PRECONDITION_REQUIRED),
        (status=413,description=afd_http::openapi::PAYLOAD_TOO_LARGE),(status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: Patch = input::body(&body)?;
    let request = PlatformPatch {
        name: request.name,
        description: request.description,
        required_credentials_reasons: request.required_credentials_reasons,
        published: request.published,
    };
    let result = services
        .authoring_setup()
        .curate_platform_library(&id, input::expected(&headers)?, request)
        .await
        .map_err(authoring_refusal)?;
    Ok(([(header::ETAG, result.card.etag.clone())], Json(result)).into_response())
}

#[cfg_attr(feature="openapi", utoipa::path(
    delete,path="/v1/admin/fleet-libraries/{id}",operation_id="delete_platform_fleet_library",tag=afd_http::openapi::tag::FLEET_LIBRARY,
    summary="Delete a withdrawn native platform entry",
    params(afd_http::openapi::path::PlatformLibrary,afd_http::openapi::header::IfMatch),
    responses((status=204,description="Entry hidden; installed revision pins retained"),(status=400,description=afd_http::openapi::BAD_REQUEST),
        (status=401,description=afd_http::openapi::UNAUTHORIZED),(status=403,description=PLATFORM_SCOPE),(status=404,description="Entry unavailable"),
        (status=409,description="Withdraw before deleting"),(status=412,description=afd_http::openapi::PRECONDITION_FAILED),(status=428,description=afd_http::openapi::PRECONDITION_REQUIRED),
        (status=429,description=afd_http::openapi::TOO_MANY_REQUESTS),(status=500,description=afd_http::openapi::INTERNAL),(status=503,description=afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    services
        .authoring_setup()
        .delete_platform_library(&id, input::expected(&headers)?)
        .await
        .map_err(authoring_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
