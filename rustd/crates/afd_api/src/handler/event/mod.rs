//! The narrative log over HTTP: a workspace's history, a fleet's, and one
//! event expanded.
//!
//! The port of `workspaces/events.zig`, `fleets/events.zig` and
//! `fleets/event_detail.zig`. The parameters those three read live in the
//! `query` module beside this one; what is here is the three verbs and how a
//! stored row becomes the wire's.
//!
//! # Two listings, one statement
//!
//! `/events` and `/fleets/{id}/events` answer the same question with the fleet
//! free or fixed, and the console's Live Wall drills from the first to the
//! second without changing endpoint. They bind one statement through
//! [`crate::services::WorkspaceEvents`], so the two cannot disagree about a
//! fleet's history —
//! which is what eight concatenated statement variants in the Zig store
//! eventually would.
//!
//! # Rows are borrowed onto the wire, never copied
//!
//! `EventSummary<'a>` and `EventDetail<'a>` are `#[serde(borrow)]` throughout,
//! so a page of two hundred rows serializes out of the strings the store
//! already allocated. The page is held by the caller for exactly as long as the
//! response is built, which is what makes the borrow sound.
//!
//! # 404 covers three facts, deliberately
//!
//! No such event, an event of another fleet, and an event of another workspace
//! answer identically. The statement carries both scopes as predicates, so this
//! handler receives one empty result for all three and could not tell them
//! apart if it wanted to — see `afd_core::error_code::EVENT_NOT_FOUND`.

mod query;

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_events::{Cursor, EventDetailRow, EventRow, next_cursor};
use afd_wire::event::{EventDetail, EventSummary, EventsResponse};
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceEvents as _};

use self::query::{Listing, WorkspaceListing};

/// The scoped events each verb's failures are logged under.
const EVENT_WORKSPACE_LIST: &str = "workspace_events_list_failed";
const EVENT_FLEET_LIST: &str = "fleet_events_list_failed";
const EVENT_DETAIL: &str = "fleet_event_detail_failed";

/// The refusal a fleet segment that is not an identifier earns.
const DETAIL_FLEET_ID: &str = "fleet_id must be a UUIDv7";

/// The refusal an event segment this daemon will not look up earns.
const DETAIL_EVENT_ID: &str = "event_id is required";

/// The refusal an event this workspace and fleet do not hold earns.
///
/// `S_EVENT_NOT_FOUND`, kept verbatim.
const DETAIL_EVENT_NOT_FOUND: &str = "Event not found";

/// The longest event identifier this surface will look up.
///
/// `EVENT_ID_MAX_LEN`, mirrored. `event_id` is TEXT on `core.fleet_events` and
/// arrives on the wire from the producer rather than being minted here, so
/// there is no shape to validate. This bound only refuses an identifier long
/// enough to be an attack on the index rather than a lookup.
const EVENT_ID_MAX_LEN: usize = 256;

/// The segments the per-fleet listing's template carries.
#[derive(Debug, Deserialize)]
pub struct FleetPath {
    /// The fleet named in the path, still text.
    pub fleet_id: String,
}

/// The segments the expanded read's template carries.
#[derive(Debug, Deserialize)]
pub struct EventPath {
    /// The fleet named in the path, still text.
    pub fleet_id: String,
    /// The event named in the path. Free-form TEXT, never an identifier this
    /// daemon minted.
    pub event_id: String,
}

