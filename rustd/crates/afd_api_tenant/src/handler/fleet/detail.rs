//! One fleet over HTTP: read it, edit it, purge it.
//!
//! The port of `fleets/get.zig`, `patch.zig` and `delete.zig`. Everything here
//! is addressed by a fleet id as well as a workspace id, which is the line
//! [`super`] splits on and the same line the route table draws.
//!
//! # 403 and 404 are separate axes
//!
//! A foreign WORKSPACE in the path is a 403 from the ownership layer, which
//! never runs a statement on the caller's behalf. A workspace the caller DOES
//! own, naming a fleet they do not, is a 404 — every statement is
//! workspace-scoped in its predicate, so this daemon does not learn whether the
//! fleet exists elsewhere and could not disclose it if asked.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{FleetDetail, Patch};
use afd_wire::fleet::{FleetDetailResponse, PatchedFleetResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, StatusCode, header};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceFleets as _};
pub use afd_http::handler::{DETAIL_FLEET_ID, FleetPath, parse_fleet_id};

// The refusal sentences live beside the parser that produces them and are
// re-exported here because the tests and the siblings address them as
// `detail::DETAIL_*`, the path they were published under.
use super::detail_request::read_patch;
pub use super::detail_request::{
    DETAIL_CONFIG_AMBIGUOUS, DETAIL_CONFIG_REQUIRED, DETAIL_MALFORMED_JSON, DETAIL_SOURCE_BOUNDS,
    DETAIL_STATUS_INVALID, DETAIL_TRIGGER_BOUNDS,
};
use super::triggers;

/// The scoped events each verb's failures are logged under.
const EVENT_READ: &str = "fleet_read_failed";
const EVENT_PATCH: &str = "fleet_patch_failed";
const EVENT_PURGE: &str = "fleet_purge_failed";

