//! `POST /v1/webhooks/{fleet_id}/github` — one fleet's own GitHub deliveries.
//!
//! The port of `webhooks/github.zig`. It reads the delivery's kind from the
//! header GitHub sets, hands the verified body to [`super::github::classify`],
//! and turns the three answers into the two responses this surface has.
//!
//! # Why the event kind comes from a header and never from the body
//!
//! `x-github-event` is what GitHub says the delivery IS. Inferring it from the
//! payload's shape would let a sender who holds the signing secret present a
//! body that classifies as one event while the header says another — and the
//! allow-list a fleet author wrote is checked against the header.
//!
//! # Every non-acceptance here is a 2xx
//!
//! A green build, a label edit, a repair branch, an event this daemon serves no
//! rule for, a paused fleet: all real deliveries, all correctly signed, none of
//! them waking anything. They answer 200 with a reason. A 4xx would put each in
//! GitHub's redelivery queue, where retrying changes none of them. Only the
//! wall's own refusals are errors, and they are answered before this file runs.

use std::sync::Arc;

use afd_core::error_code;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};
use afd_http::handler::{FleetPath, parse_fleet_id};
use webhook::{DETAIL_EVENT_HEADER, HEADER_EVENT, text};

use super::github::{Ingest, Policy, classify};

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "webhook_github_append_failed";

/// `POST /v1/webhooks/{fleet_id}/github`.
///
/// # Errors
/// The wall's refusals, and `UZ-WH-002` for a verified body that is not the
/// event its own header claims.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/webhooks/{fleet_id}/github",
    tag = afd_http::openapi::tag::WEBHOOKS,
    operation_id = "github_webhook",
    summary = "Receive a signed GitHub event",
    description = concat!(
        "Verifies the body with the `webhook_secret` field from the ",
        "workspace's `github` secret. Reviewable `pull_request` events and ",
        "failed completed `workflow_run` events create fleet events. A ",
        "delivery identifier remains reserved for 72 hours after acceptance. ",
    ),
    params(
        afd_http::openapi::path::FleetOnly,
        ("X-GitHub-Event" = String, Header, description = "GitHub event type."),
        ("X-GitHub-Delivery" = String, Header, description = "GitHub-issued delivery UUID. Used as the dedupe key."),
        ("X-Hub-Signature-256" = String, Header, description = "HMAC-SHA256 of the raw body, prefixed with `sha256=`."),
    ),
    responses(
        (status = 200, description = "A correctly signed delivery this fleet does not act on, and why", body = webhook::Ignored),
        (status = 202, description = afd_http::openapi::ACCEPTED, body = webhook::Accepted),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    webhook::within_cap(&body)?;
    // The claim key is the BODY's digest, and never the delivery header.
    //
    // GitHub signs the body alone, so `x-github-delivery` is unauthenticated:
    // anyone able to resend a captured signed payload can vary that header
    // freely, and keying on it would mint a fresh claim per variation — the
    // same event running the fleet again for every value the resender picks,
    // which is precisely what replay suppression exists to prevent. Preferring
    // it "when present" does not help, because present is the attacker's
    // choice to make.
    //
    // The digest is inside what the signature covers, so it is distinct per
    // payload and cannot be moved without breaking verification. GitHub's own
    // redelivery resends the identical body, so a genuine retry still lands on
    // the identical key and is still suppressed. `app_route` keys on this same
    // digest, unconditionally, for the same reason.
    let delivery_id = afd_ingress::replay_id(&body);
    let event = text(&headers, HEADER_EVENT)
        .ok_or_else(|| Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER))?
        .to_owned();

    // Nothing above this line has read the body as anything but bytes.
    let proven = webhook::verified(&services, &fleet, &headers, body).await?;

    // Checked against the HEADER's word for the delivery, before the payload is
    // parsed: an author's allow-list is written in GitHub's event vocabulary.
    if !proven.binding.admits(&event) {
        return Ok(ignored(webhook::REASON_EVENT_NOT_SUBSCRIBED));
    }
    if !proven.binding.is_runnable() {
        return Ok(ignored(webhook::REASON_FLEET_PAUSED));
    }

    let ingest =
        classify(Policy::Manual, &event, &proven.body, services.now()).map_err(|_unreadable| {
            Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER)
        })?;

    let digest = match ingest {
        Ingest::Accept(digest) => digest,
        Ingest::Ignore(reason) => return Ok(ignored(reason)),
        Ingest::Unsupported => return Ok(ignored(webhook::REASON_UNSUPPORTED_EVENT)),
    };

    let appended = services
        .ingress()
        .deliver(
            afd_ingress::Surface::Fleet,
            &proven.binding,
            &afd_ingress::Delivery {
                event_id: &delivery_id,
                actor: &webhook::actor(proven.binding.source()),
                request_json: &digest,
            },
        )
        .await
        .map_err(Refusal::at(EVENT_APPEND))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(webhook::Accepted {
            event_id: appended.id.as_str().into(),
            replayed: appended.replayed,
        }),
    )
        .into_response())
}

/// The 200 a deliberately-dropped delivery answers with.
fn ignored(reason: &'static str) -> Response {
    (
        StatusCode::OK,
        Json(webhook::Ignored {
            ignored: reason.into(),
        }),
    )
        .into_response()
}
