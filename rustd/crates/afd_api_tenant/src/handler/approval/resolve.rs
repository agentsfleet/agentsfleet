//! Answering one gate: the decision verb, and what it reads off the request.
//!
//! Split from [`super`] because it is the half that WRITES. The two reads
//! beside it render rows; this one takes a person's answer, and carries the
//! rules that answer has to meet — which decision the path spells, how long a
//! note may be, and what the second answerer is told.

use garde::Validate as _;
use std::borrow::Cow;
use std::sync::Arc;

use afd_approval::{Decision, Resolution};
use afd_core::error_code;
use afd_http::envelope;
use afd_wire::approval::{ResolveApprovalRequest, ResolvedResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use super::{DETAIL_ALREADY_RESOLVED, DETAIL_NOT_FOUND, EVENT_RESOLVE, ResolvePath, read_gate};
use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceApprovals as _};

/// The refusal a decision the path does not spell earns.
const DETAIL_UNKNOWN_DECISION: &str = "decision must be approve or deny";

/// The refusal a note past the column's working bound earns.
const DETAIL_REASON_TOO_LONG: &str = "reason exceeds max length";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// `POST /v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}`.
///
/// Two segments where the Zig daemon spelled one — see the route table on why
/// the decision moved out of the gate id's segment.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}",
    tag = afd_http::openapi::tag::APPROVALS,
    operation_id = "approve_workspace_approval",
    summary = "Approve a pending request",
    description = concat!(
        "Approves a pending request. If two callers resolve it, the first ",
        "result wins. Other callers receive 409 with the existing result and ",
        "resolver. ",
    ),
    request_body = ResolveApprovalRequest,
    params(
        afd_http::openapi::path::GateDecision,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ResolvedResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn resolve<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(ResolvePath { gate_id, decision }): Path<ResolvePath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let outcome = parse_decision(&decision)?;
    let reason = read_reason(&body)?;

    // The read is the workspace scoping AND the source of the action id the
    // decision keys on. A resolve that took the gate id straight from the path
    // would answer without ever proving the gate is this workspace's.
    let gate = read_gate(&services, &owned.workspace, &gate_id, EVENT_RESOLVE).await?;

    let resolution = services
        .approvals()
        .resolve(
            &gate.action_id,
            outcome,
            person.subject(),
            reason.as_ref(),
            Some(&gate.fleet_id),
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_RESOLVE))?;

    match resolution {
        Resolution::Resolved(row) => Ok(Json(ResolvedResponse {
            gate_id: Cow::Owned(row.gate_id),
            action_id: Cow::Owned(row.action_id),
            outcome: Cow::Owned(row.status),
            resolved_at: row.updated_at,
            resolved_by: Cow::Owned(row.resolved_by),
        })
        .into_response()),
        // A 409 carrying the standing answer, not merely the fact of one.
        // `current_state` tells a client to stop retrying; the attribution
        // beside it is what the dashboard renders and what
        // `approvals/resolve.zig` has always sent. The resolver is not
        // interpolated into the SENTENCE — a subject is an entity value, and
        // the detail rules keep those out of `detail` — so it rides the
        // envelope as an extension instead.
        Resolution::AlreadyResolved(row) => Err(Refusal::already_resolved(
            error_code::APPROVAL_ALREADY_RESOLVED,
            DETAIL_ALREADY_RESOLVED,
            envelope::Resolution {
                gate_id: row.gate_id,
                action_id: row.action_id,
                outcome: row.status,
                resolved_at: row.updated_at,
                resolved_by: row.resolved_by,
            },
        )),
        Resolution::NotFound => Err(Refusal::coded(
            error_code::APPROVAL_NOT_FOUND,
            DETAIL_NOT_FOUND,
        )),
    }
}
/// The status the path's verb resolves to.
fn parse_decision(decision: &str) -> Result<Decision, Refusal> {
    match decision {
        "approve" => Ok(Decision::Approved),
        "deny" => Ok(Decision::Denied),
        _unknown => Err(Refusal::malformed(DETAIL_UNKNOWN_DECISION)),
    }
}

/// The operator's note, or an empty one.
///
/// An absent body and an absent `reason` are the same answer: a decision is
/// complete without a note, and demanding one would make the common case the
/// awkward one.
fn read_reason(body: &Bytes) -> Result<Cow<'_, str>, Refusal> {
    if body.is_empty() {
        return Ok(Cow::Borrowed(""));
    }
    let request: ResolveApprovalRequest<'_> = afd_core::json::object_from_slice(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    // Plain JSON strings borrow from `body`; escaped strings necessarily own
    // their decoded value. Carrying the `Cow` through the await accepts both
    // without cloning either one.
    request
        .validate()
        .map_err(|_report| Refusal::malformed(DETAIL_REASON_TOO_LONG))?;
    Ok(request.reason.unwrap_or(Cow::Borrowed("")))
}
