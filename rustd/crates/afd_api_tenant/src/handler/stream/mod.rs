//! The live tail over HTTP: one fleet's activity, or a whole workspace's.
//!
//! A stream is a `Stream` and the response body polls it, so no thread is
//! parked for the life of a connection. The decisions a caller can observe are
//! fixed here: which frames are sent, in what order, numbered how, and what a
//! caller at the ceiling is told.
//!
//! # The ceiling is claimed inside the handler, not before the ownership check
//!
//! The ownership layer is a `tower` layer and has already run by the time a
//! handler body starts, so a stream refused at the ceiling has still cost one
//! indexed point lookup. That is the price of having ONE ownership check for
//! every workspace route rather than a second copy inside this one — and a
//! point lookup is not what a tab-storm is made of.
//!
//! # Why the slot rides the stream
//!
//! [`afd_sse::Slot`] is released by `Drop`, and the stream owns it. A client
//! that vanishes mid-frame, a handler that returns early, a task cancelled by
//! shutdown — every one of them drops the response body, which drops the
//! stream, which returns the slot. There is no deregister call to forget.
//!
//! # The body shares the payload
//!
//! Every viewer of a fleet writes the same published bytes; see [`body`].

mod body;
mod guard;
mod revocable;
mod wall;

/// The revoked-stream frame and shared close guidance for both stream descriptions.
///
/// A macro rather than a `const` because `concat!` takes only literals, and this is the one
/// place the literal is written. Gated with the descriptions that use it: without `openapi`
/// nothing does.
#[cfg(feature = "openapi")]
macro_rules! access_revoked_data {
    () => {
        concat!(
            "`data: {\"kind\":\"access_revoked\",\"error_code\":\"UZ-AUTH-001\"}` ",
            "and closes; do not reconnect. Failed access checks close the stream after about ",
        )
    };
}

#[cfg(test)]
mod transport_tests;

use std::sync::Arc;

use afd_core::error_code;
use afd_sse::{Live, Slot};
use axum::extract::{Path, State};
use axum::response::Response;

use self::body::serve;

use crate::auth::{Acting, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceFleets as _};

use super::fleet::detail::{FleetPath, parse_fleet_id};

/// What both streams answer once they are open.
// Read only by the two annotations, which the default build compiles away.
#[cfg(feature = "openapi")]
const STREAM_OPEN: &str = "The stream is open; frames follow as Server-Sent Events";

/// The media type `Sse` writes, named for the document.
#[cfg(feature = "openapi")]
const EVENT_STREAM: &str = "text/event-stream";

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
        "Streams fleet activity using Server-Sent Events (SSE). Each message has `id`, ",
        "`event`, and JSON `data`; `event` matches `data.kind`.\n\n",
        "The first message is `hello`, with `fleet_ids` containing this fleet and empty ",
        "`counters`. It confirms subscription readiness. No `hello` means the live ",
        "subscription is unavailable.\n\n",
        "Activity includes:\n",
        "- `event_admitted`: accepted chat message, with `event_id`, `actor`, ",
        "`event_type`, `message`, and `created_at`.\n",
        "- `event_received`: run started, with the same fields; `message` appears for ",
        "user messages.\n",
        "- `event_complete`: the saved event without fleet or workspace identifiers, plus ",
        "`fleet_status` and `pending_approvals`. `final_reply` appears up to 64 KiB; ",
        "otherwise fetch event details.\n",
        "- `gate_opened` and `gate_resolved`: `gate_id`, `event_id`, and ",
        "`pending_approvals`. Requests outside a run use null `event_id`. Resolutions add ",
        "`status` and `resolved_by`.\n",
        "- `chunk`, `tool_call_started`, `tool_call_progress`, and `tool_call_completed`: ",
        "live reply and tool activity. Tool messages include `event_id` and `name`. Newer ",
        "runners add a unique `call_id` per call, shared across its messages.\n",
        "- Completed tools from newer runners add `status` (`succeeded`, `failed`, or ",
        "`interrupted`), the first and last five output lines, and `output_line_count`. ",
        "These lines are `output_head` and `output_tail`; process calls also include ",
        "`exit_code`.\n\n",
        "`catching_up` reports missed messages through `dropped`; zero means the count is ",
        "unknown. Reload saved events after this signal. Identifiers restart at zero per ",
        "connection; `Last-Event-ID` is ignored. `hello` and `catching_up` use zero ",
        "without advancing activity identifiers. Heartbeats arrive after 15 idle seconds, ",
        "with `kind: heartbeat` and no identifier.\n\n",
        "Access is checked every 15 seconds. `access_revoked` sends ",
        access_revoked_data!(),
        "one minute; recover as after a disconnect. At capacity, 503 `UZ-API-002` ",
        "includes `Retry-After`. Fetch missed events before reconnecting.",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = STREAM_OPEN, body = String, content_type = EVENT_STREAM),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn fleet<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Acting(principal): Acting,
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

    let tail = services.live().tail_of(fleet.as_str());
    let frames = guard::guarded(tail, Arc::clone(&services), principal, owned.workspace);
    Ok(serve(frames, slot))
}

/// `GET /v1/workspaces/{workspace_id}/events/stream`.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/events/stream",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "stream_workspace_events",
    summary = "Stream live activity for a whole workspace",
    description = concat!(
        "Streams activity for every fleet you can read using one Server-Sent Events (SSE) ",
        "connection. The first `hello` contains `kind: hello` and readable `fleet_ids`. ",
        "Activity uses the fleet stream's event types, with `fleet_id` added to each ",
        "payload. Fleets join or leave the stream as access changes.\n\n",
        "`catching_up` contains `kind: catching_up` and `dropped`, the number missed ",
        "since the previous signal. Zero means the count is unknown. Reload missed events ",
        "through `/v1/workspaces/{workspace_id}/events`. After reconnecting, reload this ",
        "history to recover the gap.\n\n",
        "Identifiers restart at zero per connection; `Last-Event-ID` is ignored. `hello` ",
        "and `catching_up` use zero without advancing activity identifiers. Heartbeats ",
        "arrive after 15 idle seconds, with `kind: heartbeat` and no identifier.\n\n",
        "Access is checked every ten seconds. `access_revoked` sends ",
        access_revoked_data!(),
        "40 seconds; recover as after a disconnect. At capacity, 503 `UZ-API-002` ",
        "includes `Retry-After`.",
    ),
    params(
        afd_http::openapi::path::Workspace,
    ),
    responses(
        (status = 200, description = STREAM_OPEN, body = String, content_type = EVENT_STREAM),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
    live.admit().ok_or_else(|| {
        afd_observability::producers::http::stream_shed();
        Refusal::at_stream_ceiling(live.carrying(), live.capacity())
    })
}
