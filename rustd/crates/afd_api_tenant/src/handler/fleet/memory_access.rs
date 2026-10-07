//! `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memory-access` — who
//! reads and publishes the workspace's shared memory.
//!
//! Sharing is a grant a workspace admin gives a fleet, never the default. The
//! route takes `fleet:write`, because a grant is an edit to the fleet rather
//! than a lifecycle transition, and ownership of the fleet is the route's own,
//! derived from its template. Presence-based: a grant the body leaves out keeps
//! its value, so the same body sent twice leaves the same row and answers the
//! same reply.

use std::sync::Arc;

use afd_api_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{FleetMemories as _, Services};

use super::detail::{FleetPath, parse_fleet_id};

/// The scoped event a refused change is logged under.
const EVENT_ACCESS: &str = "memory_access_failed";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed memory access body";

/// Sets the fleet's shared-memory grants.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memory-access",
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "patch_fleet_memory_access",
    summary = "Set who reads and publishes shared memory",
    description = concat!(
        "Sets the fleet's two shared-memory grants. `read` lets the fleet ",
        "see entries other fleets in this workspace published, each naming ",
        "its writer. `publish` lets the fleet store entries the workspace ",
        "reads. Both start false. Omit a field to keep its value. The reply ",
        "is both grants as they now stand, and the same body sent twice ",
        "gives the same reply. ",
    ),
    request_body = MemoryAccessRequest,
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = MemoryAccess),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn set<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let change = afd_http::handler::read_strict_body::<MemoryAccessRequest>(&body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let access: MemoryAccess = services
        .memories()
        .set_access(&owned.workspace, &fleet, change)
        .await
        .map_err(Refusal::at(EVENT_ACCESS))?;
    Ok(Json(access).into_response())
}
