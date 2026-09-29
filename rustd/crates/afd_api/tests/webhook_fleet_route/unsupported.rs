//! A delivery the fleet route verifies and reads, then has no rule for.
//!
//! GitHub sends a `ping` the moment a hook is created. It is signed and
//! well-formed, so it is neither a forgery nor the sender's bug: the route
//! answers 200 with the reason it dropped it, and nothing reaches the stream.
//! A 400 would tell the operator their signature was wrong; a 5xx would earn a
//! retry for a delivery no retry can change.

#![cfg(feature = "test-util")]

use super::*;

/// The event GitHub sends when a hook is created.
const EVENT_PING: &str = "ping";

/// A real `ping` delivery.
const PING_DELIVERY: &str = include_str!("../../../../../tests/fixtures/webhooks/github_ping.json");

/// The reason the route drops an event it serves no rule for.
const REASON_UNSUPPORTED: &str = "unsupported_event";

#[tokio::test]
async fn a_signed_event_with_no_rule_is_dropped_with_its_reason() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let response = deliver(&ingress, EVENT_PING, signed::DELIVERY_ID, PING_DELIVERY).await;

    assert_eq!(ignored_reason(response).await, REASON_UNSUPPORTED);
    assert!(
        ingress.deliveries().is_empty(),
        "an event this daemon serves no rule for must not reach the stream"
    );
}
