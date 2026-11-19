//! `/v1/runners/me/leases/{lease_id}/fleets` — the fleet a lease runs reading
//! which fleets its workspace holds, and what one of them did recently.
//!
//! # The workspace is the lease's
//!
//! Both reads prove the lease first and act inside the workspace that lease
//! runs in. Nothing in a query names a workspace, so a runner cannot read
//! another workspace's fleets, and a fleet id from one reads as unknown.
//!
//! # Another fleet's words are excerpts
//!
//! An event row carries at most `EXCERPT_MAX_BYTES` of what the fleet was
//! asked and what it answered: a digest needs a line, and the full answer
//! stays in the dashboard. The reads are the stores' own, so a fleet sees
//! the list the dashboard shows and the thread its chat shows.

use std::borrow::Cow;
use std::sync::Arc;

use af_core::error_code;
use af_core::id::Uuid7;
use af_core::paging::{
    CEILING, Cursor as PageCursor, PagingRefusal, QUERY_LIMIT, QUERY_STARTING_AFTER,
};
use af_validate::{Ceiling, Limit};
use af_wire::event::message_of;
use af_wire::fleets_verb::{FleetEventItem, FleetEventsPage, FleetItem, FleetsPage, excerpt};
use afd_events::{Cursor, EventDetailRow};
use afd_fleet::lease::Standing;
use afd_fleet_lifecycle::{After, FleetPage, FleetRow};
use afd_http::handler::{Refusal, decoded_parameter, parameter, parse_id};
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};

use super::schedule::{fence, lease};
use crate::auth::RunnerIdentity;
use crate::services::{Leasing as _, Services, WorkspaceEvents as _, WorkspaceFleets as _};

/// The scoped event a fleets read that failed is logged under.
const EVENT_FAILED: &str = "runner_fleets_failed";

/// The `current_state` a holder a reclaim superseded names.
pub(super) const STATE_SUPERSEDED: &str = "superseded";

/// The refusal a fleet path segment that is not an identifier earns.
pub(super) const DETAIL_FLEET_ID: &str = "fleet_id must be a valid UUIDv7";

/// The refusal a fleet this workspace does not hold earns.
pub(super) const DETAIL_FLEET_NOT_FOUND: &str = "Fleet not found";

/// The refusal a fleets page size outside the shared band earns.
const DETAIL_LIMIT: &str = PagingRefusal::Limit.detail();

/// The most events one read serves: the thread read's own cap, since each row
/// is read with its bodies.
const EVENTS_MAX: u32 = 25;

/// The events a read serves when it names no `limit`.
const EVENTS_DEFAULT: u32 = 20;

/// The refusal an events page size outside its band earns.
const DETAIL_EVENTS_LIMIT: &str =
    const_format::concatcp!("limit must be between 1 and ", EVENTS_MAX);

/// The refusal a `starting_after` this daemon never issued earns.
const DETAIL_CURSOR: &str = "The cursor is not valid";

/// One fleet's events: [`EVENTS_DEFAULT`] unless asked, at most [`EVENTS_MAX`].
const EVENTS_CEILING: Ceiling = Ceiling::new(EVENTS_MAX, EVENTS_DEFAULT);

/// Lists the fleets of the leased fleet's workspace, newest first.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::LEASE_FLEETS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_list_fleets",
    summary = "List the workspace's fleets",
    description = concat!(
        "Lists the fleets in the running fleet's workspace, newest first, archived and ",
        "installing fleets included with their status. Continue with `starting_after` ",
        "and `limit`.",
    ),
    params(
        afd_http::openapi::path::Lease,
        afd_http::openapi::query::Fence,
        ("starting_after" = Option<String>, Query, description = "Use the previous response's `next_cursor` unchanged."),
        ("limit" = Option<u32>, Query, description = "Page size, 1 to 100. Defaults to 50."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = FleetsPage<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let query = query.unwrap_or_default();
    let token = fence(Some(&query))?;
    let limit = Limit::parse(parameter(&query, QUERY_LIMIT), CEILING)
        .map_err(|_break| Refusal::malformed(DETAIL_LIMIT))?;
    let after = list_cursor(starting_after(&query)?.as_deref())?;
    let lease = proven(&*services, runner.id(), &lease_id, token, EVENT_FAILED).await?;
    let page = services
        .fleets()
        .page(&lease.workspace_id, after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Json(fleets_page(&page)).into_response())
}

/// Lists one fleet's events, newest first, its messages still waiting for a
/// runner included.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::LEASE_FLEET_EVENTS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_list_fleet_events",
    summary = "List one fleet's recent events",
    description = concat!(
        "Lists one fleet's events in the running fleet's workspace, newest first. Each ",
        "row carries at most 1024 bytes of what was asked and what was answered. ",
        "Waiting messages read `queued`. A fleet outside the workspace returns 404 ",
        "`UZ-AGT-009`.",
    ),
    params(
        afd_http::openapi::path::Lease,
        ("fleet_id" = String, Path, description = "The fleet to read: one in the running fleet's workspace."),
        afd_http::openapi::query::Fence,
        ("starting_after" = Option<String>, Query, description = "Use the previous response's `next_cursor` unchanged."),
        ("limit" = Option<u32>, Query, description = "Page size, 1 to 25. Defaults to 20."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = FleetEventsPage<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn events<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease_id, fleet_id)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let query = query.unwrap_or_default();
    let fleet = parse_id(&fleet_id, DETAIL_FLEET_ID)?;
    let token = fence(Some(&query))?;
    let limit = Limit::parse(parameter(&query, QUERY_LIMIT), EVENTS_CEILING)
        .map_err(|_break| Refusal::malformed(DETAIL_EVENTS_LIMIT))?;
    let cursor = starting_after(&query)?
        .as_deref()
        .map(Cursor::decode)
        .transpose()
        .map_err(|_unminted| Refusal::malformed(DETAIL_CURSOR))?;
    let lease = proven(&*services, runner.id(), &lease_id, token, EVENT_FAILED).await?;
    known(&*services, &lease, &fleet).await?;
    let rows = i64::from(limit);
    // One row more than will be served, so has-more is a fact and not a guess.
    let fetched = services
        .events()
        .thread_for_fleet(&lease.workspace_id, &fleet, cursor.as_ref(), rows + 1, None)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Json(events_page(&fetched, rows)).into_response())
}