/// The refusal a conditional PATCH that asks for nothing earns.
///
/// An `If-Match` with no field to write is a caller expecting a compare that
/// cannot happen — answering 200 would tell them their edit landed.
pub const DETAIL_CONDITIONAL_EMPTY: &str = "A conditional fleet update requires at least one field";

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — one fleet, whole.
///
/// Carries an `ETag` over the editable surface, which the source editor sends
/// back as `If-Match`. A read that could not attach one would leave that editor
/// unable to save safely, so there is no tagless success here.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "get_fleet",
    summary = "Get a fleet",
    description = concat!(
        "Returns one fleet's editable source, trigger markdown, bundle pin, ",
        "trigger list, status, and lifetime counters. The response carries an ",
        "`ETag` header over `source_markdown` and `trigger_markdown`. Send ",
        "that value as `If-Match` when saving source changes to avoid ",
        "overwriting another operator's edit. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = FleetDetailResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let detail = services
        .fleets()
        .detail(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    let tag = detail.etag();
    let mut response = Json(detail_response(&detail)).into_response();
    attach(&mut response, &tag);
    Ok(response)
}

/// `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — a partial update.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "patch_fleet",
    summary = "Update a fleet",
    description = concat!(
        "Updates fleet files or status. Every request field is optional. ",
        "`config_json` and `trigger_markdown` cannot be used together. A ",
        "killed fleet cannot be changed. Source editors can send `If-Match` ",
        "with the `ETag` from GET. A stale tag returns 412 with the current ",
        "tag in the problem body; omitting the header preserves last-write- ",
        "wins behavior. Status changes require an operator role. Config ",
        "changes require workspace membership. ",
    ),
    request_body = Option<afd_wire::fleet::PatchFleetRequest>,
    params(
        afd_http::openapi::path::Fleet,
        ("If-Match" = Option<String>, Header, description = "Optional source-version tag from GET. Stale values return 412 with the current `etag`."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PatchedFleetResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = afd_http::openapi::PRECONDITION_FAILED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: http::HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let request = read_patch(&body, if_match)?;

    // Answered without a row lock, and without a row read: a dashboard saving an
    // untouched form should not make this daemon take a lock for it.
    if request.is_empty() {
        if request.if_match.is_some() {
            return Err(Refusal::malformed(DETAIL_CONDITIONAL_EMPTY));
        }
        return Ok(Json(PatchedFleetResponse::Unchanged {
            fleet_id: Cow::Borrowed(fleet.as_str()),
            config_revision: None,
        })
        .into_response());
    }

    let patched = services
        .fleets()
        .patch(&owned.workspace, &fleet, &request, services.now())
        .await
        .map_err(stale_or(EVENT_PATCH))?;

    let mut response = Json(patched_response(&fleet, &request, &patched)).into_response();
    attach(&mut response, &patched.etag);
    Ok(response)
}

/// `DELETE /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — purge a killed fleet.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "delete_fleet",
    summary = "Permanently delete a fleet",
    description = concat!(
        "Permanently deletes the fleet, events, runs, approvals, grants, ",
        "keys, and memory. This cannot be undone. Set the fleet status to ",
        "`killed` first. Otherwise the request returns 409. Requires an ",
        "operator role. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn purge<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    services
        .fleets()
        .purge(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_PURGE))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Renders a PATCH failure, carrying the current tag when the source was stale.
///
/// A 412 whose body names the tag the row holds NOW, so an editor re-applies in
/// one round trip instead of re-reading to discover what it should have sent.
fn stale_or(event: &'static str) -> impl FnOnce(afd_fleet_lifecycle::Error) -> Refusal {
    move |error| match error.stale_tag() {
        Some(current) => Refusal::preconditioned(error.code(), error.detail(), current),
        None => Refusal::at(event)(error),
    }
}

/// Attaches the tag an editor's next conditional write is compared against.
///
/// A tag that will not render as a header value is dropped rather than failing
/// the response: the write is already committed, and a 500 here would tell a
/// caller their edit did not land when it did. It is a hex digest in quotes, so
/// the arm is unreachable in practice.
fn attach(response: &mut Response, tag: &str) {
    if let Ok(value) = HeaderValue::from_str(tag) {
        response.headers_mut().insert(header::ETAG, value);
    }
}

/// One fleet as the wire shows it.
fn detail_response(detail: &FleetDetail) -> FleetDetailResponse<'_> {
    FleetDetailResponse {
        id: Cow::Borrowed(&detail.row.id),
        name: Cow::Borrowed(&detail.row.name),
        status: Cow::Borrowed(detail.row.status.as_str()),
        source_markdown: Cow::Borrowed(&detail.source_markdown),
        trigger_markdown: detail.trigger_markdown.as_deref().map(Cow::Borrowed),
        bundle_content_hash: detail.bundle_content_hash.as_deref().map(Cow::Borrowed),
        triggers: triggers(detail.row.triggers.as_ref()),
        events_processed: detail.row.events_processed,
        budget_used_nanos: detail.row.budget_used_nanos,
        created_at: detail.row.created_at_ms,
        updated_at: detail.row.updated_at_ms,
    }
}

/// The committed PATCH's reply — which of the shapes depends on what was asked.
fn patched_response<'a>(
    fleet: &'a Uuid7,
    request: &Patch,
    patched: &'a afd_fleet_lifecycle::Patched,
) -> PatchedFleetResponse<'a> {
    match request.status {
        Some(asked) => PatchedFleetResponse::Transitioned {
            fleet_id: Cow::Borrowed(fleet.as_str()),
            status: Cow::Borrowed(asked.status().as_str()),
            config_revision: patched.revision,
            etag: Cow::Borrowed(&patched.etag),
        },
        None => PatchedFleetResponse::Changed {
            fleet_id: Cow::Borrowed(fleet.as_str()),
            config_revision: patched.revision,
            etag: Cow::Borrowed(&patched.etag),
        },
    }
}
