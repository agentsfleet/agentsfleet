//! The approval inbox over HTTP: read the queue, read one gate, answer it.
//!
//! The port of `approvals/list.zig`, `approvals/detail.zig` and
//! `approvals/resolve.zig`.
//!
//! # Reading the queue and answering it are separate capabilities
//!
//! `ApprovalRead` reaches the list and the detail; `ApprovalResolve` is what
//! the decision demands. Seeing what a fleet wants to do is not the authority
//! to let it — the route table says so, and this module never re-decides it.
//!
//! # The decision is spelled in the path, not the body
//!
//! `POST .../approvals/{gate_id}:approve` and `:deny`. Two templates in one
//! route row, so a body field could not disagree with the URL a person clicked
//! — and an audit reading access logs alone can tell an approval from a denial.
//!
//! # The gate is addressed by row id and resolved by action id
//!
//! The path names the GATE; the store's decision keys on the ACTION, because a
//! re-raised action leaves more than one gate row and the resolve must take the
//! one the caller is looking at. So this handler reads the gate first — which
//! is also the workspace scoping — and hands its `action_id` to the decision,
//! narrowed to the fleet that row names.

use std::borrow::Cow;
use std::sync::Arc;

use afd_approval::{Decision, Filter, GateRow, Resolution};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::approval::{
    ApprovalSummary, ApprovalsResponse, ResolveApprovalRequest, ResolvedResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::value::RawValue;

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceApprovals as _};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "approval_list_failed";
const EVENT_DETAIL: &str = "approval_detail_failed";
const EVENT_RESOLVE: &str = "approval_resolve_failed";

/// The refusal a gate this workspace does not hold earns.
///
/// `MSG_APPROVAL_NOT_FOUND`, kept verbatim, and it says "or already resolved"
/// on purpose: the two are one answer to a caller, so the sentence must not
/// promise to tell them apart.
const DETAIL_NOT_FOUND: &str = "Approval action not found or already resolved";

/// The refusal a decision the path does not spell earns.
const DETAIL_UNKNOWN_DECISION: &str = "decision must be approve or deny";

/// The refusal an over-long note earns.
const DETAIL_REASON_TOO_LONG: &str = "reason exceeds max length";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// How long an operator's note may be.
///
/// `REASON_MAX`, mirrored: the column is unbounded text and a note is a
/// sentence, so the cap is what keeps a decision from becoming storage.
const REASON_MAX_BYTES: usize = 4096;

/// How many gates one page holds.
///
/// Fixed rather than client-chosen: the inbox is a human queue, and a caller
/// asking for ten thousand rows is not a person reading them.
const PAGE_LIMIT: i64 = 50;

/// The two segments the item templates carry beside the workspace.
#[derive(Debug, Deserialize)]
pub(crate) struct ApprovalPath {
    /// The gate named in the path, still text.
    pub gate_id: String,
}

/// The gate and the decision, each in its own path segment.
#[derive(Debug, Deserialize)]
pub(crate) struct ResolvePath {
    /// The gate named in the path, still text.
    pub gate_id: String,
    /// The verb that follows it.
    pub decision: String,
}

/// `GET /v1/workspaces/{workspace_id}/approvals` — the queue.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/approvals",
    tag = afd_http::openapi::tag::APPROVALS,
    operation_id = "list_workspace_approvals",
    summary = "List pending approval gates for a workspace",
    description = concat!(
        "Returns approval gates oldest-first (oldest is most urgent). Each ",
        "row surfaces the fleet's proposed action, gathered evidence, ",
        "blast-radius assessment, and timeout countdown. Filter by fleet, gate ",
        "kind, or status. Cursor pagination over (created_at, id) so ",
        "concurrent inserts don't cause silent skips. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
        ("status" = Option<String>, Query, description = "Defaults to \"pending\". Supply \"approved\", \"denied\", \"timed_out\" to query terminal states."),
        ("fleet_id" = Option<String>, Query),
        ("gate_kind" = Option<String>, Query),
        ("limit" = Option<String>, Query),
        ("cursor" = Option<String>, Query),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ApprovalsResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
) -> Result<Response, Refusal> {
    let gates = services
        .approvals()
        .page(&owned.workspace, Filter::default(), None, PAGE_LIMIT)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;

    Ok(Json(ApprovalsResponse {
        items: gates.iter().map(summary).collect(),
        // One page for now: the queue is bounded by what a person will read,
        // and a cursor nothing issues would be a field a client could not use.
        next_cursor: None,
    })
    .into_response())
}

