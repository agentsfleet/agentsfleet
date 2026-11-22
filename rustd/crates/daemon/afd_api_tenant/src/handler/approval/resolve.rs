//! Answering one gate: the decision verb, and what it reads off the request.
//!
//! Split from [`super`] because it is the half that WRITES. The two reads
//! beside it render rows; this one takes a person's answer, and carries the
//! rules that answer has to meet — which decision the path spells, how long a
//! note may be, and what the second answerer is told.

use garde::Validate as _;
use std::borrow::Cow;
use std::sync::Arc;

use af_core::error_code;
use afd_api_wire::approval::{ResolveApprovalRequest, ResolvedResponse};
use afd_approval::{Decision, Resolution};
use afd_http::envelope;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use super::{DETAIL_ALREADY_RESOLVED, DETAIL_NOT_FOUND, EVENT_RESOLVE, ResolvePath, read_gate};
use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{TenantPlane, WorkspaceApprovals as _};

/// The refusal a decision the path does not spell earns.
const DETAIL_UNKNOWN_DECISION: &str = "decision must be approve, deny or answer";

/// The refusal an answer to a gate that asked no question earns, and an
/// approval of one that did.
const DETAIL_WRONG_DECISION: &str = "a question takes an answer, and only a question does";

/// The refusal an answer decision with no answer earns.
const DETAIL_ANSWER_MISSING: &str = "answer is required";

/// The refusal a gate of an archived fleet earns.
const DETAIL_FLEET_ARCHIVED: &str = "The fleet is archived, so its approvals take no answer";

/// The refusal a note past the column's working bound earns.
const DETAIL_REASON_TOO_LONG: &str = "reason or answer is out of bounds";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// `POST /v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}`.
///
/// The decision is its own segment rather than a suffix on the gate id's — see
/// the route table on why.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}",
    tag = afd_http::openapi::tag::APPROVALS,
    operation_id = "approve_workspace_approval",
    summary = "Approve, reject or answer a pending request",
    description = concat!(
        "Approves or rejects a pending request, or answers a question with ",
        "`answer` and a body of `{\"answer\":\"…\"}`. A request past its ",
        "deadline still takes a decision. If two callers resolve it, the first ",
        "result wins. A repeat of the standing decision receives 200 with the ",
        "same result; any other caller receives 409 with the existing result ",
        "and resolver. A request whose fleet is archived receives 409.",
    ),
    request_body = ResolveApprovalRequest<'_>,
    params(
        afd_http::openapi::path::GateDecision,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ResolvedResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn resolve<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(ResolvePath { gate_id, decision }): Path<ResolvePath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let outcome = parse_decision(&decision)?;
    let detail = read_detail(&body, outcome)?;

    // The read is the workspace scoping AND the source of the action id the
    // decision keys on. A resolve that took the gate id straight from the path
    // would answer without ever proving the gate is this workspace's.
    let gate = read_gate(&services, &owned.workspace, &gate_id, EVENT_RESOLVE).await?;
    if !outcome.fits(&gate.gate_type) {
        return Err(Refusal::malformed(DETAIL_WRONG_DECISION));
    }

    let resolution = services
        .approvals()
        .resolve(
            gate.action_id.as_str(),
            outcome,
            person.subject(),
            detail.as_ref(),
            Some(gate.fleet_id.as_str()),
        )
        .await
        .map_err(Refusal::at(EVENT_RESOLVE))?;

    super::setup::after_resolution(&services, &gate, outcome, &resolution, &person).await;
    answered(resolution, outcome, &detail)
}

/// The response a resolution earns. A repeat of the decision that stands —
/// the same answer, for a question — earns the same 200 the first did.
fn answered(resolution: Resolution, outcome: Decision, detail: &str) -> Result<Response, Refusal> {
    match resolution {
        Resolution::Resolved(row) => Ok(receipt(row)),
        Resolution::AlreadyResolved(row)
            if row.status == outcome.as_str()
                && (outcome != Decision::Answered || row.detail == detail) =>
        {
            Ok(receipt(row))
        }
        // A 409 carrying the standing answer, not merely the fact of one.
        // `current_state` tells a client to stop retrying; the attribution
        // beside it is what the dashboard renders. The resolver is not
        // interpolated into the SENTENCE — a subject is an entity value, and
        // the detail rules keep those out of `detail` — so it rides the
        // envelope as an extension instead.
        Resolution::AlreadyResolved(row) => Err(Refusal::already_resolved(
            error_code::APPROVAL_ALREADY_RESOLVED,
            DETAIL_ALREADY_RESOLVED,
            envelope::Resolution {
                gate_id: row.gate_id.into(),
                action_id: row.action_id.into(),
                outcome: row.status,
                resolved_at: row.resolved_at,
                resolved_by: row.resolved_by,
            },
        )),
        Resolution::FleetArchived => Err(Refusal::coded(
            error_code::AGENTSFLEET_ARCHIVED,
            DETAIL_FLEET_ARCHIVED,
        )),
        Resolution::WrongDecision => Err(Refusal::malformed(DETAIL_WRONG_DECISION)),
        Resolution::NotFound => Err(Refusal::coded(
            error_code::APPROVAL_NOT_FOUND,
            DETAIL_NOT_FOUND,
        )),
    }
}

/// The 200 body a decision that stands earns.
fn receipt(row: afd_approval::Resolved) -> Response {
    Json(ResolvedResponse {
        gate_id: Cow::Owned(row.gate_id.into()),
        action_id: Cow::Owned(row.action_id.into()),
        outcome: Cow::Owned(row.status),
        resolved_at: row.resolved_at,
        resolved_by: Cow::Owned(row.resolved_by),
    })
    .into_response()
}

/// The status the path's verb resolves to.
fn parse_decision(decision: &str) -> Result<Decision, Refusal> {
    match decision {
        "approve" => Ok(Decision::Approved),
        "deny" => Ok(Decision::Denied),
        "answer" => Ok(Decision::Answered),
        _unknown => Err(Refusal::malformed(DETAIL_UNKNOWN_DECISION)),
    }
}

/// What the gate's `detail` records: the person's answer for the `answer`
/// decision, which requires one, and the operator's note, or an empty one,
/// otherwise.
///
/// An absent body and an absent `reason` are the same answer: a decision is
/// complete without a note, and demanding one would make the common case the
/// awkward one.
fn read_detail(body: &Bytes, outcome: Decision) -> Result<Cow<'_, str>, Refusal> {
    if body.is_empty() {
        return if outcome == Decision::Answered {
            Err(Refusal::malformed(DETAIL_ANSWER_MISSING))
        } else {
            Ok(Cow::Borrowed(""))
        };
    }
    let request: ResolveApprovalRequest<'_> = afd_http::handler::read_body(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    // Plain JSON strings borrow from `body`; escaped strings necessarily own
    // their decoded value. Carrying the `Cow` through the await accepts both
    // without cloning either one.
    request
        .validate()
        .map_err(|_report| Refusal::malformed(DETAIL_REASON_TOO_LONG))?;
    if outcome == Decision::Answered {
        return request
            .answer
            .ok_or_else(|| Refusal::malformed(DETAIL_ANSWER_MISSING));
    }
    Ok(request.reason.unwrap_or(Cow::Borrowed("")))
}
