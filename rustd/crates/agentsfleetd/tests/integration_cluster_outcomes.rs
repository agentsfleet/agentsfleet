//! Dimension 6.1 — the golden paths, and what they answer on a sharded store.
//!
//! # What this asserts that the sibling e2e suite does not
//!
//! `test_runner_suite_vs_rust_daemon` proves WIRING: that the verb is served
//! on the path a runner sends to, that the guard and the store scope by one
//! identity, that a payload round-trips. None of that changes when the
//! datastore shards.
//!
//! This asserts the OUTCOME — the status and the body a client is documented
//! to receive — for the four golden-path steps whose keys stopped living on
//! one server. Each arm names the hazard it closes, and every one of them is a
//! failure that appears only on a cluster and only at run time:
//!
//! - an append whose at-most-once marker hashes away from its stream is
//!   `CROSSSLOT`, and the documented 200 becomes a 500;
//! - a poll reads a readiness partition on one node and the fleet's stream on
//!   another, so a driver that could not follow the second returns no work
//!   from a fleet that has some;
//! - a live frame is `SPUBLISH`ed, which is SHARD-scoped, so a subscriber
//!   attached to the wrong node hears silence and the dashboard goes quiet
//!   with nothing in any log;
//! - a settlement commits in PostgreSQL and acknowledges in the queue, and
//!   those are now two different machines.
//!
//! # Why the outcomes are spelled out rather than compared to a fixture
//!
//! These are the answers the published API documents. A test that asserted
//! "the same thing it did last time" would ratify a regression the moment one
//! landed, and the point of a cutover gate is the opposite of that.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_datastore::SubscriptionHub;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{Scenario, redis_config, scenario};
use crate::reads::{balance, counter_column, lease_column, ledger_rows};
use crate::tail::{next_frame, settle};
use crate::wire::{capable_beat, field, json, poll_for_seeded_lease, post, report_body};

/// The event type the daemon has a gate for, so an extra append is leasable
/// rather than ended as unsupported.
const SUPPORTED_EVENT: &str = "fleet_steer";

/// The text a forwarded chunk carries.
const CHUNK_TEXT: &str = "the cluster carried this across a shard";

/// The activity channel a fleet's live tail is published on, either side of
/// the fleet id. Spelled here because the hub takes the whole channel name and
/// a test that guessed it would agree with nothing.
const CHANNEL_PREFIX: &str = "fleet:";
const CHANNEL_SUFFIX: &str = ":activity";

/// Rows one settled event writes: one receive, one stage.
const LEDGER_ROWS_PER_EVENT: i64 = 2;

/// A runner that has proven its capabilities is leasable; one that has not
/// reads degraded and is correctly answered no-work, which would make every
/// arm below pass for the wrong reason.
async fn prove_capabilities(http: &reqwest::Client, run: &Scenario) {
    let beat = post(http, run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "a heartbeat is accepted on the documented path"
    );
    assert_eq!(
        field(&json(beat).await, "degraded"),
        &json!(false),
        "and the proven capabilities clear the degraded verdict"
    );
}

/// A second append onto a stream that already holds one.
///
/// # The hazard
///
/// The append is a script over TWO keys: the fleet's stream and the
/// at-most-once marker that stops one logical event becoming two entries. A
/// cluster refuses a multi-key script whose keys hash to different slots, with
/// `CROSSSLOT`, and no amount of retrying fixes it. The marker therefore
/// carries the stream key as its hash tag. If that ever comes apart, the
/// documented outcome of accepting work becomes a 500 — and it does so on the
/// SECOND event, not the first, because the first is what creates the stream.
async fn a_second_admission_lands_on_the_same_slot(run: &Scenario) -> String {
    let second = run.enqueue_event(SUPPORTED_EVENT).await;
    assert_ne!(
        second, run.event_id,
        "the append minted a new logical event rather than returning the first"
    );
    second
}

/// The poll returns the fleet's oldest event, reading two nodes to do it.
///
/// # The hazard
///
/// Readiness is `fleet:ready:{p}` and the work is `fleet:{id}:events`. Those
/// are different keys, chosen to hash to different slots on purpose, so the
/// partition a poll samples is spread across the cluster. A poll that could
/// not follow the second read would answer `lease: null` — a documented,
/// perfectly valid response — from a fleet that is holding work. That is the
/// failure this arm exists for: it does not look like an error anywhere.
async fn a_poll_crosses_the_partition_and_the_stream(
    http: &reqwest::Client,
    run: &Scenario,
) -> (String, u64) {
    // WHICH fleet answers is a rotation's question, not one request's: the
    // index is sixteen partitions and a poll reads one. `poll_for_seeded_lease`
    // turns the rotation and stops on this scenario's own event, which is the
    // OLDEST entry on this fleet's stream — so a lease coming back at all is
    // the stream read having followed the partition read to a different node
    // rather than answering from whatever was local.
    poll_for_seeded_lease(http, run).await
}

