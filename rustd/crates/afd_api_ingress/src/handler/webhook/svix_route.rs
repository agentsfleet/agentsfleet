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
use afd_webhook::vendor::svix;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use afd_http::handler::{FleetPath, parse_fleet_id};
use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};

use super::verify_svix::verified_svix;
use super::{DETAIL_EVENT_HEADER, actor, text};

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "webhook_svix_append_failed";

/// `POST /v1/webhooks/svix/{fleet_id}`.
///
/// # Errors
/// The wall's refusals, and `UZ-WH-002` for a verified body that is not the
/// JSON document a fleet's prose can reason over.
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let delivery_id = text(&headers, svix::ID_HEADER)
        .unwrap_or(&fleet_id)
        .to_owned();
    webhook::within_cap(&body)?;

    // Nothing above this line has read the body as anything but bytes.
    let proven = verified_svix(&services, &fleet, &headers, body).await?;

    if !proven.binding.is_runnable() {
        return Ok((
            StatusCode::OK,
            Json(webhook::Ignored {
                ignored: webhook::REASON_FLEET_PAUSED,
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
            event_id: appended.id.as_str(),
            replayed: appended.replayed,
        }),
    )
        .into_response())
}
