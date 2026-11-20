//! An idle draft's edit applies whole against its strong version, or not at all.

use super::{DraftPath, access, attach, id, input, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::TenantPlane,
};
#[cfg(feature = "openapi")]
use afd_api_wire::setup::{Draft, DraftPatchRequest};
use afd_http::handler::{Refusal, setup_refusal};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::HeaderMap;
use std::sync::Arc;

/// Applies one complete idle edit using the current strong version.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/v1/workspaces/{workspace_id}/drafts/{draft_id}", operation_id = "patch_workspace_draft",
    tag = afd_http::openapi::tag::DRAFTS, summary = "Edit an idle private setup chat",
    params(afd_http::openapi::path::Draft, afd_http::openapi::header::IfMatch), request_body = DraftPatchRequest,
    responses((status = 200, body = Draft, description = "Updated private draft", headers(("ETag" = String))),
        (status = 400, description = "Invalid edit"), (status = 404, description = super::DRAFT_UNAVAILABLE),
        (status = 409, description = "Draft busy or selection unavailable"), (status = 412, description = afd_http::openapi::PRECONDITION_FAILED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE), (status = 428, description = afd_http::openapi::PRECONDITION_REQUIRED),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn patch<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(path): Path<DraftPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let draft = services
        .drafts()
        .patch(
            access(&owned, &person),
            &id(&path)?,
            input::expected(&headers)?,
            input::patch(&body)?,
            services.now(),
        )
        .await
        .map_err(setup_refusal)?;
    let reviewed = services
        .setup()
        .read_draft(access(&owned, &person), &draft.id)
        .await
        .map_err(setup_refusal)?;
    let tag = reviewed.etag.clone();
    let mut response = Json(render::draft(reviewed)).into_response();
    attach(&mut response, &tag);
    Ok(response)
}
