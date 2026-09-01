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
//! # What this route carries, and what it deliberately does not
//!
//! The wall and the handshake. A delivery whose signature verifies is
//! acknowledged; what would ACT on it is not built here. That omission is the
//! spec's rather than an oversight — *"Slack bot behaviour beyond event
//! ingress (the reactive bot surface is its own product track)"* is Out of
//! Scope, and Product Clarity §6 says this milestone builds no new event
//! producers. Resolving a delivery to a fleet is both.
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
use afd_connector::registry::{EventIngress, Handshake};
use afd_core::error_code;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode};

use crate::handler::{Refusal, webhook};
use crate::services::Services;

use super::provider_of;
/// The echo a handshake is answered with. Public wire.
use afd_wire::ingress::EchoAnswer;

/// The scoped event a dropped delivery is logged under.
const EVENT_DROPPED: &str = "connector_events_dropped";

/// The refusal a connector that delivers no events earns.
///
/// Distinct from an unknown provider: this one IS shipped and connectable, it
/// simply has no inbound surface. An operator who pointed a vendor's webhook
/// configuration at the wrong connector reads the difference in the code.
const DETAIL_NO_EVENT_INGRESS: &str = "This connector delivers no events.";

/// The reason a signed delivery this milestone serves no producer for is
/// dropped.
///
/// Distinct from [`webhook::REASON_UNSUPPORTED_EVENT`], which means "a kind no
/// rule matches". This means the opposite: the delivery is understood, and what
/// would act on it is a product track this milestone does not build. An
/// operator reading the two apart is the difference between "check your
/// subscription" and "that feature is not here yet".
const REASON_NO_PRODUCER: &str = "event_producer_not_ported";

/// The reason a signed body that is not a JSON object is dropped.
const REASON_UNREADABLE: &str = "unreadable_body";

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
        // real one — and this milestone builds nothing that acts on one.
        return Answer::Drop(REASON_NO_PRODUCER);
    };

    if field(envelope, echo.type_field) != Some(echo.type_value) {
        return Answer::Drop(REASON_NO_PRODUCER);
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
        "route. An invalid signature returns 401 `UZ-SLK-010`. A timestamp ",
        "outside 5 minutes returns 401 `UZ-SLK-011`. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 500, description = afd_http::openapi::INTERNAL),
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

    Ok(answer(provider, &ingress, &proven.body))
}

