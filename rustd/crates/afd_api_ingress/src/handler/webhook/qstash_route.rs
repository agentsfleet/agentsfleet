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
use afd_cron::verifier::{self, SigningKeys, Unverified};
use afd_fleet_lifecycle::FleetStatus;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use crate::handler::{Refusal, webhook};
use crate::services::{FleetSchedules as _, Services};

use super::text;
/// What an accepted fire is answered with. Public wire.
use afd_wire::ingress::Fired;

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "schedule_fire_append_failed";

/// The scoped event a dropped fire is logged under.
const EVENT_DROPPED: &str = "schedule_fire_dropped";

/// The header the scheduler carries its signed token in.
///
/// `cron/constants.zig`'s `signature_header`, kept byte-for-byte.
const HEADER_SIGNATURE: &str = "upstash-signature";

/// The header naming which schedule fell due.
const HEADER_SCHEDULE: &str = "upstash-schedule-id";

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

/// The reason a fire naming no schedule at all is dropped.
const REASON_NO_SCHEDULE_HEADER: &str = "schedule_header_absent";

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
        "Receives a signed fire from Upstash QStash and appends a `cron` ",
        "event to the target Fleet. A valid duplicate, missing, inactive, or ",
        "stale fire is accepted without appending an event. ",
    ),
    params(
        ("Upstash-Signature" = String, Header, description = "QStash signature over the exact request body and destination URL."),
        ("Upstash-Schedule-Id" = String, Header, description = "Schedule identifier supplied by QStash."),
        ("Upstash-Message-Id" = String, Header, description = "QStash delivery identifier used for replay suppression."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = Fired),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNVERIFIED),
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

    let token = text(&headers, HEADER_SIGNATURE).unwrap_or_default();
    let Some(keys) = services.schedule_signing_keys() else {
        // No keys configured is fail-closed, and it is the same answer a forged
        // callback gets: a deployment that cannot verify must not act, and
        // telling a prober which of the two it hit would say whether this
        // daemon is misconfigured.
        return Err(unverified());
    };

    // Nothing above this line has read the body as anything but bytes.
    let proven = verifier::verify_at(
        &SigningKeys {
            current: keys.current.clone(),
            next: keys.next.clone(),
        },
        services.schedule_destination(),
        token,
        &body,
    )
    .map_err(|refused: Unverified| {
        // Logged with the reason, answered without it — the operator needs to
        // know a key is missing; the sender must not.
        let reason = refused.reason();
        tracing::warn!(
            reason,
            event = EVENT_DROPPED,
            error_code = error_code::WEBHOOK_SIGNATURE_INVALID.as_str(),
        );
        unverified()
    })?;

    let Some(schedule) = text(&headers, HEADER_SCHEDULE).and_then(|id| Uuid7::parse(id).ok())
    else {
        return Ok(dropped(REASON_NO_SCHEDULE_HEADER));
    };

    let Some(target) = services
        .schedules()
        .fire_target(&schedule)
        .await
        .map_err(Refusal::at(EVENT_DROPPED))?
    else {
        return Ok(dropped(REASON_NO_SUCH_SCHEDULE));
    };

    // Both halves can stop a fire and they are different facts: the schedule
    // being paused is the scheduler not yet knowing, and the fleet being
    // stopped is an operator halting everything it does.
    if !target.desired_status.fires() {
        return Ok(dropped(REASON_SCHEDULE_PAUSED));
    }
    if !FleetStatus::parse(&target.fleet_status).is_some_and(FleetStatus::is_runnable) {
        return Ok(dropped(webhook::REASON_FLEET_PAUSED));
    }

    let fired = services
        .schedules()
        .fire(&schedule, &target, &proven.message_id)
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

/// The one refusal an unproven callback earns — see [`DETAIL_UNVERIFIED`].
fn unverified() -> Refusal {
    Refusal::coded(error_code::WEBHOOK_SIGNATURE_INVALID, DETAIL_UNVERIFIED)
}

/// The 200 a deliberately-dropped fire answers with.
///
/// Logged as well as answered: the sender is a scheduler that will never look,
/// and an operator asking "why did my schedule not run" has only this line.
fn dropped(reason: &str) -> Response {
    tracing::info!(reason, event = EVENT_DROPPED);
    (
        StatusCode::OK,
        Json(webhook::Ignored {
            ignored: reason.into(),
        }),
    )
        .into_response()
}
