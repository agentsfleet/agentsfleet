//! `POST /v1/ingress/qstash/schedules` — a schedule falling due.
//!
//! The port of `cron/`'s fire path. This is the route that makes "the daemon
//! owns no timer" true: nothing here sleeps or ticks, and a fleet runs on a
//! schedule because the external scheduler posted a signed callback saying it
//! was time.
//!
//! # Every non-acceptance here is a 2xx, and for a sharper reason than usual
//!
//! A paused schedule, a fleet somebody stopped, a schedule since deleted — all
//! real callbacks the scheduler was correctly told to send. A 4xx would put
//! each into its retry loop, and retrying changes none of them. Worse, the
//! scheduler treats sustained failures as a reason to stop delivering, so a
//! route that answered 4xx for a paused schedule could get the WHOLE
//! deployment's schedules throttled.
//!
//! Only two things earn a refusal: a body past the cap, and a callback that did
//! not prove itself.

use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_cron::{FireTarget, VerifiedFire, verifier};
use afd_fleet_lifecycle::FleetStatus;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use crate::handler::{Refusal, webhook};
use crate::services::{FleetSchedules as _, Services};

use super::text;
/// What an accepted fire is answered with. Public wire.
use afd_api_wire::ingress::Fired;

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "schedule_fire_append_failed";

/// The scoped event a dropped fire is logged under.
const EVENT_DROPPED: &str = "schedule_fire_dropped";

/// The header the scheduler carries its signed token in.
///
/// Fixed by the sender: Upstash `QStash` signs every delivery under this name.
const HEADER_SIGNATURE: &str = "upstash-signature";

/// The refusal a callback that did not prove itself earns.
///
/// One sentence for every way it can fail. Which key, which claim, and which
/// check are facts a forger would use to narrow their search, and an honest
/// sender never sees this at all.
const DETAIL_UNVERIFIED: &str = "The schedule callback could not be verified.";

/// The reason a fire for a schedule this daemon no longer holds is dropped.
const REASON_NO_SUCH_SCHEDULE: &str = "schedule_not_found";

/// The reason a fire for a schedule nobody wants firing is dropped.
const REASON_SCHEDULE_PAUSED: &str = "schedule_paused";

/// The reason a fire naming no schedule this daemon registered is dropped.
///
/// Covers a missing header and an id the scheduler chose itself alike: neither
/// names a row here.
const REASON_NO_SCHEDULE_HEADER: &str = "schedule_header_absent";

/// The most of an unrecognised schedule id a dropped fire logs.
///
/// Room for any id this daemon or the scheduler issues, with margin; the rest
/// of an oversized header is a sender's to keep.
const MAX_LOGGED_ID_BYTES: usize = 96;

/// `POST /v1/ingress/qstash/schedules`.
///
/// # Errors
/// `UZ-WH-030` for a body past the cap, and `UZ-WH-010` for a callback that did
/// not prove itself.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/ingress/qstash/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "ingest_qstash_schedule",
    summary = "Receive a scheduled fire",
    description = concat!(
        "Receives signed schedule deliveries from Upstash QStash and queues a fleet event. ",
        "Duplicate, missing, inactive, or outdated schedules are acknowledged without ",
        "starting a run.",
    ),
    request_body(content = serde_json::Value, description = afd_http::openapi::DELIVERY),
    params(
        ("Upstash-Signature" = String, Header, description = "QStash signature over the exact request body and destination URL."),
        ("Upstash-Schedule-Id" = String, Header, description = "Schedule identifier returned by QStash: `fleet-schedule-{schedule_id}`."),
        ("Upstash-Message-Id" = String, Header, description = "QStash delivery identifier used for replay suppression."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::IGNORED, body = webhook::Ignored),
        (status = 202, description = "The schedule fired, or already had for this delivery", body = Fired),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNVERIFIED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    webhook::within_cap(&body)?;
    let proven = prove(services.as_ref(), &headers, &body)?;

    // The schedule that fell due, by the id this daemon registered it under:
    // a schedule the scheduler filed under an id of its own resolves to none.
    let named = text(&headers, afd_cron::HEADER_SCHEDULE_ID);
    let Some(schedule) = named.and_then(afd_cron::schedule_of) else {
        return Ok(unnamed(named));
    };

    let Some(target) = services
        .schedules()
        .fire_target(&schedule)
        .await
        .map_err(Refusal::at(EVENT_DROPPED))?
    else {
        return Ok(no_such_schedule(&schedule));
    };
    if let Some(dropped) = held(services.as_ref(), &schedule, &target).await? {
        return Ok(dropped);
    }

    let fired = services
        .schedules()
        .fire(&schedule, &target, &proven.message_id, services.now())
        .await
        .map_err(Refusal::at(EVENT_APPEND))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(Fired {
            event_id: fired.event_id.as_str().into(),
            replayed: fired.replayed,
        }),
    )
        .into_response())
}

