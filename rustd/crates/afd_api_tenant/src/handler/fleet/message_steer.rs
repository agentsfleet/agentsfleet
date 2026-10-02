//! A fleet's message thread over HTTP: say something.
//!
//! The write half of `message.rs`, split at the length cap. The port of
//! `fleets/messages.zig`, and the only place in this daemon where a person puts
//! work onto a fleet's stream.
//!
//! # A steer to a stopped fleet is refused, never accepted
//!
//! The ingress check is the difference between a 409 a person can act on and a
//! 202 whose run never happens. It reads the status alone rather than the
//! fleet, because deciding whether a message may be posted is not worth
//! loading two authored documents.
//!
//! # A repeat is answered even by a fleet that stopped taking work
//!
//! A caller that names its operation and never saw the 202 sends again. If the
//! first send was admitted, the answer is that send's event — even when the
//! fleet has stopped or paused since, because the message is already on its
//! way to run. Ownership is checked first, so the lookup only ever reads this
//! workspace's fleet, and a runnable fleet pays no lookup at all: the ledger's
//! own insert answers a repeat (`afd_events::steer`).

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_events::Steered;
use afd_wire::event::{EventType, SteerAccepted, SteerRequest, operation_id_usable};
use afd_wire::tail::TailFrame;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;
use http::StatusCode;

use afd_fleet_lifecycle::FleetStatus;

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{FleetSteering, Services, WorkspaceFleets as _};

use super::detail::{FleetPath, parse_fleet_id};

/// The scoped event a failed steer is logged under.
const EVENT_STEER: &str = "fleet_steer_failed";

/// The state a reused operation id's 409 names: the id already became a
/// message.
const STATE_ADMITTED: &str = "admitted";

/// The refusal a steer with no body earns.
const DETAIL_BODY_REQUIRED: &str = "request body required";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal an empty message earns.
const DETAIL_MESSAGE_EMPTY: &str = "message must not be empty";

/// The refusal a message past the bound, or holding NUL, earns.
///
/// One sentence names both rules: a caller told only the one it did not break
/// would go looking at the wrong limit.
const DETAIL_MESSAGE_INVALID: &str =
    "message must not exceed 8192 bytes or contain a NUL character";

/// The refusal an unusable client operation identity earns.
///
/// One sentence names every rule the field can break (both ends of the bound
/// and a NUL inside it), so a caller told only the one it did not break never
/// goes looking at the wrong limit. It also says omitting the field is valid,
/// since the field is optional.
const DETAIL_OPERATION_ID_INVALID: &str = "operation_id, when sent, must be 1 to 200 bytes with no NUL character; omit it to send without retry protection";

/// The refusal a fleet this workspace does not hold earns.
const DETAIL_FLEET_NOT_FOUND: &str = "Fleet not found";

/// The refusal a fleet that will not take work earns.
const DETAIL_NOT_ACTIVE: &str = "Fleet is not active";

/// The word a steer's reply carries in `status`.
const STATUS_ACCEPTED: &str = "accepted";

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
        "Send `operation_id` to make a retry safe: repeat the same value and ",
        "this endpoint returns the first run's event, never a second run. ",
        "A lost response then costs nothing, even if the fleet stopped or ",
        "paused since. The same value with a different message, or from another ",
        "sender, is refused with 409 `UZ-AGT-016`. Each signed-in person is one ",
        "sender; a workspace's API keys are one between them. Omit it and ",
        "every call is a new message, which is what a person sending twice means. ",
    ),
    request_body = SteerRequest,
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 202, description = afd_http::openapi::ACCEPTED, body = SteerAccepted),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
    let steer = read_steer(&body)?;
    let actor = actor_for(person.person());

    // Ownership first: a fleet this workspace does not hold is a 404 before any
    // operation id is looked at, so a guessed id can never probe another
    // tenant's ledger.
    let status = services
        .fleets()
        .ingress_status(&owned.workspace, &fleet)
        .await
        .map_err(Refusal::at(EVENT_STEER))?
        .ok_or_else(|| Refusal::coded(error_code::AGENTSFLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND))?;
    let request_json = stored_payload(&steer.message)?;
    let operation_id = steer.operation_id.as_deref();
    let (fleet, workspace) = (fleet.as_str(), owned.workspace.as_str());
    let steering = services.steering();
    if !status.is_runnable() {
        // A fleet that will not take work refuses new messages — but answers a
        // retry of one it admitted while it did, with that message's event:
        // refusing the retry would report as never sent a message that is
        // going to run.
        let replayed = match operation_id {
            Some(operation) => steering
                .replayed(fleet, workspace, &actor, &request_json, operation)
                .await
                .map_err(refuse_steer)?,
            None => None,
        };
        return replayed
            .map(|event_id| accepted(event_id, true))
            .ok_or_else(|| not_runnable(status));
    }
    let steered = steering
        .append(fleet, workspace, &actor, &request_json, operation_id)
        .await
        .map_err(refuse_steer)?;
    announce_admitted(steering, fleet, &steered, &actor, &steer.message).await;
    let replayed = steered.replayed();
    Ok(accepted(steered.event_id, replayed))
}

