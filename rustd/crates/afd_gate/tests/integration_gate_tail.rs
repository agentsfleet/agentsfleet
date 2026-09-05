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
use afd_gate::gate::{Gates, Verdict, Waiting};
use afd_redis::SubscriptionHub;
use afd_redis::hub::Received;
use serde_json::json;

use self::fixture::{Fixture, NOW, config, connect_redis, redis_config};

/// How long the hub's pump is given to register the subscription before the
/// park publishes; `subscribe` queues the command rather than round-tripping.
const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

/// How long a published frame is given to reach the subscriber.
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// A parked gate is announced on the fleet's live tail, count included.
///
/// After both writes, so a console reacting to the frame finds the row it
/// names; the count rides the frame so the console can show the gate waiting
/// without a read of its own.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_parked_gate_is_announced_on_the_fleets_live_tail() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", fixture.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    assert_eq!(
        gates
            .check(fixture.check("event-announced", &config(true)), NOW)
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
    fixture.cleanup().await;
}
