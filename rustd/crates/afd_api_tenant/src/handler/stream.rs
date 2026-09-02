//! The live tail over HTTP: one fleet's activity, or a whole workspace's.
//!
//! The port of `fleets/events_stream.zig` and `workspaces/events_stream.zig`,
//! and the shape is not theirs. That daemon hands the socket to a detached
//! thread because a parked handler-pool thread would black-hole its queue's
//! share of every later request; here a stream is a `Stream`, the response body
//! polls it, and nothing is parked. What survives the port is every OBSERVABLE
//! decision: which frames are sent, in what order, numbered how, and what a
//! caller at the ceiling is told.
//!
//! # The ceiling is claimed inside the handler, not before the ownership check
//!
//! The daemon this ports claims its registry slot before authorizing, so a
//! tab-storm is shed for the cost of one lock. Here the ownership layer is a
//! `tower` layer and has already run by the time a handler body starts, so a
//! refused stream costs one indexed point lookup more than it does there. That
//! is the price of having ONE ownership check for every workspace route rather
//! than a second copy inside this one — and a point lookup is not what a
//! tab-storm is made of.
//!
//! # Why the slot rides the stream
//!
//! [`afd_sse::Slot`] is released by `Drop`, and the stream owns it. A client
//! that vanishes mid-frame, a handler that returns early, a task cancelled by
//! shutdown — every one of them drops the response body, which drops the
//! stream, which returns the slot. There is no deregister call to forget.

mod wall;

use std::convert::Infallible;
use std::sync::Arc;

use afd_core::error_code;
use afd_sse::{Frame, Live, Slot};
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse as _, Response};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use crate::auth::{Acting, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceFleets as _};

use super::fleet::detail::{FleetPath, parse_fleet_id};

/// The scoped event each surface's failures are logged under.
const EVENT_FLEET_STREAM: &str = "fleet_events_stream_failed";

/// The workspace multiplex's.
const EVENT_WORKSPACE_STREAM: &str = "workspace_events_stream_failed";

/// The refusal a fleet this workspace does not hold earns.
const DETAIL_FLEET_NOT_FOUND: &str = "Fleet not found";

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream`.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "stream_fleet_events",
    summary = "Stream live fleet activity",
    description = concat!(
        "Opens a Server-Sent Events (SSE) stream for new fleet activity. Each ",
        "message includes `id`, `event`, and `data`. Identifiers restart at 0 ",
        "for each connection. The route ignores `Last-Event-ID`. At capacity, ",
        "the route returns 503 `UZ-API-002` with `Retry-After`. Read missed ",
        "events before reconnecting. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn fleet<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let slot = admit(services.live())?;

    // Read the status only to prove the fleet is THIS workspace's. A stream
    // opened on a fleet somebody else owns would deliver their frames, so the
    // check is not a nicety — and a stopped fleet still streams, because a
    // person watching one wants to see it stop.
    services
        .fleets()
        .ingress_status(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_FLEET_STREAM))?
        .ok_or_else(|| Refusal::coded(error_code::AGENTSFLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND))?;

    Ok(serve(services.live().tail_of(fleet.as_str()), slot))
}

/// `GET /v1/workspaces/{workspace_id}/events/stream`.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/events/stream",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "stream_workspace_events",
    summary = "Stream live activity for a whole workspace",
    description = concat!(
        "Opens ONE Server-Sent Events (SSE) stream carrying live activity for ",
        "every fleet the caller can read in the workspace. It is the Fleets ",
        "Wall's single connection, replacing one stream per tile. The first ",
        "frame is `event: hello`. Its `data` is ",
        "`{\"kind\":\"hello\",\"fleet_ids\":[...]}` for the current readable fleet ",
        "set. Activity frame `data` contains the publisher payload with ",
        "`fleet_id` added as the leading field. The client routes each frame ",
        "by that tag. A stalled client can overflow its bounded server queue. ",
        "The server then sends `event: catching_up` with ",
        "`{\"kind\":\"catching_up\",\"dropped\":N}`. `dropped` is the new drop ",
        "count since the previous signal. Control frames use identifier 0 and ",
        "do not advance the activity sequence. Activity identifiers start at ",
        "0 for each connection. The route ignores `Last-Event-ID`. The ",
        "connection adjusts its fan-in as fleets appear or disappear. A ",
        "caller whose workspace access is revoked stops receiving on the next ",
        "refresh. At capacity the route returns 503 `UZ-API-002` with `Retry- ",
        "After`. After a reconnect opens, recover the gap through `GET ",
        "/v1/workspaces/{workspace_id}/events`. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn workspace<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Acting(principal): Acting,
) -> Result<Response, Refusal> {
    let slot = admit(services.live())?;

    // The first enumeration runs HERE rather than on the stream's first poll,
    // so a workspace whose datastore is down is refused with a status code
    // instead of being handed an open connection that will never say anything.
    let fleets = services
        .fleets()
        .live_set(&owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_WORKSPACE_STREAM))?;

    let frames = wall::frames(services, owned.workspace, principal, &fleets);
    Ok(serve(frames, slot))
}

/// A slot for one stream, or the refusal a full instance answers.
fn admit(live: &Live) -> Result<Slot, Refusal> {
    live.admit()
        .ok_or_else(|| Refusal::at_stream_ceiling(live.carrying(), live.capacity()))
}

/// One response body, holding `slot` for as long as it is alive.
///
/// The heartbeat is `axum`'s own keep-alive rather than a frame this crate
/// emits: it is a comment, an `EventSource` ignores it, and the WRITE is the
/// point — it is what discovers a client that went away without closing.
fn serve(frames: BoxStream<'static, Frame>, slot: Slot) -> Response {
    let held = stream::unfold((frames, slot), |(mut frames, slot)| async move {
        let frame = frames.next().await?;
        Some((Ok::<Event, Infallible>(event_of(frame)), (frames, slot)))
    });
    Sse::new(held)
        .keep_alive(
            KeepAlive::new()
                .interval(afd_sse::HEARTBEAT_INTERVAL)
                .text(afd_sse::HEARTBEAT_TEXT),
        )
        .into_response()
}

/// One decided frame, as `axum` writes it.
///
/// The `id:` line is this CONNECTION's counter. A browser will send it back as
/// `Last-Event-ID` on reconnect and this daemon ignores it — honouring it would
/// promise a resumption pub/sub cannot deliver, because it keeps nothing to
/// resume from. The client recovers the gap through the events list.
fn event_of(frame: Frame) -> Event {
    Event::default()
        .id(frame.seq.to_string())
        .event(frame.kind)
        .data(frame.data)
}
