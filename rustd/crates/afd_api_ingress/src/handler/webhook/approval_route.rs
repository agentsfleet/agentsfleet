//! `POST /v1/webhooks/{fleet_id}/approval` — an approver's answer, arriving.
//!
//! The port of `webhooks/approval.zig`. A person pressed approve or deny on a
//! Slack message and Slack posted the interactive payload here; this resolves
//! the gate the fleet is parked on.
//!
//! # This route writes a ROW where every other one in this family writes a
//! stream entry
//!
//! The rest of the signed-ingress family appends an event and lets a runner
//! decide what it means. An approval is already a decision — there is nothing
//! for a fleet to reason over — so it goes straight to the approvals service,
//! which is the same door the dashboard and the sweeper use. One door is what
//! makes the decision atomic across all three: a dashboard click racing this
//! callback sees the gate already resolved rather than writing a second answer.
//!
//! # The fleet in the URL is a FILTER, not a lookup
//!
//! `approval.zig` binds it into the resolving statement's `WHERE`, so a payload
//! naming a gate that belongs to another fleet resolves nothing rather than
//! resolving someone else's gate. The same binding happens here, and it is the
//! reason the path carries a fleet id at all for a body that already names its
//! own action.

use std::sync::Arc;

use afd_approval::{Decision, Resolution};
use afd_core::error_code;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};
use serde::Deserialize;

use crate::handler::{Refusal, webhook};
use crate::services::{Services, WorkspaceApprovals as _};
use afd_http::handler::{FleetPath, parse_fleet_id};

use super::verify_platform::verified_approval;
/// What a resolved gate is answered with. Deliberately not
/// `afd_wire::approval::ResolvedResponse` — that is the dashboard's shape, and a
/// callback sender is owed a different one.
use afd_wire::ingress::Resolved;

/// The scoped event a failed resolution is logged under.
const EVENT_RESOLVE: &str = "approval_webhook_resolve_failed";

/// What an approver's answer is recorded as having come from.
///
/// `approval_gate_resolver.zig`'s `SLACK_WEBHOOK`, kept byte-for-byte: the
/// audit column is read by operators and by the dashboard, and a value that
/// changed during a cutover would split one gate's history across two spellings.
const BY_SLACK_WEBHOOK: &str = "slack:webhook";

/// The answer meaning the gate may proceed.
const DECISION_APPROVE: &str = "approve";

/// The answer meaning it may not.
const DECISION_DENY: &str = "deny";

/// What a resolved gate is answered with.
const STATUS_RESOLVED: &str = "resolved";

/// The refusal a body this route cannot read as an answer earns.
const DETAIL_INVALID_BODY: &str =
    "Approval payload could not be parsed. Expected an action and a decision.";

/// The refusal a body naming an answer that is not one earns.
const DETAIL_INVALID_DECISION: &str = "The decision must be approve or deny.";

/// The refusal a payload naming no gate of this fleet's earns.
const DETAIL_NOT_FOUND: &str = "No pending approval matches this action for this fleet.";

/// The payload Slack posts, as `approval.zig` reads it.
///
/// Unknown fields are ignored rather than refused, which is the port's rule and
/// not laxity: Slack adds fields to interactive payloads without notice, and a
/// daemon that refused an unrecognised one would go down on a vendor's release
/// note.
#[derive(Debug, Deserialize)]
struct Answer {
    /// The gate this answers.
    action_id: String,
    /// `approve` or `deny`, and nothing else.
    decision: String,
}

/// `POST /v1/webhooks/{fleet_id}/approval`.
///
/// # Errors
/// The wall's refusals, `UZ-WH-002` for a body this route cannot read as an
/// answer, and `UZ-APPROVAL-002` for a payload naming no gate of this fleet's.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/webhooks/{fleet_id}/approval",
    tag = afd_http::openapi::tag::WEBHOOKS,
    operation_id = "approval_webhook",
    summary = "Resolve a fleet approval gate",
    description = concat!(
        "Called by a human (via email link or Slack action) to approve or ",
        "reject a paused fleet. Body is HMAC-signed by the issuer; the ",
        "signature is verified against the fleet's webhook secret. ",
    ),
    params(
        afd_http::openapi::path::FleetOnly,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    webhook::within_cap(&body)?;

    // Nothing above this line has read the body as anything but bytes.
    let proven = verified_approval(&services, &headers, body).await?;

    let answer: Answer = serde_json::from_slice(&proven.body).map_err(|_unreadable| {
        Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_INVALID_BODY)
    })?;

    if answer.action_id.is_empty() {
        return Err(Refusal::coded(
            error_code::WEBHOOK_MALFORMED,
            DETAIL_INVALID_BODY,
        ));
    }

    let outcome = match answer.decision.as_str() {
        DECISION_APPROVE => Decision::Approved,
        DECISION_DENY => Decision::Denied,
        // `TimedOut` is deliberately unreachable from here: expiring a gate is
        // the sweeper's, and a sender that could spell it would be writing a
        // decision no person made.
        _unknown => {
            return Err(Refusal::coded(
                error_code::WEBHOOK_MALFORMED,
                DETAIL_INVALID_DECISION,
            ));
        }
    };

    let resolution = services
        .approvals()
        .resolve(
            &answer.action_id,
            outcome,
            BY_SLACK_WEBHOOK,
            webhook::REASON_APPROVAL_WEBHOOK,
            Some(fleet.as_str()),
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_RESOLVE))?;

    // A gate somebody already answered is answered 200 rather than 409 here,
    // where the dashboard's own route answers 409. Slack retries a non-2xx and
    // retrying cannot change an already-resolved gate, so a conflict status
    // would buy a retry storm and no new information — which is `approval.zig`'s
    // own choice: it logs `already_resolved` and answers `resolved` either way.
    match resolution {
        Resolution::Resolved(_) | Resolution::AlreadyResolved(_) => {}
        Resolution::NotFound => {
            return Err(Refusal::coded(
                error_code::APPROVAL_NOT_FOUND,
                DETAIL_NOT_FOUND,
            ));
        }
    }

    Ok((
        StatusCode::OK,
        Json(Resolved {
            status: STATUS_RESOLVED.into(),
            action_id: answer.action_id.as_str().into(),
            decision: answer.decision.as_str().into(),
        }),
    )
        .into_response())
}
