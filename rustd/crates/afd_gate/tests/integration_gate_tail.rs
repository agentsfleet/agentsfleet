//! What the gate plane tells the fleet's live tail: a human has been asked.
//!
//! A park writes its row and its reference and then says so on
//! `fleet:{id}:activity`, count included, so a console watching the fleet
//! shows the gate waiting without a read. Beside the lifecycle suite rather
//! than inside it, so each file stays about one property.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use std::time::Duration;

use afd_crypto::entropy::Entropy;
use afd_dragonfly::SubscriptionHub;
use afd_dragonfly::hub::Received;
use afd_gate::gate::{Gates, Verdict, Waiting};
use serde_json::json;

use self::fixture::{Fixture, NOW, config_gates, connect_redis, dragonfly_config};

/// How long the hub's pump is given to register the subscription before the
/// park publishes; `subscribe` queues the command rather than round-tripping.
const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

/// How long a published frame is given to reach the subscriber.
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// The policy that parks this fixture's event.
///
/// A write binding parked every first-encounter event until M202 and this suite
/// used one. It no longer parks anything — the standing integration grant
/// authorises the write — so the gate is opened by an authored rule instead.
/// What is under test here is the ANNOUNCEMENT, not which policy raised it.
const APPROVE_EVERY_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#;

/// A parked gate is announced on the fleet's live tail, count included.
///
/// After both writes, so a console reacting to the frame finds the row it
/// names; the count rides the frame so the console can show the gate waiting
/// without a read of its own.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_parked_gate_is_announced_on_the_fleets_live_tail() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", fixture.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    assert_eq!(
        gates
            .check(
                fixture.check("event-announced", &config_gates(APPROVE_EVERY_CHAT)),
                NOW,
            )
            .await,
        Verdict::Await(Waiting::Parked)
    );

    let received = tokio::time::timeout(FRAME_DEADLINE, tail.recv())
        .await
        .expect("the park reaches the fleet's tail")
        .expect("the subscription stays live");
    // A lag notice is not a frame this test published; failing on one is the
    // honest outcome rather than a retry that would mask a dropped park.
    let message = match received {
        Received::Message(message) => Some(message),
        Received::Lagged(_) => None,
    }
    .expect("the tail carried the park's frame, not a lag notice");
    let frame: serde_json::Value =
        serde_json::from_str(&message.payload).expect("the frame is JSON");
    assert_eq!(frame.get("kind"), Some(&json!("gate_opened")));
    assert_eq!(frame.get("event_id"), Some(&json!("event-announced")));
    assert!(
        frame
            .get("gate_id")
            .is_some_and(serde_json::Value::is_string)
    );
    assert_eq!(
        frame.get("pending_approvals"),
        Some(&json!(1)),
        "the gate just raised is the one waiting"
    );
    // The park's frame carries where the fleet stands, read on the insert's
    // own connection: the figures on the wire are the database's.
    let counters = afd_events::fleet_counters(&fixture.database, fixture.fleet.as_str())
        .await
        .expect("the counters read back");
    assert_eq!(
        frame.get("events_processed"),
        Some(&json!(counters.events_processed)),
        "gate_opened carries the fleet's event count"
    );
    assert_eq!(
        frame.get("budget_used_nanos"),
        Some(&json!(counters.budget_used_nanos)),
        "gate_opened carries the fleet's spend"
    );
    fixture.cleanup().await;
}
