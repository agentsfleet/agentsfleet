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
use afd_fleet_lifecycle::{ConfigSource, FleetDetail, Patch, Requested};
use afd_wire::fleet::{FleetDetailResponse, PatchFleetRequest, PatchedFleetResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, StatusCode, header};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
pub use afd_http::handler::{DETAIL_FLEET_ID, FleetPath, parse_fleet_id};
use crate::services::{Services, WorkspaceFleets as _};

use super::triggers;

/// The scoped events each verb's failures are logged under.
const EVENT_READ: &str = "fleet_read_failed";
const EVENT_PATCH: &str = "fleet_patch_failed";
const EVENT_PURGE: &str = "fleet_purge_failed";

/// The refusal a PATCH body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal a PATCH naming both configuration sources earns.
pub const DETAIL_CONFIG_AMBIGUOUS: &str = "config_json and trigger_markdown are mutually exclusive";

/// The refusal an empty `config_json` earns.
pub const DETAIL_CONFIG_REQUIRED: &str = "config_json is required";

/// The refusal a status outside the operator-targetable set earns.
pub const DETAIL_STATUS_INVALID: &str = "status must be one of \"active\", \"stopped\", \"killed\"";

/// The refusal a conditional PATCH that asks for nothing earns.
///
/// An `If-Match` with no field to write is a caller expecting a compare that
/// cannot happen — answering 200 would tell them their edit landed.
pub const DETAIL_CONDITIONAL_EMPTY: &str = "A conditional fleet update requires at least one field";

/// The refusal a document outside its length bounds earns.
pub const DETAIL_TRIGGER_BOUNDS: &str = "trigger_markdown must be 1..64KiB";

/// The refusal a source document outside its length bounds earns.
pub const DETAIL_SOURCE_BOUNDS: &str = "source_markdown must be 1..64KiB";

/// The most bytes an authored document may carry.
///
/// The sentences above say 64KiB and this says two hundred. The mismatch is in
/// the Zig too, and it is the NUMBER that is load-bearing — ported as-is,
/// because a client sitting between the two would change class if either moved.
const MAX_MARKDOWN_LEN: usize = 200 * 1024;

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — one fleet, whole.
///
/// Carries an `ETag` over the editable surface, which the source editor sends
/// back as `If-Match`. A read that could not attach one would leave that editor
/// unable to save safely, so there is no tagless success here.
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

/// The fleet named in the path, refused before a connection is drawn.
///
/// Which is what keeps the `::uuid` cast in the statements from ever being the
/// thing that fails, leaving every error from below a genuine datastore fault.

/// The PATCH the body asks for, or the refusal it earns.
///
/// Every ambiguity is resolved HERE, once, into a type that cannot hold it: the
/// two configuration sources become one [`ConfigSource`], and the status becomes
/// a [`Requested`] that cannot spell `paused`.
fn read_patch(body: &Bytes, if_match: Option<String>) -> Result<Patch, Refusal> {
    if body.is_empty() {
        return Ok(Patch {
            if_match,
            ..Patch::default()
        });
    }
    let sent = afd_core::json::object_from_slice::<PatchFleetRequest<'_>>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    let config = match (
        sent.config_json.as_deref(),
        sent.trigger_markdown.as_deref(),
    ) {
        // Both drive `core.fleets.config_json`, so there is no answer to which
        // one wins — refused at the door rather than resolved by precedence.
        (Some(_json), Some(_document)) => return Err(Refusal::malformed(DETAIL_CONFIG_AMBIGUOUS)),
        (Some(""), None) => return Err(Refusal::malformed(DETAIL_CONFIG_REQUIRED)),
        (Some(json), None) => Some(ConfigSource::Json(json.to_owned())),
        (None, Some(document)) => Some(ConfigSource::Trigger(
            bounded(document, DETAIL_TRIGGER_BOUNDS)?.to_owned(),
        )),
        (None, None) => None,
    };
    let source_markdown = sent
        .source_markdown
        .as_deref()
        .map(|document| bounded(document, DETAIL_SOURCE_BOUNDS).map(str::to_owned))
        .transpose()?;

    Ok(Patch {
        config,
        status: sent.status.as_deref().map(requested).transpose()?,
        source_markdown,
        if_match,
    })
}

/// The document, if it is one this daemon will store.
fn bounded<'a>(document: &'a str, detail: &'static str) -> Result<&'a str, Refusal> {
    if document.is_empty() || document.len() > MAX_MARKDOWN_LEN {
        return Err(Refusal::malformed(detail));
    }
    Ok(document)
}

/// The transition a spelling asks for, or the refusal an unknown one earns.
///
/// `paused` is refused here rather than accepted and ignored: it belongs to the
/// platform's anomaly gate, and admitting it would let a caller forge a
/// system-halt provenance on their own fleet.
fn requested(spelling: &str) -> Result<Requested, Refusal> {
    match spelling {
        "active" => Ok(Requested::Active),
        "stopped" => Ok(Requested::Stopped),
        "killed" => Ok(Requested::Killed),
        _reserved_or_unknown => Err(Refusal::malformed(DETAIL_STATUS_INVALID)),
    }
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