/// Proves `lease_id` is `runner`'s live lease under `token`, naming
/// `superseded` when a reclaim took it.
pub(super) async fn proven<D: Services>(
    services: &D,
    runner: &Uuid7,
    lease_id: &str,
    token: u64,
    event: &'static str,
) -> Result<Standing, Refusal> {
    services
        .leases()
        .standing(runner, lease(lease_id)?, token)
        .await
        .map_err(Refusal::conflict_or_at(event, superseded))
}

/// The state a refused standing is in, when the refusal is a conflict.
pub(super) fn superseded(error: &afd_fleet::Error) -> Option<&'static str> {
    (error.code() == error_code::RUN_STALE_FENCING_TOKEN).then_some(STATE_SUPERSEDED)
}

/// Refuses a fleet the lease's workspace does not hold, as one that never
/// existed: the scope is an authorization, not a filter.
async fn known<D: Services>(services: &D, lease: &Standing, fleet: &Uuid7) -> Result<(), Refusal> {
    let status = services
        .fleets()
        .ingress_status(&lease.workspace_id, fleet)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    if status.is_none() {
        return Err(Refusal::coded(
            error_code::AGENTSFLEET_NOT_FOUND,
            DETAIL_FLEET_NOT_FOUND,
        ));
    }
    Ok(())
}

/// The page cursor the query carries, percent-decoded: the runner form-encodes
/// it, and a cursor's `:` and `+` arrive as escapes.
fn starting_after(query: &str) -> Result<Option<Cow<'_, str>>, Refusal> {
    decoded_parameter(query, QUERY_STARTING_AFTER)
        .map_err(|_broken| Refusal::malformed(DETAIL_CURSOR))
}

/// The decoded boundary of the fleets walk, or the refusal a foreign token
/// earns: the walk keys on `(created_at, id)`, as the dashboard's list does.
fn list_cursor(raw: Option<&str>) -> Result<Option<After>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let Ok(PageCursor::Timestamp { at, id }) = PageCursor::parse(raw) else {
        return Err(Refusal::malformed(DETAIL_CURSOR));
    };
    let id = Uuid7::parse(&id).map_err(|_not_a_fleet| Refusal::malformed(DETAIL_CURSOR))?;
    Ok(Some(After { created_at: at, id }))
}

/// One page of fleets, and the cursor that continues it when a row exists
/// beyond it.
fn fleets_page(page: &FleetPage) -> FleetsPage<'_> {
    let next_cursor = page.more.then(|| page.rows.last()).flatten().map(|last| {
        Cow::Owned(
            PageCursor::Timestamp {
                at: last.created_at,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    FleetsPage {
        items: page.rows.iter().map(fleet_item).collect(),
        total: None,
        next_cursor,
    }
}

/// One fleet as another fleet reads it.
fn fleet_item(row: &FleetRow) -> FleetItem<'_> {
    FleetItem {
        fleet_id: Cow::Borrowed(&row.id),
        name: Cow::Borrowed(&row.name),
        status: Cow::Borrowed(row.status.as_str()),
        description: row.description.as_deref().map(Cow::Borrowed),
        events_processed: Cow::Owned(row.events_processed.to_string()),
    }
}

/// One page of events from `fetched`, which holds one row more than `rows`
/// when a page follows.
fn events_page(fetched: &[EventDetailRow], rows: i64) -> FleetEventsPage<'_> {
    let kept = usize::try_from(rows).unwrap_or(usize::MAX);
    let items = fetched.get(..kept).unwrap_or(fetched);
    let next_cursor = (fetched.len() > items.len())
        .then(|| items.last())
        .flatten()
        .map(|last| Cow::Owned(Cursor::after(last.row.created_at, &last.row.event_id).encode()));
    FleetEventsPage {
        items: items.iter().map(event_item).collect(),
        total: None,
        next_cursor,
    }
}

/// One event as another fleet reads it, its bodies cut to excerpts.
fn event_item(detail: &EventDetailRow) -> FleetEventItem<'_> {
    let row = &detail.row;
    let request_excerpt = match message_of(&detail.request_json) {
        Cow::Borrowed(text) => Cow::Borrowed(excerpt(text)),
        Cow::Owned(text) => Cow::Owned(excerpt(&text).to_owned()),
    };
    FleetEventItem {
        event_id: Cow::Borrowed(row.event_id.as_str()),
        actor: Cow::Borrowed(&row.actor),
        event_type: Cow::Borrowed(&row.event_type),
        status: Cow::Borrowed(&row.status),
        created_at: row.created_at,
        request_excerpt,
        response_excerpt: detail
            .response_text
            .as_deref()
            .map(|text| Cow::Borrowed(excerpt(text))),
        failure_label: row.failure_label.as_deref().map(Cow::Borrowed),
    }
}
