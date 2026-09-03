//! `POST /v1/webhooks/svix/{fleet_id}` — the Svix-signed variant.
//!
//! The port of `webhooks/fleet.zig`'s Svix branch. The same delivery as
//! [`super::receive_route`] serves, from a sender that signs it under Svix's
//! scheme instead of the provider's own — which is why the fleet id sits in a
//! different path segment rather than behind a flag: the route IS the choice of
//! verifier, and a query parameter deciding which wall to cross would be a wall
//! a sender picks.
//!
//! # `svix-id` is the claim key, and it is a signed one
//!
//! Unlike `x-github-delivery`, this header is not an unauthenticated hint: it
//! is the FIRST field of the signed payload
//! ([`afd_webhook::vendor::svix::verify_at`]), so a captured delivery resent
//! under a fresh id no longer verifies. That is what makes it safe to key the
//! at-most-once claim on, where the App ingress had to fall back to the body's
//! digest.

use std::sync::Arc;

use afd_core::error_code;
use afd_ingress::{Delivery, Surface};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};
use afd_http::handler::{FleetPath, parse_fleet_id};

use super::verify_svix::{SvixVerified, verified_svix};
use super::{DETAIL_EVENT_HEADER, actor};

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "webhook_svix_append_failed";

/// `POST /v1/webhooks/svix/{fleet_id}`.
///
/// # Errors
/// The wall's refusals, and `UZ-WH-002` for a verified body that is not the
/// JSON document a fleet's prose can reason over.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/webhooks/svix/{fleet_id}",
    tag = afd_http::openapi::tag::WEBHOOKS,
    operation_id = "receive_svix_webhook",
    summary = "Receive a signed Svix event",
    description = concat!(
        "Receives a signed Svix event for one fleet. A valid new event ",
        "returns 202. A duplicate or an event for a paused fleet returns 200. ",
    ),
    request_body(content = serde_json::Value, description = afd_http::openapi::DELIVERY),
    params(
        afd_http::openapi::path::FleetOnly,
        ("svix-id" = String, Header, description = "Svix message identifier (used for deduplication and signature binding)."),
        ("svix-timestamp" = String, Header, description = "Unix timestamp of the Svix delivery (used for replay protection)."),
        ("svix-signature" = String, Header, description = "Space-separated list of Svix signatures (v1 scheme)."),
    ),
    responses(
        (status = 200, description = afd_http::openapi::IGNORED, body = webhook::Ignored),
        (status = 202, description = afd_http::openapi::ACCEPTED, body = webhook::Accepted),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNVERIFIED),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
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

    // Nothing above this line has read the body as anything but bytes. The
    // claim key comes back FROM the wall rather than from a second read of the
    // headers, which is what makes it the id that was signed.
    let SvixVerified {
        proven,
        delivery_id,
    } = verified_svix(&services, &fleet, &headers, body).await?;

    if !proven.binding.is_runnable() {
        return Ok((
            StatusCode::OK,
            Json(webhook::Ignored {
                ignored: webhook::REASON_FLEET_PAUSED.into(),
            }),
        )
            .into_response());
    }

    let digest = webhook::json_payload(&proven.body)
        .ok_or_else(|| Refusal::coded(error_code::WEBHOOK_MALFORMED, DETAIL_EVENT_HEADER))?;

    let appended = services
        .ingress()
        .deliver(
            Surface::Fleet,
            &proven.binding,
            &Delivery {
                event_id: &delivery_id,
                actor: &actor(proven.binding.source()),
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
