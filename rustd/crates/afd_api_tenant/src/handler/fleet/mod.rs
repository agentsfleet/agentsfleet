//! The shared workspace fleet directory; native setup lives in `native`.
//!
//! The item half — read, edit, purge — is [`detail`], split along the line the
//! route table already draws: everything here is addressed by a workspace
//! alone, everything there by a fleet as well.
//!
//! # Ownership is not checked here, and that is the point
//!
//! A hand-written check at the top of each handler is one a new handler could
//! forget, and that would be a cross-tenant read with nothing failing. The
//! check is a LAYER mounted from the route's own template, so
//! [`WorkspaceContext`] is a handler saying which workspace it is acting in —
//! never a handler deciding whether it may.

pub mod detail;
pub mod memory;
pub mod memory_access;
mod memory_request;
pub mod message;
pub mod message_steer;
pub(crate) mod native;

use std::borrow::Cow;
use std::sync::Arc;

use afd_api_wire::fleet::{FleetSummary, FleetsResponse, Triggers};
use afd_core::id::Uuid7;
use afd_core::paging::Cursor;
use afd_fleet_lifecycle::{After, FleetPage, FleetRow};
use axum::Json;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use serde_json::value::RawValue;

use crate::auth::WorkspaceContext;
use crate::handler::{Refusal, parameter};
use crate::services::{Services, WorkspaceFleets as _};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "fleet_list_failed";

/// The page a caller naming no `limit` gets.
///
/// Twenty where the workspace directory serves fifty — each surface publishes
/// its own default, kept apart on purpose.
const LIST_LIMIT_DEFAULT: u32 = 20;

/// The most rows one list page may carry.
const LIST_LIMIT_MAX: u32 = 100;

/// The `starting_after` parameter's name.
const QUERY_STARTING_AFTER: &str = "starting_after";

/// The `limit` parameter's name.
const QUERY_LIMIT: &str = "limit";

/// The refusal a `starting_after` this daemon never issued earns.
pub const DETAIL_INVALID_CURSOR: &str = "Invalid cursor format";

/// `GET /v1/workspaces/{workspace_id}/fleets` — one page, newest first.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "list_fleets",
    summary = "List fleets in a workspace",
    description = concat!(
        "Returns visible fleets with the newest fleet first. Omit ",
        "`starting_after` for the first page. Use the returned `next_cursor` ",
        "to read the next page. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
        ("starting_after" = Option<String>, Query, description = "Opaque page token from a prior response's `next_cursor` field. Omit for the first page."),
        ("limit" = Option<String>, Query, description = "Page size. Defaults to 20. Clamped to a maximum of 100."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = FleetsResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
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
    let query = query.unwrap_or_default();
    let limit = limit_or_default(parameter(&query, QUERY_LIMIT));
    let after = parse_cursor(parameter(&query, QUERY_STARTING_AFTER))?;

    let page = services
        .fleets()
        .page(&owned.workspace, after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(page_response(&page)).into_response())
}

/// The page size asked for, clamped, or the default.
///
/// Total rather than fallible: a limit that will not parse reads as the default
/// here, where the workspace directory answers a 400. Each is its own surface's
/// published behaviour — making them agree would move a client sitting on
/// either into a different status class.
fn limit_or_default(raw: Option<&str>) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok())
        .filter(|asked| *asked > 0)
        .map_or(LIST_LIMIT_DEFAULT, |asked| asked.min(LIST_LIMIT_MAX))
}

/// The decoded boundary, or the refusal a foreign token earns.
fn parse_cursor(raw: Option<&str>) -> Result<Option<After>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let cursor =
        Cursor::parse(raw).map_err(|_foreign| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    let Cursor::Timestamp { at, id } = cursor else {
        // Some other list's token: the fleets walk keys on `(created_at, id)`,
        // and a text-boundary cursor names a sort this one does not have.
        return Err(Refusal::malformed(DETAIL_INVALID_CURSOR));
    };
    let id = Uuid7::parse(&id).map_err(|_not_a_fleet| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    Ok(Some(After { created_at: at, id }))
}

/// One page, and the cursor that continues it.
///
/// The cursor is emitted only when a row EXISTS beyond this page — decided by
/// over-fetching rather than by the page being full — so a client never spends a
/// request on a page that comes back empty.
fn page_response(page: &FleetPage) -> FleetsResponse<'_> {
    let next_cursor = page.more.then(|| page.rows.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at: last.created_at,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    FleetsResponse {
        items: page.rows.iter().map(summary).collect(),
        total: None,
        next_cursor,
    }
}

/// One row as the wire shows it.
fn summary(row: &FleetRow) -> FleetSummary<'_> {
    FleetSummary {
        id: Cow::Borrowed(&row.id),
        name: Cow::Borrowed(&row.name),
        status: Cow::Borrowed(row.status.as_str()),
        created_at: row.created_at,
        updated_at: row.updated_at,
        triggers: triggers(row.triggers.as_ref()),
        events_processed: row.events_processed.to_string(),
        budget_used_nanos: row.budget_used_nanos.to_string(),
    }
}

/// The stored trigger projection, spliced as raw JSON.
///
/// The projection is cast from a `JSONB` column, so it always parses; the
/// `.ok()` is the conversion's error type, not a path a stored row takes.
pub(super) fn triggers(stored: Option<&afd_fleet_lifecycle::Triggers>) -> Triggers {
    stored.and_then(|projection| RawValue::from_string(projection.as_json_text().to_owned()).ok())
}

#[cfg(test)]
mod tests;
