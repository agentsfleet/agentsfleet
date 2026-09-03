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

use afd_approval::{Filter, GateRow};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::paging::Cursor as CoreCursor;
use afd_wire::approval::{ApprovalSummary, ApprovalsResponse};
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::value::RawValue;

use self::query::{Listing, Resume};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceApprovals as _};

mod query;
pub(crate) mod resolve;

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "approval_list_failed";
const EVENT_DETAIL: &str = "approval_detail_failed";
pub(super) const EVENT_RESOLVE: &str = "approval_resolve_failed";

/// The refusal a gate this workspace does not hold earns.
///
/// `MSG_APPROVAL_NOT_FOUND`, kept verbatim, and it says "or already resolved"
/// on purpose: the two are one answer to a caller, so the sentence must not
/// promise to tell them apart.
pub(super) const DETAIL_NOT_FOUND: &str = "Approval action not found or already resolved";

/// The refusal a gate somebody already answered earns.
///
/// It names the channels rather than the person: which of them answered is in
/// `current_state` and on the gate itself, and a subject spelled here would be
/// an entity value in a sentence the detail rules keep clear of them.
pub(super) const DETAIL_ALREADY_RESOLVED: &str =
    "Approval gate already resolved by another channel";

/// How long an operator's note may be.
///
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
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let asked = Listing::parse(&query.unwrap_or_default())?;
    let resume = asked.cursor.as_ref().map(Resume::borrowed);
    let gates = services
        .approvals()
        .page(
            &owned.workspace,
            Filter {
                status: asked.status,
                fleet_id: asked.fleet_id.as_deref(),
                gate_kind: asked.gate_kind.as_deref(),
            },
            resume,
            asked.limit,
        )
        .await
        .map_err(Refusal::at(EVENT_LIST))?;

    Ok(Json(ApprovalsResponse {
        items: gates.iter().map(summary).collect(),
        next_cursor: next_cursor(&gates, asked.limit).map(Cow::Owned),
    })
    .into_response())
}

/// Where the next page resumes, or nothing on the last one.
///
/// A short page is the last page, so it hands back no cursor: issuing one would
/// send a client back for rows the store already said were not there. A FULL
/// page is not proof that more exist — the boundary case answers an empty page
/// next — and that is the same trade `approvals/list.zig` makes, deliberately,
/// because the alternative is reading one row further on every request.
fn next_cursor(gates: &[GateRow], limit: i64) -> Option<String> {
    let last = gates.last()?;
    (i64::try_from(gates.len()).is_ok_and(|read| read == limit)).then(|| {
        CoreCursor::Timestamp {
            at_ms: last.created_at,
            id: last.gate_id.clone(),
        }
        .to_string()
    })
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

/// One gate inside this workspace, or the refusal for none.
pub(super) async fn read_gate<D: Services>(
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
