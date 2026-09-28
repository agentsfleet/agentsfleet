//! A fleet's message thread over HTTP: read the turns.
//!
//! The port of `fleets/messages_list.zig`. The read pages history the event
//! routes already serve; the write on the same template — the only place in
//! this daemon where a person puts work onto a fleet's stream — is
//! `message_steer.rs`.
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
//! The write lives in `message_steer.rs`, split at the length cap.

use std::borrow::Cow;
use std::sync::Arc;

use afd_events::{Cursor, EventDetailRow, THREAD_DEFAULT_LIMIT, THREAD_MAX_LIMIT};
use afd_wire::event::ThreadResponse;
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::WorkspaceContext;
use crate::handler::event::expanded;
use crate::handler::{Refusal, parameter};
use crate::services::{Services, WorkspaceEvents as _};

use super::detail::{FleetPath, parse_fleet_id};

/// The scoped event a failed thread read is logged under.
const EVENT_THREAD: &str = "fleet_thread_list_failed";

/// The `starting_after` parameter's name — this surface's cursor spelling.
const QUERY_STARTING_AFTER: &str = "starting_after";

/// The `limit` parameter's name.
const QUERY_LIMIT: &str = "limit";

/// The refusal a page size outside the served band earns.
const DETAIL_LIMIT: &str = "limit must be between 1 and 25";

/// The refusal a continuation this walk did not issue earns.
const DETAIL_CURSOR: &str = "invalid starting_after cursor";

/// The soft ceiling on one thread page's encoded bytes.
///
/// `THREAD_PAGE_BODY_BUDGET_BYTES`, mirrored.
const PAGE_BUDGET_BYTES: usize = 512 * 1024;

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
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