/// The callback's signature, checked against this deployment's keys and
/// destination before the body is read as anything but bytes.
fn prove<D: Services>(
    services: &D,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<VerifiedFire, Refusal> {
    let token = text(headers, HEADER_SIGNATURE).unwrap_or_default();
    let Some(keys) = services.schedule_signing_keys() else {
        // No keys configured is fail-closed, and it is the same answer a forged
        // callback gets: a deployment that cannot verify must not act, and
        // telling a prober which of the two it hit would say whether this
        // daemon is misconfigured.
        return Err(unverified());
    };

    // Nothing above this line has read the body as anything but bytes.
    verifier::verify_at(keys, services.schedule_destination(), token, body).map_err(
        |refused: afd_cron::Error| {
            // Logged with the reason, answered without it — the operator needs to
            // know a key is missing; the sender must not.
            let reason = refused.refusal_word();
            tracing::warn!(
                reason,
                event = EVENT_DROPPED,
                error_code = error_code::WEBHOOK_SIGNATURE_INVALID.as_str(),
            );
            unverified()
        },
    )
}

/// The 200 a fire that must not run answers with, or `None` when it may run.
///
/// Both halves can stop a fire and they are different facts: the schedule
/// being paused is the scheduler not yet knowing, and the fleet being stopped
/// is an operator halting everything it does.
async fn held<D: Services>(
    services: &D,
    schedule: &Uuid7,
    target: &FireTarget,
) -> Result<Option<Response>, Refusal> {
    if !target.desired_status.fires() {
        return Ok(Some(dropped(REASON_SCHEDULE_PAUSED)));
    }
    if !FleetStatus::parse(&target.fleet_status).is_some_and(FleetStatus::is_runnable) {
        // A `once` schedule's one moment has passed whether or not the fleet
        // ran, and kept, it would match the same minute next year. A paused
        // SCHEDULE is the person's to resume, so only this drop retires.
        if target.once {
            services
                .schedules()
                .retire(&target.workspace, &target.fleet, schedule, services.now())
                .await
                .map_err(Refusal::at(EVENT_DROPPED))?;
        }
        return Ok(Some(dropped(webhook::REASON_FLEET_PAUSED)));
    }
    Ok(None)
}

/// The one refusal an unproven callback earns — see [`DETAIL_UNVERIFIED`].
fn unverified() -> Refusal {
    Refusal::coded(error_code::WEBHOOK_SIGNATURE_INVALID, DETAIL_UNVERIFIED)
}

/// The 200 a fire naming no schedule of this daemon's answers with, logging
/// the id it did name.
///
/// That id is the one clue an operator has: an `scd_…` here is a schedule the
/// scheduler filed under an id of its own, still registered and still firing.
/// Capped, because the header is outside the signature and its sender chooses
/// its length.
fn unnamed(named: Option<&str>) -> Response {
    let named = logged_id(named);
    tracing::info!(
        named,
        reason = REASON_NO_SCHEDULE_HEADER,
        event = EVENT_DROPPED
    );
    ignored(REASON_NO_SCHEDULE_HEADER)
}

/// The 200 a fire naming a schedule this daemon no longer holds answers with,
/// logging which schedule it named.
///
/// A deleted row whose registration survived upstream fires here, so the id
/// is what an operator needs to remove that registration. It was parsed into
/// a row id already, so it is bounded and safe to log whole.
fn no_such_schedule(schedule: &Uuid7) -> Response {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let schedule_id = schedule.as_str();
    tracing::info!(
        schedule_id,
        reason = REASON_NO_SUCH_SCHEDULE,
        event = EVENT_DROPPED
    );
    ignored(REASON_NO_SUCH_SCHEDULE)
}

/// As much of `named` as a dropped fire logs: empty when absent, and no more
/// than [`MAX_LOGGED_ID_BYTES`].
///
/// A header value read as text is visible ASCII today, but the cut still backs
/// off to a character boundary, so a wider reader can never make it return the
/// whole value uncapped.
fn logged_id(named: Option<&str>) -> &str {
    named.map_or("", |id| {
        let mut end = id.len().min(MAX_LOGGED_ID_BYTES);
        while !id.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        id.get(..end).unwrap_or_default()
    })
}

/// The 200 a deliberately-dropped fire answers with.
///
/// Logged as well as answered: the sender is a scheduler that will never look,
/// and an operator asking "why did my schedule not run" has only this line.
fn dropped(reason: &str) -> Response {
    tracing::info!(reason, event = EVENT_DROPPED);
    ignored(reason)
}

/// The body every dropped fire is answered with.
fn ignored(reason: &str) -> Response {
    (
        StatusCode::OK,
        Json(webhook::Ignored {
            ignored: reason.into(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
