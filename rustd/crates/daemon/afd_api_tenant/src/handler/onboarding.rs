//! The workspace's onboarding checklist over HTTP.
//!
//! One verb, and it answers with the whole checklist: five signals derived
//! from what the workspace holds, read in one round trip.

use std::sync::Arc;

use afd_api_wire::onboarding::OnboardingResponse;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{TenantPlane, WorkspaceOnboarding as _};

/// The scoped event a failed read is logged under.
const EVENT_ONBOARDING: &str = "onboarding_read_failed";

/// `GET /v1/workspaces/{workspace_id}/onboarding` — the whole checklist.
///
/// Five derivable signals from one round trip. One HTTP call, one
/// authorization, where the dashboard used to make several.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/onboarding",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "get_workspace_onboarding",
    summary = "Read the workspace's onboarding checklist state",
    description = concat!(
        "Returns Getting Started progress: model configured, fleet created, credential ",
        "stored, event processed, and message sent.",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = OnboardingResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn read<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
) -> Result<Response, Refusal> {
    let signals = services
        .onboarding()
        .signals(&owned.workspace, &owned.tenant)
        .await
        .map_err(Refusal::at(EVENT_ONBOARDING))?;

    Ok(Json(OnboardingResponse {
        model_configured: signals.model_configured,
        has_fleet: signals.has_fleet,
        has_secret: signals.has_secret,
        has_processed_event: signals.has_processed_event,
        has_steer_event: signals.has_steer_event,
    })
    .into_response())
}
