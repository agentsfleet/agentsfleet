//! `POST /v1/webhooks/{fleet_id}` — one fleet's own deliveries, any provider.
//!
//! The port of `webhooks/fleet.zig`. Where [`super::github_route`] knows it is
//! serving GitHub and can read an event kind out of a header GitHub sets, this
//! route knows only which fleet the URL named. The provider is whatever the
//! fleet's trigger declares, and the scheme its signature is checked under
//! comes from that declaration rather than from anything the sender said.
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
//! No header on this surface is both universal and signed. `x-github-delivery`
//! is GitHub's alone; a generic sender may send none. So the identity of a
//! delivery is the delivery — [`afd_ingress::replay_id`], the same
//! authenticated identity the App ingress keys on, and for the same reason: a
//! value the signature covers cannot be varied without invalidating the proof.
//!
//! The cost is recorded rather than hidden. A sender that means to fire the
//! same fleet twice with a byte-identical body inside the claim window is
//! suppressed the second time. That is the safe direction — a duplicate run
//! spends a model on work already done — and a sender that means two runs can
//! vary the body, which every real payload does by carrying its own timestamp
//! or identifier.

use std::sync::Arc;

use afd_core::error_code;
use afd_ingress::{Delivery, Surface};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::{HeaderMap, StatusCode};

use afd_http::handler::{FleetPath, parse_fleet_id};
use crate::handler::{Refusal, webhook};
use crate::services::{Services, WebhookIngress as _};

use super::{DETAIL_EVENT_HEADER, actor};

/// The scoped event a failed append is logged under.
const EVENT_APPEND: &str = "webhook_append_failed";

/// `POST /v1/webhooks/{fleet_id}`.
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
    webhook::within_cap(&body)?;

    // Nothing above this line has read the body as anything but bytes.
    let proven = webhook::verified(&services, &fleet, &headers, body).await?;

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

    let event_id = afd_ingress::replay_id(&proven.body);
    let appended = services
        .ingress()
        .deliver(
            Surface::Fleet,
            &proven.binding,
            &Delivery {
                event_id: &event_id,
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
