//! A fleet's message thread over HTTP: read the turns, or say something.
//!
//! The port of `fleets/messages_list.zig` and `fleets/messages.zig`. Two verbs
//! on one template, and they are not symmetric — the read pages history the
//! event routes already serve, and the write is the only place in this daemon
//! where a person puts work onto a fleet's stream.
//!
//! # The page is byte-budgeted, not byte-refused
//!
//! Every row here carries a trigger payload and an agent's full answer, so a
//! page of twenty-five can be enormous while a page of twenty-five listing
//! rows cannot. Rows join until the budget is spent and the cursor marks the
//! cut, which under keyset paging is a complete and truthful answer. The FIRST
//! row ships whatever it costs: a single oversized turn must not brick the
//! thread it sits at the top of.
//!
//! # A steer to a stopped fleet is refused, never accepted
//!
//! The ingress check is the difference between a 409 a person can act on and a
//! 202 whose run never happens. It reads the status alone rather than the
//! fleet, because deciding whether a message may be posted is not worth
//! loading two authored documents.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_events::{Cursor, EventDetailRow, THREAD_DEFAULT_LIMIT, THREAD_MAX_LIMIT};
use afd_wire::event::{SteerAccepted, SteerRequest, ThreadResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::event::expanded;
use crate::handler::{Refusal, parameter};
use crate::services::{FleetSteering as _, Services, WorkspaceEvents as _, WorkspaceFleets as _};

use super::detail::{FleetPath, parse_fleet_id};

/// The scoped events each verb's failures are logged under.
const EVENT_THREAD: &str = "fleet_thread_list_failed";
const EVENT_STEER: &str = "fleet_steer_failed";

/// The `starting_after` parameter's name — this surface's cursor spelling.
const QUERY_STARTING_AFTER: &str = "starting_after";

/// The `limit` parameter's name.
const QUERY_LIMIT: &str = "limit";

/// The refusal a page size outside the served band earns.
const DETAIL_LIMIT: &str = "limit must be between 1 and 25";

/// The refusal a continuation this walk did not issue earns.
const DETAIL_CURSOR: &str = "invalid starting_after cursor";

/// The refusal a steer with no body earns.
const DETAIL_BODY_REQUIRED: &str = "request body required";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal an empty message earns.
const DETAIL_MESSAGE_EMPTY: &str = "message must not be empty";

/// The refusal an over-long message earns.
const DETAIL_MESSAGE_LONG: &str = "message must not exceed 8192 bytes";

/// The refusal a fleet this workspace does not hold earns.
const DETAIL_FLEET_NOT_FOUND: &str = "Fleet not found";

/// The refusal a fleet that will not take work earns.
const DETAIL_NOT_ACTIVE: &str = "Fleet is not active";

/// The longest message a steer may carry.
///
/// `MAX_MESSAGE_LEN`, mirrored. A steer is a sentence a person typed; past
/// this it is a payload, and the fleet's own trigger surface is where a
/// payload belongs.
const MAX_MESSAGE_BYTES: usize = 8192;

/// The soft ceiling on one thread page's encoded bytes.
///
/// `THREAD_PAGE_BODY_BUDGET_BYTES`, mirrored.
const PAGE_BUDGET_BYTES: usize = 512 * 1024;

/// The word a steer's reply carries in `status`.
const STATUS_ACCEPTED: &str = "accepted";

/// The page size, or the refusal a caller outside the band earns.
///
/// One to twenty-five, an order of magnitude below the event listings' band:
/// every row here carries two bodies. Zero is refused rather than clamped —
/// a caller asking for no turns has made a mistake, and an empty page would
/// read as an empty thread.
fn parse_limit(raw: Option<&str>) -> Result<i64, Refusal> {
    let Some(raw) = raw else {
        return Ok(THREAD_DEFAULT_LIMIT);
    };
    let requested: i64 = raw
        .parse()
        .map_err(|_digits| Refusal::malformed(DETAIL_LIMIT))?;
    if !(1..=THREAD_MAX_LIMIT).contains(&requested) {
        return Err(Refusal::malformed(DETAIL_LIMIT));
    }
    Ok(requested)
}

/// The continuation this walk issued, or the refusal one it did not earns.
fn parse_cursor(raw: Option<&str>) -> Result<Option<Cursor>, Refusal> {
    raw.map(Cursor::decode)
        .transpose()
        .map_err(|_unminted| Refusal::malformed(DETAIL_CURSOR))
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages`.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "list_fleet_messages",
    summary = "List a fleet's chat thread with bodies",
    description = concat!(
        "Returns the newest chat events first. Each item carries the trigger ",
        "payload (`request_json`) and the agent's full answer ",
        "(`response_text`). One request replaces reading the event list and ",
        "then each event's detail. A page holds at most `limit` items and at ",
        "most 512 KiB of encoded items. The newest item always ships, even ",
        "alone. Follow `next_cursor` to read the rest. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
        ("starting_after" = Option<String>, Query, description = "Opaque continuation cursor from a previous page's `next_cursor`."),
        ("limit" = Option<String>, Query),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ThreadResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn thread<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let query = query.unwrap_or_default();
    let limit = parse_limit(parameter(&query, QUERY_LIMIT))?;
    let after = parse_cursor(parameter(&query, QUERY_STARTING_AFTER))?;

    // One row MORE than will be served, so has-more is a fact and not a guess.
    let fetched = services
        .events()
        .thread_for_fleet(&owned.workspace, &fleet, after.as_ref(), limit + 1)
        .await
        .map_err(Refusal::at(EVENT_THREAD))?;

    Ok(Json(page(&fetched, limit)).into_response())
}

/// `POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages`.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "post_fleet_message",
    summary = "Post a chat message to a fleet",
    description = concat!(
        "Starts a fleet run with a chat event. Returns an event identifier ",
        "for tracking in the activity stream. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 202, description = afd_http::openapi::ACCEPTED),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn steer<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let message = read_message(&body)?;

    let status = services
        .fleets()
        .ingress_status(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_STEER))?
        .ok_or_else(|| Refusal::coded(error_code::AGENTSFLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND))?;
    if !status.is_runnable() {
        return Err(Refusal::conflict(
            error_code::AGENTSFLEET_PAUSED_INGRESS,
            DETAIL_NOT_ACTIVE,
            status.as_str(),
        ));
    }

    let request_json = serde_json::to_string(&SteerRequest { message })
        .map_err(|_unencodable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    let event_id = services
        .steering()
        .append(
            fleet.as_str(),
            owned.workspace.as_str(),
            &actor_for(&person),
            &request_json,
        )
        .await
        .map_err(Refusal::at(EVENT_STEER))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(SteerAccepted {
            status: Cow::Borrowed(STATUS_ACCEPTED),
            event_id: Cow::Owned(event_id),
        }),
    )
        .into_response())
}

/// The actor this credential records, decided by its CLASS.
///
/// Never by whether a subject is present. An `agt_t` api-key resolves to the
/// capabilities of the person who minted it and carries their subject, so a
/// presence test would record every machine-driven wake as that human — worse
/// than recording nobody, because it lets an actor-shaped assertion certify "a
/// person woke this fleet" while automation did.
fn actor_for(person: &PersonIdentity) -> String {
    use afd_auth::principal::PersonCredential;
    match person.person().credential() {
        // A terminal credential and a browser session both name their human:
        // the whole point of a user-scoped credential is that a steer from a
        // terminal is attributable to one.
        PersonCredential::SessionToken { .. } | PersonCredential::CliCredential => {
            format!("{}{}", afd_events::ACTOR_PREFIX, person.subject())
        }
        PersonCredential::TenantApiKey => afd_events::ACTOR_MACHINE.to_owned(),
    }
}

/// The message a steer carries, or the refusal its body earns.
///
/// # An escaped message is a message, not a malformed body
///
/// `serde` hands back `Cow::Owned` whenever a JSON string carries an escape,
/// so a borrow-only reader would refuse every message containing a newline, a
/// quote or an emoji — which is most of what a person actually types into a
/// chat box. The sibling refusal on the approval note gets away with that
/// because an operator's note is a short justification; a steer is prose. So
/// this hands back the `Cow` and the caller re-serializes it, which is also
/// what makes the escaping on the way OUT the same library's problem rather
/// than a format string's.
fn read_message(body: &Bytes) -> Result<Cow<'_, str>, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    let request: SteerRequest<'_> = afd_core::json::object_from_slice(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;
    if request.message.is_empty() {
        return Err(Refusal::malformed(DETAIL_MESSAGE_EMPTY));
    }
    // Bounded on the DECODED bytes, which is what reaches the stream and what
    // the runner will read — not on the escaped form a client happened to send.
    if request.message.len() > MAX_MESSAGE_BYTES {
        return Err(Refusal::malformed(DETAIL_MESSAGE_LONG));
    }
    Ok(request.message)
}

/// One page, cut at the row cap or the byte budget, whichever comes first.
fn page(fetched: &[EventDetailRow], limit: i64) -> ThreadResponse<'_> {
    let included = included_under_budget(fetched, limit);
    // `get` rather than an index: `included_under_budget` cannot return more
    // than `fetched.len()`, but the slice would panic if it ever did, and a
    // proof a reader has to reconstruct is not one worth relying on here.
    let items = fetched.get(..included).unwrap_or(fetched);
    let has_more = fetched.len() > included;
    ThreadResponse {
        items: items.iter().map(expanded).collect(),
        // Never populated — see `ThreadResponse::total` on why the key stays.
        total: None,
        next_cursor: has_more.then(|| items.last()).flatten().map(|last| {
            Cow::Owned(Cursor::after(last.row.created_at, &last.row.event_id).encode())
        }),
    }
}

/// How many leading rows fit the budget, capped at `limit`.
///
/// The first row is exempt: a single turn larger than the whole budget would
/// otherwise make the thread it heads unreadable rather than merely expensive.
fn included_under_budget(rows: &[EventDetailRow], limit: i64) -> usize {
    let cap = usize::try_from(limit.max(0)).unwrap_or(usize::MAX);
    let mut spent = 0usize;
    for (taken, row) in rows.iter().enumerate() {
        if taken >= cap {
            return taken;
        }
        let cost = encoded_bytes(row);
        if taken > 0 && spent.saturating_add(cost) > PAGE_BUDGET_BYTES {
            return taken;
        }
        spent = spent.saturating_add(cost);
    }
    rows.len().min(cap)
}

/// What one row costs on the wire.
///
/// Measured by encoding the row this response will actually emit, rather than
/// by summing the columns: the budget is about bytes a client receives, and
/// the escaping in a JSON string is part of that.
fn encoded_bytes(row: &EventDetailRow) -> usize {
    serde_json::to_string(&expanded(row)).map_or(0, |text| text.len())
}

#[cfg(test)]
mod tests;
