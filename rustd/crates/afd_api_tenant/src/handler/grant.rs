//! A fleet's integration grants over HTTP: read them, take one back.
//!
//! The port of `integration_grants/workspace.zig` — `innerListGrants` and
//! `innerRevokeGrant`, which are the only two verbs that surface ever served.
//! There is no create here and there never was one on this path: a grant is
//! seeded by the INSTALL (`fleets/create_grants.zig`) at the moment the fleet's
//! declared credentials become knowable, and the retired external request route
//! is pinned as retired by `grant_surface_integration_test.zig`.
//!
//! # Reading a grant and revoking one are separate capabilities
//!
//! `grant:read` reaches the list; `grant:write` is what the revoke demands. The
//! route table says so and this module never re-decides it. Seeing which third
//! parties a fleet may reach is not the authority to cut one off — a dashboard
//! showing the row to a whole team should not thereby let any of them break the
//! fleet's next run.
//!
//! # Two 404s, and they are not the same 404
//!
//! A fleet this workspace does not hold answers `Fleet not found`; a grant that
//! fleet does not hold answers `Grant not found or already revoked` under its
//! own registry code. The store keeps them apart in its return type rather than
//! collapsing both into an empty answer, because an operator's remedy differs:
//! one is the wrong path, the other is a list that has moved on.

use std::borrow::Cow;
use std::sync::Arc;

use afd_approval::{GrantRow, Revocation};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::grant::{GrantSummary, GrantsResponse};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::handler::fleet::detail::{DETAIL_FLEET_ID, FleetPath};
use crate::services::{FleetGrants as _, Services};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "grant_list_failed";
const EVENT_REVOKE: &str = "grant_revoke_failed";

/// The refusal a fleet outside this workspace earns.
///
/// `S_AGENTSFLEET_NOT_FOUND`, kept verbatim. A 404 rather than the ownership
/// layer's 403, and deliberately the same answer for a fleet that never existed
/// and one that is somebody else's — a caller probing identifiers must not
/// learn which fleets are real elsewhere.
const DETAIL_FLEET_NOT_FOUND: &str = "Fleet not found";

/// The refusal a revoke that changed nothing earns.
///
/// `Grant not found or already revoked`, kept verbatim, and it says both on
/// purpose: the grant is unusable either way, so the sentence must not promise
/// to tell them apart.
const DETAIL_GRANT_NOT_FOUND: &str = "Grant not found or already revoked";

/// The two segments beyond the workspace that the item template carries.
///
/// A named struct rather than `Path<(String, String)>`: the template carries
/// THREE parameters, and a tuple extractor would take them positionally — which
/// binds correctly only for as long as nobody reorders the path.
#[derive(Debug, Deserialize)]
pub(crate) struct GrantPath {
    /// The fleet named in the path, still text.
    pub fleet_id: String,
    /// The grant named in the path, still text.
    pub grant_id: String,
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants`.
///
/// Every grant the fleet holds, whatever its status — including the `pending`
/// and `revoked` rows the runner plane is blind to. That breadth is the point
/// of the surface: an operator is being shown what a person has and has not
/// answered, which is exactly the distinction a mint must not be able to make.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants",
    tag = afd_http::openapi::tag::INTEGRATION_GRANTS,
    operation_id = "list_integration_grants",
    summary = "List integration grants for a fleet",
    description = "Returns pending, approved, and revoked grants, newest first. ",
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = GrantsResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&fleet_id)?;
    let held = services
        .grants()
        .page(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_LIST))?
        .ok_or_else(fleet_not_found)?;

    Ok(Json(GrantsResponse {
        items: held.iter().map(summary).collect(),
        total: held.len(),
    })
    .into_response())
}

/// `DELETE /v1/workspaces/{ws}/fleets/{fleet_id}/integration-grants/{grant_id}`.
///
/// A 204 and no body, matching `hx.noContent()`. What the revoke leaves behind
/// is a row, not an absence — the grant keeps its `approved_at`, so the history
/// still records that somebody once said yes.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants/{grant_id}",
    tag = afd_http::openapi::tag::INTEGRATION_GRANTS,
    operation_id = "revoke_integration_grant",
    summary = "Revoke an integration grant",
    description = concat!(
        "Immediately revokes a grant. A revoked grant blocks the fleet on its ",
        "next call against the affected service. ",
    ),
    params(
        afd_http::openapi::path::Grant,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(GrantPath { fleet_id, grant_id }): Path<GrantPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&fleet_id)?;
    let grant = parse_grant(&grant_id)?;

    let outcome = services
        .grants()
        .revoke(&owned.workspace, &fleet, &grant, services.now())
        .await
        .map_err(Refusal::at(EVENT_REVOKE))?;

    match outcome {
        Revocation::Revoked => Ok(StatusCode::NO_CONTENT.into_response()),
        Revocation::GrantAbsent => Err(grant_not_found()),
        Revocation::FleetAbsent => Err(fleet_not_found()),
    }
}

/// The fleet named in the path, refused before a connection is drawn.
///
/// The sibling detail route's sentence and status, reused rather than restated:
/// `GET .../fleets/{fleet_id}` already answers a 400 for a segment that is not
/// an identifier, and one path shape answering two different ways depending on
/// what follows it is a difference no client could act on.
///
/// A divergence from the Zig, which reaches the `::uuid` cast and answers 404.
/// Refusing here is what keeps that cast from ever being the thing that fails,
/// leaving every error from below a genuine datastore fault.
fn parse_fleet(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_not_an_identifier| Refusal::malformed(DETAIL_FLEET_ID))
}

/// The grant named in the path, or the refusal for a row that cannot exist.
///
/// Reads as ABSENT rather than as malformed, unlike the fleet above, and the
/// asymmetry follows the approval detail's: a caller probing grant identifiers
/// learns nothing from the difference between a well-formed id this fleet does
/// not hold and a string that could never be one. The fleet segment is not in
/// that position — it addresses a resource the caller was already shown.
fn parse_grant(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_not_an_identifier| grant_not_found())
}

/// The refusal for a fleet this workspace does not hold.
fn fleet_not_found() -> Refusal {
    Refusal::coded(error_code::AGENTSFLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND)
}

/// The refusal for a grant that is gone, or was never there.
fn grant_not_found() -> Refusal {
    Refusal::coded(error_code::GRANT_REVOKE_NOT_FOUND, DETAIL_GRANT_NOT_FOUND)
}

/// One stored grant, as the wire shows it.
fn summary(held: &GrantRow) -> GrantSummary<'_> {
    GrantSummary {
        id: Cow::Borrowed(&held.id),
        service: Cow::Borrowed(&held.service),
        status: Cow::Borrowed(&held.status),
        created_at: held.created_at,
        approved_at: held.approved_at,
        revoked_at: held.revoked_at,
        reason: Cow::Borrowed(&held.reason),
    }
}