/// Tells every screen on the fleet the message was accepted, before the 202.
///
/// Once per message: a repeat carries no admission instant, because its first
/// send announced it. Best-effort, like every tail frame — a queue that will
/// not take it is logged by the publisher and the steer is still accepted.
async fn announce_admitted(
    steering: &impl FleetSteering,
    fleet: &str,
    steered: &Steered,
    actor: &str,
    message: &str,
) {
    let Some(created_at) = steered.admitted_at else {
        return;
    };
    let frame = TailFrame::EventAdmitted {
        event_id: Cow::Borrowed(&steered.event_id),
        actor: Cow::Borrowed(actor),
        event_type: Cow::Borrowed(EventType::Chat.as_str()),
        message: Cow::Borrowed(message),
        created_at,
    };
    steering.announce(fleet, &frame).await;
}

/// The body the ledger stores for a message.
///
/// It deliberately carries NO operation id. That is a transport fact — how the
/// CALLER names its retry — and not something the fleet reads, so putting it in
/// the body would hand every run a field it has no use for and change the bytes
/// a replay re-appends. The ledger holds it where it belongs, as `producer_key`.
fn stored_payload(message: &str) -> Result<String, Refusal> {
    serde_json::to_string(&SteerRequest {
        message: Cow::Borrowed(message),
        operation_id: None,
    })
    .map_err(|_unencodable| Refusal::malformed(DETAIL_MALFORMED_JSON))
}

/// The 409 a new message meets on a fleet that will not take work.
fn not_runnable(status: FleetStatus) -> Refusal {
    Refusal::conflict(
        error_code::AGENTSFLEET_PAUSED_INGRESS,
        DETAIL_NOT_ACTIVE,
        status.as_str(),
    )
}

/// The 202 a steer answers with, for a new message or a repeat alike — and
/// which of the two it is.
fn accepted(event_id: String, replayed: bool) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(SteerAccepted {
            status: Cow::Borrowed(STATUS_ACCEPTED),
            event_id: Cow::Owned(event_id),
            replayed,
        }),
    )
        .into_response()
}

/// The refusal a failed steer earns: a reused operation id is a 409 naming the
/// state the id is in; everything else renders as its plane decided.
fn refuse_steer(error: afd_events::Error) -> Refusal {
    if error.is_operation_conflict() {
        Refusal::conflict_at(EVENT_STEER, STATE_ADMITTED)(error)
    } else {
        Refusal::at(EVENT_STEER)(error)
    }
}

/// The actor this credential records, decided by its CLASS.
///
/// Never by whether a subject is present. An `agt_t` api-key resolves to the
/// capabilities of the person who minted it and carries their subject, so a
/// presence test would record every machine-driven wake as that human — worse
/// than recording nobody, because it lets an actor-shaped assertion certify "a
/// person woke this fleet" while automation did.
fn actor_for(person: &afd_auth::principal::Person) -> String {
    use afd_auth::principal::PersonCredential;
    match person.credential() {
        // A terminal credential and a browser session both name their human:
        // the whole point of a user-scoped credential is that a steer from a
        // terminal is attributable to one.
        PersonCredential::SessionToken { .. } | PersonCredential::CliCredential => {
            afd_events::steer_actor(person.subject().as_str())
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
fn read_steer(body: &Bytes) -> Result<SteerRequest<'_>, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    let request: SteerRequest<'_> = afd_http::handler::read_body(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;
    if request.validate().is_err() {
        // Three sentences read back off one report, because they are three
        // different mistakes to whoever has to fix them. The operation id is
        // tested FIRST: a caller that sent a bad one and was told its message
        // was empty would go looking at the wrong field.
        let operation_unusable = request
            .operation_id
            .as_deref()
            .is_some_and(|id| !operation_id_usable(id));
        return Err(Refusal::malformed(if operation_unusable {
            DETAIL_OPERATION_ID_INVALID
        } else if request.message.is_empty() {
            DETAIL_MESSAGE_EMPTY
        } else {
            DETAIL_MESSAGE_INVALID
        }));
    }
    Ok(request)
}

#[cfg(test)]
mod tests;