/// `GET /v1/workspaces/{workspace_id}/events` — the whole workspace's history.
pub(crate) async fn workspace_list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let asked = WorkspaceListing::parse(&query.unwrap_or_default(), services.now())?;
    let page = services
        .events()
        .page_for_workspace(
            &owned.workspace,
            asked.fleet.as_ref(),
            &asked.listing.filter,
            asked.listing.cursor.as_ref(),
            asked.listing.limit,
        )
        .await
        .map_err(Refusal::at(EVENT_WORKSPACE_LIST))?;

    Ok(Json(page_response(&page, asked.listing.limit)).into_response())
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events` — one fleet's.
pub(crate) async fn fleet_list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&fleet_id)?;
    let asked = Listing::parse(&query.unwrap_or_default(), services.now())?;
    let page = services
        .events()
        .page_for_fleet(
            &owned.workspace,
            &fleet,
            &asked.filter,
            asked.cursor.as_ref(),
            asked.limit,
        )
        .await
        .map_err(Refusal::at(EVENT_FLEET_LIST))?;

    Ok(Json(page_response(&page, asked.limit)).into_response())
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}`.
pub(crate) async fn detail<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(EventPath { fleet_id, event_id }): Path<EventPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet(&fleet_id)?;
    if event_id.is_empty() || event_id.len() > EVENT_ID_MAX_LEN {
        return Err(Refusal::malformed(DETAIL_EVENT_ID));
    }

    let found = services
        .events()
        .one(&owned.workspace, &fleet, &event_id)
        .await
        .map_err(Refusal::at(EVENT_DETAIL))?;

    let event =
        found.ok_or_else(|| Refusal::coded(error_code::EVENT_NOT_FOUND, DETAIL_EVENT_NOT_FOUND))?;
    Ok(Json(expanded(&event)).into_response())
}

/// The fleet the path names, or the refusal a non-identifier earns.
fn parse_fleet(fleet_id: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(fleet_id).map_err(|_shape| Refusal::malformed(DETAIL_FLEET_ID))
}

/// One page, with the cursor the next one resumes from.
fn page_response(page: &[EventRow], limit: i64) -> EventsResponse<'_> {
    EventsResponse {
        items: page.iter().map(summary).collect(),
        next_cursor: next_cursor(page, limit)
            .as_ref()
            .map(Cursor::encode)
            .map(Cow::Owned),
    }
}

/// One stored row, as a listing shows it.
fn summary(row: &EventRow) -> EventSummary<'_> {
    EventSummary {
        fleet_id: Cow::Borrowed(&row.fleet_id),
        event_id: Cow::Borrowed(&row.event_id),
        workspace_id: Cow::Borrowed(&row.workspace_id),
        actor: Cow::Borrowed(&row.actor),
        event_type: Cow::Borrowed(&row.event_type),
        status: Cow::Borrowed(&row.status),
        tokens: row.tokens,
        wall_ms: row.wall_ms,
        failure_label: row.failure_label.as_deref().map(Cow::Borrowed),
        failure_detail: row.failure_detail.as_deref().map(Cow::Borrowed),
        checkpoint_id: row.checkpoint_id.as_deref().map(Cow::Borrowed),
        resumes_event_id: row.resumes_event_id.as_deref().map(Cow::Borrowed),
        created_at: row.created_at,
        updated_at: row.updated_at,
        cost_nanos: row.cost_nanos,
    }
}

/// One stored row, as the expanded read shows it.
///
/// The bodies sit between `status` and `tokens` because that is the order the
/// daemon this ports already emits — see `afd_wire::event::EventDetail` on why
/// the two wire types are not one type plus two fields.
pub(crate) fn expanded(event: &EventDetailRow) -> EventDetail<'_> {
    let row = &event.row;
    EventDetail {
        fleet_id: Cow::Borrowed(&row.fleet_id),
        event_id: Cow::Borrowed(&row.event_id),
        workspace_id: Cow::Borrowed(&row.workspace_id),
        actor: Cow::Borrowed(&row.actor),
        event_type: Cow::Borrowed(&row.event_type),
        status: Cow::Borrowed(&row.status),
        request_json: Cow::Borrowed(&event.request_json),
        response_text: event.response_text.as_deref().map(Cow::Borrowed),
        tokens: row.tokens,
        wall_ms: row.wall_ms,
        failure_label: row.failure_label.as_deref().map(Cow::Borrowed),
        failure_detail: row.failure_detail.as_deref().map(Cow::Borrowed),
        checkpoint_id: row.checkpoint_id.as_deref().map(Cow::Borrowed),
        resumes_event_id: row.resumes_event_id.as_deref().map(Cow::Borrowed),
        created_at: row.created_at,
        updated_at: row.updated_at,
        cost_nanos: row.cost_nanos,
    }
}
