//! `POST /v1/runners/me/runs/{run_id}/steers` — the fleet a run runs
//! putting work on another fleet in its workspace.
//!
//! # A person's steer in every field but four
//!
//! The turn is the one a person's message makes, through the statement
//! that decides the target, a repeat, its status and the deployment ceiling.
//! The actor names the sending fleet, the stored body names it too, the reply
//! destination is copied from the event the sender runs, and the key is the
//! daemon's (`afd_turns::view::steer`).
//!
//! # The run is proved twice
//!
//! Once to learn the sender, and again inside the turn's transaction
//! through the write guard a schedule write uses, so a runner whose run a
//! reassignment took after the first proof writes nothing.
//!
//! # Nothing limits the targets, the chain or the count
//!
//! The fleet's `tools` list is the consent. A fleet may steer any other fleet
//! in its workspace, as often as its budget allows, and never itself.

use std::borrow::Cow;
use std::sync::Arc;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_validate::{Sentences, TEXT_MAX_CHARS};
use af_wire::steer_verb::{FleetSteerAccepted, FleetSteerRequest};
use af_wire::turn::TurnKind;
use afd_api_wire::tail::{FrameText, TailFrame};
use afd_fleet::run::Standing;
use afd_fleet::run::message::SteerSender;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_turns::Unheld;
use afd_turns::view::{FleetSteer, FleetSteered, fleet_actor};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;
use http::StatusCode;

use super::fleets::{DETAIL_FLEET_ID, DETAIL_FLEET_NOT_FOUND, STATE_SUPERSEDED, proven};
use super::schedule::write_fence;
use crate::auth::RunnerIdentity;
use crate::services::{FleetSteering as _, RunPlane as _, RunnerPlane};

/// The scoped event a steer that failed outright is logged under.
const EVENT_FAILED: &str = "runner_fleet_steer_failed";

/// A steer the daemon refused, logged with its code and both fleets.
const EVENT_REFUSED: &str = "runner_fleet_steer_refused";

/// A steer admitted or answered as a repeat.
const EVENT_ADMITTED: &str = "runner_fleet_steer_admitted";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed steer body";

/// The refusal a text outside its bounds earns: one sentence naming each rule
/// the field can break, the text ceiling among them.
const DETAIL_TEXT: &str = const_format::concatcp!(
    "text must be 1 to ",
    TEXT_MAX_CHARS,
    " characters with no NUL"
);

/// Which field a `garde` report names, as the sentence a runner reads.
const SENTENCES: Sentences = Sentences::new(&[("fleet_id", DETAIL_FLEET_ID)], DETAIL_TEXT);

/// The refusal a steer to the sending fleet earns.
const DETAIL_SELF: &str = "A fleet cannot steer itself";

/// The refusal a target that takes no new work earns.
const DETAIL_NOT_ACTIVE: &str = "Fleet is not active";

/// The refusal the same key already holding other words earns.
const DETAIL_CONFLICT: &str = "This steer was already sent with different words";

/// The refusal a holder a reassignment superseded earns, found inside the write.
const DETAIL_SUPERSEDED: &str = "A newer runner holds this run";

/// The `current_state` a steer to the sending fleet names.
const STATE_SELF: &str = "self";

/// The `current_state` a key already holding other words names.
const STATE_ADMITTED: &str = "admitted";

/// What [`handle`] documents.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = concat!(
    "Puts work on another fleet in the running fleet's workspace, as a chat message ",
    "that names the sender. The target answers in the sender's thread and runs ",
    "under its own setup. The same words from the same event return the first ",
    "`turn_id` with `replayed` true. A steer to the sender returns 409 `UZ-AGT-018`. ",
    "A fleet outside the workspace returns 404 `UZ-AGT-009`.",
);

