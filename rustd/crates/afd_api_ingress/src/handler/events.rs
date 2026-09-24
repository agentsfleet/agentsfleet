//! `POST /v1/connectors/{provider}/events` — a connector's inbound deliveries.
//!
//! The return direction of the connector family. Every other route here is this
//! deployment CALLING a provider or a person configuring one; this is a
//! provider calling us, and it is authenticated the way every inbound surface
//! in this daemon is — by a signature over the body, and by nothing else. No
//! bearer, no session, and no workspace in the path to read a secret for: the
//! secret belongs to the app registration this deployment owns, one for every
//! workspace that installed it.
//!
//! # Nothing here names a provider
//!
//! The path segment is parsed to a [`Provider`] at the edge, and everything
//! after that reads from the registry:
//!
//! ```text
//!   {provider} ──► Provider::event_ingress()  ──► None  → UZ-CONN-004
//!                          │
//!                          ├─ Scheme::for_source(provider.id())  the wall
//!                          ├─ signing_secret(admin, provider)    the secret
//!                          └─ Handshake                          the answer
//! ```
//!
//! So a connector that delivers events is one arm in
//! [`afd_connector::Provider::event_ingress`] plus its
//! [`afd_webhook::Scheme`] entry. This file does not change, and neither does
//! the route: `/v1/connectors/slack/events` is what Slack posts to today, and
//! the template that serves it serves the next one too.
//!
//! # What this route carries
//!
//! The wall and the handshake for every connector that delivers events, and
//! the producer its registry entry names. A chat connector's
//! [`EventProducer::Mention`] hands the proved body to
//! [`super::mention`], which routes a mention of the bot to one fleet and
//! admits it; every other event is acknowledged with the reason it was not
//! acted on. The producer is chosen by the registry entry, not by name, so
//! this file still names no provider.
//!
//! # Why almost every answer is a 200
//!
//! A provider retries a delivery it did not get a 2xx for, and disables an
//! endpoint that keeps failing. Past the signature there is nothing a sender
//! could fix by being told — a body this daemon does not act on is not the
//! sender's mistake — so everything the wall passes is acknowledged with its
//! reason logged. `events.zig` states the same rule: an error status "would
//! make Slack retry-loop the same delivery".
//!
//! The exceptions are the three things a sender or an operator genuinely
//! controls: a path naming no connector, a body past the cap, and a signature
//! that did not verify.

use std::sync::Arc;

use afd_connector::Provider;
use afd_connector::registry::{EventIngress, EventProducer, Handshake};
use afd_core::error_code;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode};

use crate::handler::mention::{self, Outcome, Parsed};
use crate::handler::{Refusal, webhook};
use crate::services::Services;

use super::provider_of;
/// The two documents a 200 carries. Public wire.
use afd_wire::ingress::{EchoAnswer, EventsAnswer};

/// The scoped event a dropped delivery is logged under.
const EVENT_DROPPED: &str = "connector_events_dropped";

/// The refusal a connector that delivers no events earns.
///
/// Distinct from an unknown provider: this one IS shipped and connectable, it
/// simply has no inbound surface. An operator who pointed a vendor's webhook
/// configuration at the wrong connector reads the difference in the code.
const DETAIL_NO_EVENT_INGRESS: &str = "This connector delivers no events.";

/// The reason a signed body that is not a JSON object is dropped.
pub(super) const REASON_UNREADABLE: &str = "unreadable_body";

/// The reason a handshake carrying no value to echo is dropped.
///
/// Answered as a drop rather than an echo of nothing: a provider that sent the
/// handshake kind without its value gets no proof of ownership either way, and
/// returning an empty string would look like one.
const REASON_HANDSHAKE_EMPTY: &str = "handshake_missing_echo_value";

/// What a proved delivery earns, before any of it is rendered.
///
/// A verdict rather than a `Response`, for the reason
/// [`afd_webhook::Verdict`] is one: the decision is a pure function of the
/// registry entry and the envelope, and a function that returned a rendered
/// response could only be tested by parsing HTTP back out of itself. Every
/// branch below is reachable in a unit test with no router, no datastore and no
/// signature.
#[derive(Debug, PartialEq, Eq)]
enum Answer<'a> {
    /// Echo `value` back under `field`, which is what proves this endpoint.
    Echo {
        /// The key the provider looks for in the response.
        field: &'a str,
        /// Exactly the bytes the request carried under that key.
        value: &'a str,
    },
    /// Acknowledge and drop, recording why.
    Drop(&'static str),
}

/// What a verified envelope earns.
///
/// Total over [`Handshake`], so a provider whose handshake this daemon has no
/// arm for fails the build rather than falling through to a drop.
fn decide<'e>(ingress: &EventIngress, envelope: &'e serde_json::Value) -> Answer<'e> {
    let Handshake::Echo(echo) = ingress.handshake else {
        // The provider performs no handshake, so every delivery it sends is a
        // real one, and deciding what a real one becomes is the producer's.
        return Answer::Drop(webhook::REASON_UNSUPPORTED_EVENT);
    };

    if field(envelope, echo.type_field) != Some(echo.type_value) {
        return Answer::Drop(webhook::REASON_UNSUPPORTED_EVENT);
    }

    // The handshake kind with no value under it proves nothing either way, and
    // echoing an empty string would look like it had.
    field(envelope, echo.echo_field).map_or(Answer::Drop(REASON_HANDSHAKE_EMPTY), |value| {
        Answer::Echo {
            field: echo.echo_field,
            value,
        }
    })
}

