//! §1's receive half against live Postgres and Redis: an event that parks is
//! still counted, and moves no budget.
//!
//! The receive is the one frame whose counters cannot ride its own statement:
//! the insert that writes the row is what fires the counter trigger, and a
//! `RETURNING` on that insert does not see the trigger's write. So the pull
//! reads the counters after the row landed and hands them to the frame — and
//! an event that then PARKS behind a gate never closes, so that frame is the
//! only word the tail hears about it. If it carried the old count, the tile
//! would show a fleet that never received the event a human is being asked
//! about. Proven against the lane because the claim is about the trigger's
//! timing relative to the read, which no stub reproduces.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::Delivery;
use afd_redis::hub::Received;
use afd_redis::streams::{FleetStreams, fleet_activity_channel};
use afd_redis::{Subscription, SubscriptionHub};
use afd_wire::tail::FleetCounters;
use sqlx::Row as _;

use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{Seeded, seeded};
use crate::support::Fixtures;

/// How long the tail is given to deliver a frame it was sent.
const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// How long the tail is given to prove it has NOTHING more to say.
const SILENCE_BUDGET: Duration = Duration::from_millis(500);

/// The frame kind the receive publishes.
const EVENT_RECEIVED: &str = "event_received";

/// The frame kind a closing would publish, and which a park never does.
const EVENT_COMPLETE: &str = "event_complete";

/// The probe the test publishes until Redis reports a subscriber.
const PROBE: &str = r#"{"kind":"probe"}"#;

/// A seeded fleet whose narrative log this test opens and then leaves parked.
struct Parked {
    fixtures: Fixtures,
    fleet: String,
    tail: Subscription,
    counters: FleetCounters,
}

/// Leases a seeded event, records its receive, and reads the counters the
/// pull would hand the frame — the row is then left open, which is what a
/// park is from the tail's point of view: nothing closes it.
async fn parked() -> Parked {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let hub = SubscriptionHub::start(queue::config())
        .await
        .expect("a live hub");
    let tail = hub.subscribe(&fleet_activity_channel(&fleet));
    await_subscribed(&fixtures, &fleet).await;

    let now = UnixMillis::from_millis(ENROLLED_AT);
    let leases = fixtures.leases();
    let held = leases
        .select(&runner, now)
        .await
        .expect("the selection pass must not fault")
        .expect("the fleet is leasable");
    assert_eq!(
        leases
            .record_received(&held, now)
            .await
            .expect("the narrative log must open"),
        Delivery::First
    );
    let counters = afd_events::fleet_counters(&fixtures.database, &fleet)
        .await
        .expect("the counters read after the row landed");
    leases.publish_received(&held, now, Some(counters)).await;

    Parked {
        fixtures,
        fleet,
        tail,
        counters,
    }
}

/// Publishes a probe until Redis reports one subscriber on the channel, so a
/// frame published afterwards cannot be lost to a subscription still in flight.
async fn await_subscribed(fixtures: &Fixtures, fleet: &str) {
    let publisher = FleetStreams::new(fixtures.queue().clone());
    let channel = fleet_activity_channel(fleet);
    tokio::time::timeout(DELIVERY_BUDGET, async {
        while publisher
            .publish(&channel, PROBE)
            .await
            .expect("the probe publishes")
            < 1
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Redis acknowledges the subscription");
}

/// The next non-probe frame on the tail, decoded.
async fn next_frame(tail: &mut Subscription) -> serde_json::Value {
    tokio::time::timeout(DELIVERY_BUDGET, async {
        loop {
            let Received::Message(message) = tail.recv().await.expect("the hub is up") else {
                continue;
            };
            let frame: serde_json::Value =
                serde_json::from_str(&message.payload).expect("a frame is JSON");
            if frame.pointer("/kind").and_then(serde_json::Value::as_str) != Some("probe") {
                return frame;
            }
        }
    })
    .await
    .expect("the tail delivers within budget")
}

/// Dimension 1.3. The receive frame carries the count the insert moved, and
/// no completion follows it: a parked event leaves the tile showing the event
/// it counted.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_parked_event_still_reports_its_event_count() {
    let mut run = parked().await;

    assert_eq!(
        run.counters.events_processed, 1,
        "the trigger moved the count on the insert, before any frame"
    );
    let frame = next_frame(&mut run.tail).await;
    assert_eq!(
        frame.pointer("/kind").and_then(serde_json::Value::as_str),
        Some(EVENT_RECEIVED)
    );
    assert_eq!(
        frame.pointer("/events_processed"),
        Some(&serde_json::json!(1)),
        "the frame carries the incremented count, not the one before the insert"
    );
    assert_eq!(
        frame.pointer("/budget_used_nanos"),
        Some(&serde_json::json!(0))
    );

    // Nothing closes a parked event, so the tail hears nothing more — and in
    // particular never an `event_complete` for it.
    let silence = tokio::time::timeout(SILENCE_BUDGET, run.tail.recv()).await;
    assert!(
        silence.is_err(),
        "a park publishes no completion, but the tail heard: {silence:?}"
    );
    let _ = EVENT_COMPLETE;
}

/// Dimension 1.4. The park writes no ledger row and moves no budget — it is
/// durably a retry, not a charge — and the row it leaves is still open.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_parked_event_moves_no_budget() {
    let run = parked().await;
    let fleet = Uuid7::parse(&run.fleet).expect("the seeded fleet id is well formed");
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");

    let ledger_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM billing.usage_ledger WHERE fleet_id = $1::uuid")
            .bind(fleet.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger answers a count");
    assert_eq!(ledger_rows, 0, "a park charges nothing");

    let after = afd_events::fleet_counters(&run.fixtures.database, &run.fleet)
        .await
        .expect("the counters read");
    assert_eq!(
        after.budget_used_nanos, 0,
        "no ledger row, so no budget moved"
    );
    assert_eq!(after, run.counters, "nothing moved since the receive");

    let status: String = sqlx::query(
        "SELECT status FROM core.fleet_events WHERE fleet_id = $1::uuid ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fleet.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the event row reads")
    .try_get(0)
    .expect("status is text");
    assert_eq!(
        status,
        afd_core::event::status::RECEIVED,
        "the parked row is still open — a retry, not a settled charge"
    );
}