/// Puts work on another fleet in the held fleet's workspace.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUN_STEERS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_steer_fleet",
    summary = "Steer another fleet",
    description = DESCRIPTION,
    request_body = FleetSteerRequest<'_>,
    params(afd_http::openapi::path::Run),
    responses(
        (status = 202, description = afd_http::openapi::ACCEPTED, body = FleetSteerAccepted<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn handle<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request = read_request(&body)?;
    let target = parse_id(&request.fleet_id, DETAIL_FLEET_ID)?;
    let standing = proven(
        &*services,
        runner.id(),
        &run_id,
        request.fencing_token,
        EVENT_FAILED,
    )
    .await?;
    if target == standing.fleet_id {
        log_refused(&standing, &target, error_code::FLEET_STEER_SELF);
        return Err(Refusal::conflict(
            error_code::FLEET_STEER_SELF,
            DETAIL_SELF,
            STATE_SELF,
        ));
    }
    let sender = services
        .runs()
        .sender(&standing, &request.text)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let guard = write_fence(runner.id(), &run_id, request.fencing_token)?;
    let steer = FleetSteer {
        target: &target,
        workspace: &standing.workspace_id,
        sender_fleet: &standing.fleet_id,
        sender_event: &standing.turn_id,
        masked_text: &sender.text,
        request_json: &sender.request_json,
    };
    let answered = services.steering().append_from_fleet(steer, &guard).await;
    let steered = settled(answered, &standing, &target)?;
    log_admitted(&standing, &target, &steered);
    announce(&*services, &target, &standing, &sender, &steered).await;
    let accepted = FleetSteerAccepted {
        replayed: steered.replayed(),
        turn_id: Cow::Owned(steered.turn_id),
    };
    Ok((StatusCode::ACCEPTED, Json(accepted)).into_response())
}

/// The request, read strictly and proved inside its bounds.
fn read_request(body: &Bytes) -> Result<FleetSteerRequest<'_>, Refusal> {
    let request: FleetSteerRequest<'_> =
        read_strict_body(body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    request
        .validate()
        .map_err(|report| Refusal::malformed(SENTENCES.pick(&report)))?;
    Ok(request)
}

/// The steer's answer, or the refusal it earned, logged once with both fleets.
fn settled(
    answered: afd_turns::view::Result<Result<FleetSteered, Unheld>>,
    standing: &Standing,
    target: &Uuid7,
) -> Result<FleetSteered, Refusal> {
    match answered {
        Ok(Ok(steered)) => Ok(steered),
        Ok(Err(Unheld)) => {
            log_refused(standing, target, error_code::RUN_STALE_FENCING_TOKEN);
            Err(Refusal::conflict(
                error_code::RUN_STALE_FENCING_TOKEN,
                DETAIL_SUPERSEDED,
                STATE_SUPERSEDED,
            ))
        }
        Err(error) => {
            // A server failure is logged once, by the refusal that renders it;
            // only a refusal the caller earned is logged here as one.
            let code = error.code();
            let refusal = refusal_of(error);
            if refusal.status().is_client_error() {
                log_refused(standing, target, code);
            }
            Err(refusal)
        }
    }
}

/// The refusal a turn that did not land earns, through the accessors
/// the steer's error already carries.
fn refusal_of(error: afd_turns::view::Error) -> Refusal {
    if error.is_fleet_absent() {
        Refusal::coded(error_code::AGENTSFLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND)
    } else if let Some(status) = error.not_runnable() {
        Refusal::conflict(
            error_code::AGENTSFLEET_NOT_ACTIVE,
            DETAIL_NOT_ACTIVE,
            status.as_str(),
        )
    } else if error.is_operation_conflict() {
        Refusal::conflict(
            error_code::AGENTSFLEET_OPERATION_CONFLICT,
            DETAIL_CONFLICT,
            STATE_ADMITTED,
        )
    } else {
        Refusal::at(EVENT_FAILED)(error)
    }
}

/// Tells every screen on the target the message was admitted, before a
/// runner takes it. Once: a repeat carries no intake instant.
async fn announce<D: RunnerPlane>(
    services: &D,
    target: &Uuid7,
    standing: &Standing,
    sender: &SteerSender,
    steered: &FleetSteered,
) {
    let Some(created_at) = steered.admitted_at else {
        return;
    };
    let actor = fleet_actor(&standing.fleet_id);
    // Kept to the tail batch, as a person's steer is; the turn holds it whole.
    let message = FrameText::within(&sender.message, afd_bus::TAIL_BATCH_BYTES);
    let frame = TailFrame::TurnAdmitted {
        turn_id: Cow::Borrowed(&steered.turn_id),
        actor: Cow::Borrowed(&actor),
        event_type: Cow::Borrowed(TurnKind::Chat.as_str()),
        message: Cow::Borrowed(message.text),
        message_trimmed: message.trimmed,
        created_at,
    };
    services.steering().announce(target.as_str(), &frame).await;
}

/// Logs a refused steer by its code and both fleets; never its words.
fn log_refused(standing: &Standing, target: &Uuid7, code: ErrorCode) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let error_code = code.as_str();
    let run_id = standing.run_id.as_str();
    let sender_fleet_id = standing.fleet_id.as_str();
    let target_fleet_id = target.as_str();
    tracing::warn!(
        error_code,
        run_id,
        sender_fleet_id,
        target_fleet_id,
        event = EVENT_REFUSED
    );
}

/// Logs an admitted or repeated steer; never its words.
fn log_admitted(standing: &Standing, target: &Uuid7, steered: &FleetSteered) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let turn_id = steered.turn_id.as_str();
    let sender_fleet_id = standing.fleet_id.as_str();
    let target_fleet_id = target.as_str();
    let replayed = steered.replayed();
    tracing::info!(
        turn_id,
        sender_fleet_id,
        target_fleet_id,
        replayed,
        event = EVENT_ADMITTED
    );
}

#[cfg(test)]
mod tests;
