//! What `decide` answers, for every shape a connector's ingress can take.
//!
//! Split out of [`super`], which is at the length cap. These cases need no
//! router and no pool: the decision is a pure function of the registry entry
//! and the envelope, which is exactly why it was written as one.

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
