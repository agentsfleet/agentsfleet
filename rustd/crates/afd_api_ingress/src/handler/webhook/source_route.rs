//! `POST /v1/webhooks/{fleet_id}/{source}` — one fleet's deliveries from the
//! source its URL names.
//!
//! The URL Save hands back for a webhook trigger whose provider this daemon
//! ships a scheme for (`afd_api_tenant`'s `render::webhook_urls`). The trigger
//! is chosen by the name in the URL, so a fleet that declares two webhook
//! sources gives each one an address of its own, and the scheme its signature
//! is checked under comes from that declaration rather than from anything the
//! sender said.
//!
//! The literal `/github` and `/approval` templates beside this one are matched
//! first, so those two names never arrive here. Neither can be declared as a
//! generic source by mistake either: `approval` is refused at parse, and a
//! `github` trigger is served by its own route.
//!
//! # There is no allow-list check here, and that is not an omission
//!
//! A trigger's `events` list is written in a provider's own event vocabulary,
//! and this route has no event to measure against it: the delivery carries a
//! body and a signature and nothing that names what KIND of thing happened.
//! A fleet that wants its deliveries filtered by kind declares a provider whose
//! route can read one — which today is the GitHub path beside this. Guessing a
//! kind from the payload's shape would let a sender holding the secret present
//! a body that classifies as an event their allow-list admits.
//!
//! # Why the claim key is the body's digest
//!
//! No header on this surface is both universal and signed. So the identity of a
//! delivery is the delivery — [`afd_ingress::replay_id`], the same
//! authenticated identity the App ingress keys on: a value the signature
//! covers cannot be varied without invalidating the proof. A sender that means
//! to fire the same fleet twice with a byte-identical body inside the claim
//! window is suppressed the second time, which is the safe direction; every
//! real payload varies by carrying its own timestamp or identifier.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::Response;
use http::HeaderMap;
use serde::Deserialize;

use crate::handler::{Refusal, webhook};
use crate::services::Services;
use afd_http::handler::parse_fleet_id;

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "webhook_source_append_failed";

/// The fleet and the webhook source a delivery's path names, still text.
#[derive(Debug, Deserialize)]
pub(crate) struct FleetSourcePath {
    /// The fleet named in the path.
    fleet_id: String,
    /// The `source` of the webhook trigger the delivery is for.
    source: String,
}

/// `POST /v1/webhooks/{fleet_id}/{source}`.
///
/// # Errors
/// The wall's refusals — `UZ-WH-001` when the fleet declares no webhook
/// trigger with this source — and `UZ-WH-002` for a verified body that is not
/// the JSON document a fleet's prose can reason over.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/webhooks/{fleet_id}/{source}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "receive_source_webhook",
    summary = "Send a signed event from one source to a fleet",
    description = concat!(
        "Sends a signed event to the fleet's webhook trigger with this source. ",
        "Every request requires a valid signature under that trigger's scheme. ",
        "A new event returns 202. A duplicate within 24 hours returns ",
        "`status: duplicate`. A paused fleet returns `ignored: fleet_paused`. ",
        "Send the event again after resuming the fleet. ",
    ),
    request_body(content = serde_json::Value, description = afd_http::openapi::DELIVERY),
    params(
        afd_http::openapi::path::FleetSource,
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
    Path(FleetSourcePath { fleet_id, source }): Path<FleetSourcePath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    webhook::within_cap(&body)?;

    // Nothing above this line has read the body as anything but bytes.
    let proven = webhook::verified(&services, &fleet, Some(&source), &headers, body).await?;

    let event_id = afd_ingress::replay_id(&proven.body);
    super::delivery::deliver(services.as_ref(), proven, &event_id, EVENT_APPEND).await
}