/// A frame the daemon publishes reaches a subscriber that attached elsewhere.
///
/// # The hazard
///
/// `SPUBLISH` delivers to the shard that owns the channel's slot, and
/// `SSUBSCRIBE` only hears the shard it is attached to. A publisher and a
/// subscriber that disagree about which node owns the channel produce exactly
/// no symptom: the publish succeeds, the subscribe succeeds, and the dashboard
/// is silent. Asserted through `SubscriptionHub` because that is the consumer
/// the live tail actually runs on — a raw subscribe would agree with a
/// publisher that had drifted from its only reader.
async fn a_frame_crosses_the_shard_to_its_subscriber(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
) {
    // Subscribed BEFORE the forward: pub/sub keeps nothing for a reader that
    // arrives late, so a subscription opened afterwards would prove a drop
    // that never happened.
    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the dashboard's hub connects to the lane's cluster");
    let mut tail = hub.subscribe(&format!("{CHANNEL_PREFIX}{}{CHANNEL_SUFFIX}", run.fleet));
    // The pump registers the subscription asynchronously; publishing in the
    // same instant can legitimately beat it to the server.
    settle().await;

    let forwarded = post(
        http,
        run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"fleet_response_chunk": {"text": CHUNK_TEXT}}]}),
    )
    .await;
    assert_eq!(
        forwarded.status().as_u16(),
        202,
        "the reply acknowledges RECEIPT, as documented — the publish behind it \
         is best-effort and happens whether or not anybody is listening"
    );

    let frame = next_frame(&mut tail).await.expect(
        "the frame reaches the subscriber — a shard-scoped publish that missed it \
         would leave this silent with nothing logged anywhere",
    );
    assert_eq!(
        field(&frame, "text"),
        &json!(CHUNK_TEXT),
        "and it carries the payload that was posted, not an empty envelope"
    );
    assert_eq!(
        field(&frame, "event_id"),
        &json!(run.event_id),
        "stamped with the event the lease is executing, which is how the \
         dashboard groups a fleet's tail"
    );
}

/// The terminal report settles once, and a repeat of it costs nothing.
///
/// # The hazard
///
/// The result, its settlement and the terminal lease state commit in one
/// PostgreSQL transaction, and the queue is acknowledged only after that
/// commit. Those are two machines now. A report that acknowledged first and
/// committed second would lose the result on a crash between them; one that
/// charged per delivery would debit a retried report twice. Both are invisible
/// in the success path, so the repeat is asserted rather than assumed.
async fn a_report_settles_once_across_both_stores(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
    fence: u64,
) {
    let before = balance(run).await;
    let report = report_body(lease_id, &run.event_id, fence);

    let settled = post(http, run, "/v1/runners/me/reports", &report).await;
    assert_eq!(settled.status().as_u16(), 200, "the report is accepted");
    assert_eq!(
        json(settled).await,
        json!({"ok": true}),
        "and answers in the shape a runner parses"
    );
    assert_eq!(
        lease_column(run, lease_id, "status").await.as_deref(),
        Some("reported"),
        "the lease is terminal, which is what stops the reclaim sweep re-issuing it"
    );

    let after = balance(run).await;
    assert!(
        after < before,
        "a priced run draws the wallet down: {before:?} → {after:?}"
    );
    assert_eq!(
        ledger_rows(run).await,
        LEDGER_ROWS_PER_EVENT,
        "one receive row and one stage row, and no third from a redelivery"
    );

    let repeated = post(http, run, "/v1/runners/me/reports", &report).await;
    assert_eq!(
        repeated.status().as_u16(),
        200,
        "a repeat is acknowledged rather than refused — a runner replays because \
         it did not hear the first answer, and refusing teaches it to retry forever"
    );
    assert_eq!(
        balance(run).await,
        after,
        "and it charges nothing: the settlement keys on the ledger row, so a \
         replayed receipt cannot debit twice"
    );
    assert_eq!(
        ledger_rows(run).await,
        LEDGER_ROWS_PER_EVENT,
        "with no ledger row behind it either"
    );
    assert_eq!(
        counter_column(run, "succeeded").await.as_deref(),
        Some("1"),
        "and the lifetime tally counts one completed run, not two"
    );
}

/// The next poll finds the fleet's remaining work rather than stalling on it.
///
/// # The hazard
///
/// Clearing a fleet's readiness mark is a token-checked single-key script, and
/// the fleet still holds the second event this suite appended. A clear that
/// ran on the wrong slot, or one that deleted unconditionally, leaves the
/// documented outcome intact on THIS request and loses the next event
/// entirely — which is why the remaining work is claimed rather than counted.
async fn the_fleet_is_still_discoverable_for_its_remaining_work(
    http: &reqwest::Client,
    run: &Scenario,
    second_event: &str,
) {
    let polled = post(http, run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(polled.status().as_u16(), 200, "the poll answers");
    let body = json(polled).await;
    let lease = body
        .get("lease")
        .filter(|value| !value.is_null())
        .expect("the fleet's second event is still owed, so it is still discoverable");
    assert_eq!(
        field(field(lease, "event"), "event_id"),
        &json!(second_event),
        "and the entry handed over is the one that had not been reported"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and the cluster: make test-integration-rustd"]
async fn test_cluster_preserves_application_outcomes() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    prove_capabilities(&http, &run).await;
    let second_event = a_second_admission_lands_on_the_same_slot(&run).await;
    let (lease_id, fence) = a_poll_crosses_the_partition_and_the_stream(&http, &run).await;
    a_frame_crosses_the_shard_to_its_subscriber(&http, &run, &lease_id).await;
    a_report_settles_once_across_both_stores(&http, &run, &lease_id, fence).await;
    the_fleet_is_still_discoverable_for_its_remaining_work(&http, &run, &second_event).await;

    supervisor.shutdown().await;
    run.cleanup().await;
}
