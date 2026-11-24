//! A fleet's integration grants over HTTP: read them, take one back.
//!
//! Two verbs, list and revoke. There is no create on this path: a grant is
//! requested by the fleet's own run with `request_access`, and a person
//! approves it once on the card that request raises, so no caller of this
//! surface ever names a grant into being. Only a signed-in person revokes one.
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

use af_core::error_code;
use af_core::id::Uuid7;
use af_observability::{AccessChange, FleetStep, Telemetry};
use afd_api_wire::grant::{Grant, GrantsResponse, kind};
use afd_approval::{GrantRow, Revocation};
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use crate::auth::{SignedInPerson, WorkspaceContext};
use crate::handler::Refusal;
use crate::handler::fleet::detail::{FleetPath, parse_fleet_id};
use crate::services::{FleetGrants as _, TenantPlane};

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
        (status = 200, description = afd_http::openapi::OK, body = GrantsResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let held = services
        .grants()
        .page(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_LIST))?
        .ok_or_else(fleet_not_found)?;

    Ok(Json(GrantsResponse {
        items: held.iter().map(shown).collect(),
    })
    .into_response())
}

/// `DELETE /v1/workspaces/{ws}/fleets/{fleet_id}/integration-grants/{grant_id}`.
///
/// A 204 and no body. What the revoke leaves behind is a row, not an absence —
/// the grant keeps its `approved_at`, so the history still records that
/// somebody once said yes. Only a signed-in person revokes: an API key earns
/// 403 and nothing changes.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants/{grant_id}",
    tag = afd_http::openapi::tag::INTEGRATION_GRANTS,
    operation_id = "revoke_integration_grant",
    summary = "Revoke an integration grant",
    description = concat!(
        "Revokes a grant. A live run keeps what it was issued; the fleet's next run and ",
        "its next token mint do not hold it. Requires a signed-in person; an API key ",
        "receives 403.",
    ),
    params(
        afd_http::openapi::path::Grant,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 403, description = "An API key revoked a grant (`UZ-AUTH-001`)"),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn revoke<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: SignedInPerson,
    Path(GrantPath { fleet_id, grant_id }): Path<GrantPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let grant = parse_grant(&grant_id)?;

    let outcome = services
        .grants()
        .revoke(&owned.workspace, &fleet, &grant)
        .await
        .map_err(Refusal::at(EVENT_REVOKE))?;

    match outcome {
        Revocation::Revoked => {
            report_revoked(services.as_ref(), &owned.workspace, &fleet, &grant, &person).await;
            Ok(StatusCode::NO_CONTENT.into_response())
        }
        Revocation::GrantAbsent => Err(grant_not_found()),
        Revocation::FleetAbsent => Err(fleet_not_found()),
    }
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

/// Reports a revoke after its commit, with the kind the revoked row holds.
///
/// The kind is read back rather than carried by the revoke, and a read that
/// fails reports nothing: the revoke already stands and is the caller's
/// answer, and an analytics gap is not worth refusing it over.
async fn report_revoked<D: TenantPlane>(
    services: &D,
    workspace: &Uuid7,
    fleet: &Uuid7,
    grant: &Uuid7,
    person: &SignedInPerson,
) {
    let held = services.grants().page(workspace, fleet).await;
    let kind = held
        .ok()
        .flatten()
        .and_then(|rows| rows.into_iter().find(|row| row.id == grant.as_str()))
        .and_then(|row| stored_kind(&row.kind));
    if let Some(kind) = kind {
        services.analytics().report(&Telemetry::Fleet {
            actor: Some(person.subject().to_owned()),
            step: FleetStep::AccessChanged {
                fleet_id: fleet.clone(),
                kind,
                change: AccessChange::Revoked,
            },
        });
    }
}

/// The closed spelling a stored grant kind names, so an event never carries
/// a stored string; `None` for a spelling this build does not write.
pub(crate) fn stored_kind(stored: &str) -> Option<&'static str> {
    [kind::SECRET, kind::INTEGRATION]
        .into_iter()
        .find(|known| *known == stored)
}

/// One stored grant, as the wire shows it.
///
/// The scopes are stored `JSONB` rendered as text, so they always parse; one
/// that did not would show as `null` rather than fail the fleet's read.
pub(crate) fn shown(held: &GrantRow) -> Grant<'_> {
    let scope = |text: &Option<String>| serde_json::from_str(text.as_deref()?).ok();
    Grant {
        id: Cow::Borrowed(&held.id),
        kind: Cow::Borrowed(&held.kind),
        name: Cow::Borrowed(&held.service),
        status: Cow::Borrowed(&held.status),
        scope: scope(&held.scope),
        requested_scope: scope(&held.requested_scope),
        approved_at: held.approved_at,
    }
}