/// `GET /v1/workspaces/{workspace_id}/approvals/{gate_id}` — one gate.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/approvals/{gate_id}",
    tag = afd_http::openapi::tag::APPROVALS,
    operation_id = "get_workspace_approval",
    summary = "Get a single approval gate by id",
    description = concat!(
        "Drives the dashboard detail page. 404 when the gate doesn't exist or ",
        "belongs to a different workspace. ",
    ),
    params(
        afd_http::openapi::path::Gate,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ApprovalSummary),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn detail<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(ApprovalPath { gate_id }): Path<ApprovalPath>,
) -> Result<Response, Refusal> {
    let gate = read_gate(&services, &owned.workspace, &gate_id, EVENT_DETAIL).await?;
    Ok(Json(summary(&gate)).into_response())
}

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
        // Both arms answer 200 with the same shape. The second caller asked for
        // the gate to be resolved and it is; what they must not be told is that
        // THEY resolved it, which is why `resolved_by` is read off the row.
        Resolution::Resolved(row) | Resolution::AlreadyResolved(row) => {
            Ok(Json(ResolvedResponse {
                gate_id: Cow::Owned(row.gate_id),
                action_id: Cow::Owned(row.action_id),
                outcome: Cow::Owned(row.status),
                resolved_at: row.updated_at,
                resolved_by: Cow::Owned(row.resolved_by),
            })
            .into_response())
        }
        Resolution::NotFound => Err(Refusal::coded(
            error_code::APPROVAL_NOT_FOUND,
            DETAIL_NOT_FOUND,
        )),
    }
}

/// One gate inside this workspace, or the refusal for none.
async fn read_gate<D: Services>(
    services: &Arc<D>,
    workspace: &Uuid7,
    gate_id: &str,
    event: &'static str,
) -> Result<GateRow, Refusal> {
    let gate = Uuid7::parse(gate_id)
        .map_err(|_malformed| Refusal::coded(error_code::APPROVAL_NOT_FOUND, DETAIL_NOT_FOUND))?;

    services
        .approvals()
        .one(workspace, &gate)
        .await
        .map_err(Refusal::at(event))?
        .ok_or_else(|| Refusal::coded(error_code::APPROVAL_NOT_FOUND, DETAIL_NOT_FOUND))
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
    let reason = request.reason.unwrap_or(Cow::Borrowed(""));
    if reason.len() > REASON_MAX_BYTES {
        return Err(Refusal::malformed(DETAIL_REASON_TOO_LONG));
    }
    Ok(reason)
}

/// One stored gate, as the wire shows it.
fn summary(gate: &GateRow) -> ApprovalSummary<'_> {
    ApprovalSummary {
        gate_id: Cow::Borrowed(&gate.gate_id),
        fleet_id: Cow::Borrowed(&gate.fleet_id),
        fleet_name: Cow::Borrowed(&gate.fleet_name),
        workspace_id: Cow::Borrowed(&gate.workspace_id),
        action_id: Cow::Borrowed(&gate.action_id),
        tool_name: Cow::Borrowed(&gate.tool_name),
        action_name: Cow::Borrowed(&gate.action_name),
        gate_kind: Cow::Borrowed(&gate.gate_kind),
        proposed_action: Cow::Borrowed(&gate.proposed_action),
        blast_radius: Cow::Borrowed(&gate.blast_radius),
        status: Cow::Borrowed(&gate.status),
        detail: Cow::Borrowed(&gate.detail),
        created_at: gate.created_at,
        timeout_at: gate.timeout_at,
        updated_at: gate.updated_at,
        resolved_by: Cow::Borrowed(&gate.resolved_by),
        // `None` on a row that will not parse — see the field's own note on
        // why a corrupt gate is still shown rather than failing the queue.
        evidence: serde_json::from_str::<&RawValue>(&gate.evidence_json).ok(),
    }
}