/// One non-empty string field of an envelope.
///
/// Absent, not a string, and present-and-empty are one answer: none of the
/// three is a value this route can act on, and telling them apart would be
/// three branches with one outcome.
fn field<'e>(envelope: &'e serde_json::Value, name: &str) -> Option<&'e str> {
    envelope
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

/// `POST /v1/connectors/{provider}/events`.
///
/// # Errors
/// `UZ-CONN-004` for a path naming no shipped connector, or one that delivers
/// no events; `UZ-WH-030` for a body past the cap; and the wall's own
/// refusals — `UZ-WH-020` for a deployment holding no signing secret for this
/// connector, `UZ-WH-010` for a signature that did not match, `UZ-WH-011` for
/// one outside its window.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/connectors/{provider}/events",
    tag = afd_http::openapi::tag::CONNECTORS,
    operation_id = "slack_events",
    summary = "Receive Slack events",
    description = concat!(
        "Slack sends signed events to this route. Users do not call this ",
        "route. An invalid signature returns 401 `UZ-WH-010`. A timestamp ",
        "outside 5 minutes returns 401 `UZ-WH-011`. When `agentsfleet` accepts ",
        "a mention of the bot, it returns the event identifier. For any other ",
        "delivery, `agentsfleet` returns the reason it ignored the delivery.",
    ),
    request_body(content = serde_json::Value, description = afd_http::openapi::DELIVERY),
    params(
        afd_http::openapi::path::Provider,
        ("X-Slack-Signature" = String, Header, description = "Slack request signature. Slack supplies this value."),
        ("X-Slack-Request-Timestamp" = String, Header, description = "Delivery time in Unix seconds. Values outside 5 minutes are rejected."),
    ),
    responses(
        (status = 200, description = "A handshake echo, an ignored delivery with its reason, or an accepted mention with its event identifier", body = EventsAnswer),
        (status = 401, description = afd_http::openapi::UNVERIFIED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn receive<D: Services>(
    State(services): State<Arc<D>>,
    Path(segment): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let provider = provider_of(&segment)?;

    // Before the hash and before the vault, so a path naming a connector with
    // no inbound surface costs a match rather than a decrypt — and a prober
    // cannot measure which connectors this deployment has configured by timing
    // the difference.
    let Some(ingress) = provider.event_ingress() else {
        return Err(Refusal::coded(
            error_code::CONNECTOR_UNKNOWN,
            DETAIL_NO_EVENT_INGRESS,
        ));
    };

    webhook::within_cap(&body)?;

    // Nothing above this line has read the body as anything but bytes.
    let proven = webhook::verified_connector_events(&services, provider, &headers, body).await?;

    match ingress.producer {
        EventProducer::Mention => mentioned(&*services, provider, &ingress, &proven.body).await,
    }
}

/// Answers a proved delivery for a connector whose events are mentions.
///
/// A signed body that will not parse is acknowledged, not refused: the sender
/// is already authenticated, so a 4xx would retry-loop a delivery that will
/// parse no better the second time. The handshake is echoed only here, on this
/// side of the wall, so an unverified echo cannot confirm the path to a prober
/// or reflect bytes of its choosing. Any other envelope is parsed as a mention
/// and, past the drops, routed and admitted.
///
/// # Errors
/// A datastore that would not answer while resolving or admitting.
async fn mentioned<D: Services>(
    services: &D,
    provider: Provider,
    ingress: &EventIngress,
    body: &Bytes,
) -> Result<Response, Refusal> {
    let Ok(envelope) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Ok(dropped(provider, body, REASON_UNREADABLE));
    };
    if let Answer::Echo { field, value } = decide(ingress, &envelope) {
        return Ok(echoed(field, value));
    }
    let asked = match mention::parse(envelope) {
        Parsed::Asked(asked) => asked,
        Parsed::Dropped(reason) => return Ok(dropped(provider, body, reason)),
    };
    Ok(match mention::admit(services, provider, &asked).await? {
        Outcome::Accepted(accepted) => {
            (StatusCode::OK, Json(EventsAnswer::Accepted(accepted))).into_response()
        }
        Outcome::Dropped(reason) => dropped(provider, body, reason),
    })
}

/// The 200 a handshake is answered with: the value, under its own field.
fn echoed(field: &str, value: &str) -> Response {
    (
        StatusCode::OK,
        Json(EventsAnswer::Echo(EchoAnswer {
            field: std::iter::once((field, value)).collect(),
        })),
    )
        .into_response()
}

/// The 200 a deliberately-dropped delivery answers with.
///
/// Logged as well as answered: the sender is an app that will never read this
/// response body, and an operator asking "why did my mention do nothing" has
/// only this line. The delivery's own bytes are NOT logged — Invariant 5 — so
/// the event carries the reason and a length, which is enough to tell a flood
/// of empty retries from a flood of real deliveries.
fn dropped(provider: Provider, body: &Bytes, reason: &str) -> Response {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let provider_id = provider.id();
    let body_bytes = body.len();
    tracing::info!(
        provider = provider_id,
        body_bytes,
        reason,
        event = EVENT_DROPPED,
    );
    (
        StatusCode::OK,
        Json(EventsAnswer::Ignored(webhook::Ignored {
            ignored: reason.into(),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