/// Renders what a verified delivery earned.
///
/// Split from [`receive`] so the wall crossing and the reading of a proved body
/// are the two separate concerns they are: nothing here can reach a body that
/// has not passed, because [`webhook::verified_connector_events`] is the only
/// constructor of the type carrying one.
///
/// A handshake is echoed only on this side of the wall. An unverified echo
/// would confirm the path exists to anybody who guessed it, and would let a
/// prober use this daemon to reflect bytes of their choosing.
fn answer(provider: Provider, ingress: &EventIngress, body: &Bytes) -> Response {
    // A signed body that will not parse is acknowledged, not refused. The
    // sender is already authenticated, so a 4xx would retry-loop a delivery
    // that will parse no better the second time.
    let Ok(envelope) = serde_json::from_slice::<serde_json::Value>(body) else {
        return dropped(provider, body, REASON_UNREADABLE);
    };

    match decide(ingress, &envelope) {
        Answer::Echo { field, value } => (
            StatusCode::OK,
            Json(EchoAnswer {
                field: std::iter::once((field, value)).collect(),
            }),
        )
            .into_response(),
        Answer::Drop(reason) => dropped(provider, body, reason),
    }
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
        Json(webhook::Ignored {
            ignored: reason.into(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use afd_connector::Provider;
    use afd_connector::registry::{Echo, EventIngress, Handshake};

    use super::{Answer, REASON_HANDSHAKE_EMPTY, REASON_NO_PRODUCER, decide};

    /// The envelope a case hands the decision.
    ///
    /// A body that is not JSON never reaches [`decide`] — [`super::answer`]
    /// drops it first — so a fixture that will not parse reads as the null
    /// document, which is a value the decision has to handle anyway.
    fn envelope(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
    }

    /// Slack's own descriptor, read from the registry rather than restated.
    ///
    /// A literal here would keep passing on the day the registry's field names
    /// changed, and the suite would report green over a daemon Slack could no
    /// longer verify (RULE TFX).
    ///
    /// Answers an `Option`, and every case asserts the whole one, because this
    /// crate's in-source tests carry the daemon's own restriction set: no
    /// `unwrap`, no `expect`, no `panic!`. A `None` means Slack stopped
    /// declaring an event ingress, and it fails the comparison loudly rather
    /// than being unwrapped out of sight.
    fn slack() -> Option<EventIngress> {
        Provider::Slack.event_ingress()
    }

    /// The handshake is answered with the value it carried, under its own key.
    #[test]
    fn test_the_handshake_echoes_the_value_it_asked_for() {
        let body = envelope(r#"{"type":"url_verification","challenge":"3eZbrw1a","token":"x"}"#);

        assert_eq!(
            slack().map(|ingress| decide(&ingress, &body)),
            Some(Answer::Echo {
                field: "challenge",
                value: "3eZbrw1a",
            }),
            "the key is the registry's, not this handler's — a vendor naming it \
             something else is data"
        );
    }

    /// Every shape carrying no value to echo is a drop, not an empty echo.
    ///
    /// An empty string in the response reads to the vendor exactly like a
    /// successful proof of ownership, so the one thing this must never do is
    /// answer the handshake shape with nothing in it.
    #[test]
    fn test_no_valueless_handshake_is_answered_as_one() {
        let valueless = [
            r#"{"type":"url_verification"}"#,
            r#"{"type":"url_verification","challenge":""}"#,
            r#"{"type":"url_verification","challenge":null}"#,
            r#"{"type":"url_verification","challenge":42}"#,
            r#"{"type":"url_verification","challenge":{"a":"b"}}"#,
        ];

        for raw in valueless {
            let body = envelope(raw);
            assert_eq!(
                slack().map(|ingress| decide(&ingress, &body)),
                Some(Answer::Drop(REASON_HANDSHAKE_EMPTY)),
                "`{raw}` carries nothing to echo"
            );
        }
    }

    /// A real delivery is acknowledged and dropped, naming the absent producer.
    ///
    /// The reason is the load-bearing half: an operator asking why a mention
    /// did nothing needs to read "not built yet" rather than "not subscribed".
    #[test]
    fn test_a_real_delivery_is_dropped_for_the_producer_that_is_not_ported() {
        let deliveries = [
            r#"{"type":"event_callback","event":{"type":"app_mention"}}"#,
            r#"{"type":"event_callback"}"#,
            r#"{"type":"something_new"}"#,
            "{}",
            "[]",
            r#""a string""#,
        ];

        for raw in deliveries {
            let body = envelope(raw);
            assert_eq!(
                slack().map(|ingress| decide(&ingress, &body)),
                Some(Answer::Drop(REASON_NO_PRODUCER)),
                "`{raw}` is a delivery, not the handshake"
            );
        }
    }

    /// A provider that performs no handshake echoes nothing, whatever arrives.
    ///
    /// The arm a connector added without one falls into. Proven with a body
    /// that WOULD be Slack's handshake, so a `Handshake::None` provider leaking
    /// into the echo path is caught rather than merely untested.
    #[test]
    fn test_a_provider_with_no_handshake_echoes_nothing() {
        let silent = EventIngress {
            handshake: Handshake::None,
        };
        let body = envelope(r#"{"type":"url_verification","challenge":"3eZbrw1a"}"#);

        assert_eq!(
            decide(&silent, &body),
            Answer::Drop(REASON_NO_PRODUCER),
            "a provider that proves nothing at setup has no handshake to answer"
        );
    }

    /// The field names are read per provider, not baked into the decision.
    ///
    /// The generality claim, asserted rather than described: a descriptor
    /// naming different fields is answered on those fields, and Slack's names
    /// mean nothing to it. This is what makes a second echo-handshake connector
    /// a registry arm instead of a branch in this file.
    #[test]
    fn test_the_decision_reads_whatever_fields_the_descriptor_names() {
        let elsewhere = EventIngress {
            handshake: Handshake::Echo(Echo {
                type_field: "kind",
                type_value: "verify_endpoint",
                echo_field: "nonce",
            }),
        };

        let its_own = envelope(r#"{"kind":"verify_endpoint","nonce":"abc123"}"#);
        assert_eq!(
            decide(&elsewhere, &its_own),
            Answer::Echo {
                field: "nonce",
                value: "abc123",
            }
        );

        let slacks = envelope(r#"{"type":"url_verification","challenge":"x"}"#);
        assert_eq!(
            decide(&elsewhere, &slacks),
            Answer::Drop(REASON_NO_PRODUCER),
            "Slack's field names mean nothing to a descriptor naming others"
        );
    }

    /// Exactly the connectors that declare an ingress are the ones that answer.
    ///
    /// Walks [`Provider::ALL`] so a connector added without a decision about
    /// its inbound surface shows up here rather than as a 404 somebody meets in
    /// production. Slack is named because it is the only one today; the
    /// assertion is over the whole roster, not over Slack.
    #[test]
    fn test_only_the_connectors_that_deliver_declare_an_ingress() {
        let delivering: Vec<&str> = Provider::ALL
            .iter()
            .copied()
            .filter(|provider| provider.event_ingress().is_some())
            .map(Provider::id)
            .collect();

        assert_eq!(
            delivering,
            vec!["slack"],
            "a connector that started delivering needs an `event_ingress` arm \
             and an `afd_webhook::Scheme` entry; one that stopped needs the \
             arm removed"
        );
    }
}
