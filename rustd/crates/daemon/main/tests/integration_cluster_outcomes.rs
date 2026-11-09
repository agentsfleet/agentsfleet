//! The golden paths, and what they answer across Postgres and the NATS
//! cluster.
//!
//! # What this asserts that the sibling e2e suite does not
//!
//! `test_runner_suite_vs_rust_daemon` proves WIRING: that the verb is served
//! on the path a runner sends to, that the guard and the store scope by one
//! identity, that a payload round-trips.
//!
//! This asserts the OUTCOME — the status and the body a client is documented
//! to receive — for the golden-path steps that cross a process boundary. Each
//! arm names the hazard it closes, and each is a failure that shows up only at
//! run time and looks like nothing in a log:
//!
//! - a second admission must be a second event in the `fleet.admissions`
//!   ledger, not a replay of the first;
//! - an admission rings the wake ring (`af.wake.<shard>`) and the scheduler
//!   reads the ledger's candidates, so an offer that never arrives is a runner
//!   idling beside work;
//! - a live frame is published on whichever NATS node the daemon holds, so a
//!   subscriber attached to another node that missed it hears silence and the
//!   dashboard goes quiet;
//! - a report commits its result, its charge and the terminal lease state in
//!   one Postgres transaction, and a repeat of it must cost nothing.
//!
//! # Why the outcomes are spelled out rather than compared to a fixture
//!
//! These are the answers the published API documents. A test that asserted
//! "the same thing it did last time" would ratify a regression the moment one
//! landed.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::event::EventType;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{Scenario, scenario};
use crate::reads::{
    balance, counter_column, lease_column, ledger_rows, relay_settled, unsettled_lines,
};
use crate::tail::{next_frame, settle};
use crate::wire::{capable_beat, field, post, report_body, wait_for_offer, wait_for_seeded_offer};

/// The event type the daemon has an execution path for, so an extra append is
/// leasable rather than ended as unsupported. `EventType` is a closed set and
/// the ledger takes the type itself, so this cannot drift to a spelling the
/// pull path would end.
const SUPPORTED_EVENT: EventType = EventType::Chat;

/// The text a forwarded chunk carries.
const CHUNK_TEXT: &str = "the cluster carried this across its nodes";

/// A runner that has proven its capabilities is leasable; one that has not
/// reads degraded and is correctly answered no-work, which would make every
/// arm below pass for the wrong reason.
async fn prove_capabilities(run: &Scenario) {
    let beat = crate::wire::heartbeat(run, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );
    assert_eq!(
        field(&beat, "degraded"),
        &json!(false),
        "and the proven capabilities clear the degraded verdict"
    );
}

/// A second admission onto a fleet whose ledger already holds one.
///
/// # The hazard
///
/// Admission is at most once per key: a repeat of one operation returns the
/// event it already admitted. A key that collided across two distinct
/// operations would answer the second with the first's event, and the work
/// would silently never run. The seed's key is minted per call, so the second
/// admission must be a new event.
async fn a_second_admission_is_a_second_event(run: &Scenario) -> Uuid7 {
    let second = run.enqueue_event(SUPPORTED_EVENT).await;
    assert_ne!(
        second, run.event_id,
        "the append minted a new logical event rather than returning the first"
    );
    second
}

/// The runner is offered the fleet's oldest event.
///
/// # The hazard
///
/// Readiness is the Postgres admissions ledger: an admission commits a row in
/// `fleet.admissions`, then rings the NATS wake ring for the fleet's shard,
/// and the scheduler reads the ledger's candidates and pushes an offer down
/// the runner's stream. A scheduler that never heard the ring, or read a
/// ledger that did not hold the row, offers nothing, and a runner idling
/// beside work is not an error anywhere.
async fn the_runner_is_offered_the_fleets_oldest_event(run: &Scenario) -> (String, u64) {
    // Other fleets' heads may be offered first: candidates are ordered at
    // random among eligible fleets. `wait_for_seeded_offer` reads offers until
    // this scenario's own event arrives, which is the OLDEST unsettled
    // admission on this fleet.
    wait_for_seeded_offer(run).await
}

/// A frame the daemon publishes reaches a subscriber that attached elsewhere.
///
/// # The hazard
///
/// The daemon publishes on whichever NATS node its connection holds, and the
/// subscriber may be attached to another. The cluster carries the frame across
/// only once the subscriber's interest has reached the publisher's node. A
/// route that never carries it produces exactly no symptom: the publish
/// succeeds, the subscribe succeeds, and the dashboard is silent. Asserted
/// through the bus subscription because that is the consumer
/// the live tail actually runs on — a raw subscribe would agree with a
/// publisher that had drifted from its only reader.
async fn a_frame_crosses_the_cluster_to_its_subscriber(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
) {
    // Subscribed BEFORE the forward: pub/sub keeps nothing for a reader that
    // arrives late, so a subscription opened afterwards would prove a drop
    // that never happened.
    let mut tail = crate::tail::tail_of(&run.fleet).await;
    // The interest reaches the cluster asynchronously; publishing in the same
    // instant can legitimately beat it to the daemon's node.
    settle().await;

    let forwarded = post(
        http,
        run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({"frames": [{"fleet_response_chunk": {"text": CHUNK_TEXT, "text_kind": "answer"}}]}),
    )
    .await;
    assert_eq!(
        forwarded.status().as_u16(),
        202,
        "the reply acknowledges RECEIPT, as documented — the publish behind it \
         is best-effort and happens whether or not anybody is listening"
    );

    let frame = next_frame(&mut tail).await.expect(
        "the frame reaches the subscriber — a publish the cluster never routed \
         to it would leave this silent with nothing logged anywhere",
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
/// The result, its usage line and the terminal lease state commit in one
/// Postgres transaction, and the billing relay charges the line afterwards. A
/// report that charged per delivery would debit a retried report twice, which
/// is invisible in the success path, so the repeat is asserted rather than
/// assumed.
async fn a_report_settles_once(run: &Scenario, lease_id: &str, fence: u64) {
    let before = balance(run).await;
    let report = report_body(lease_id, &run.event_id, fence);

    let settled = crate::wire::report(run, &report).await;
    assert!(settled.is_ok(), "native report refused: {settled:?}");
    assert_eq!(
        settled.expect("the native report is acknowledged"),
        json!({"ok": true}),
        "and answers in the shape a runner parses"
    );
    assert_eq!(
        lease_column(run, lease_id, "status").await.as_deref(),
        Some("reported"),
        "the lease is terminal, which is what stops the reclaim sweep re-issuing it"
    );

    let logged = ledger_rows(run).await;
    assert!(
        logged >= 1,
        "the run's usage is logged in the report's own transaction, at least its final slice"
    );
    relay_settled(run).await;
    assert_eq!(
        unsettled_lines(run).await,
        0,
        "and the billing relay settles it shortly after the report answered"
    );
    let after = balance(run).await;
    assert!(
        after < before,
        "a priced run draws the wallet down: {before:?} → {after:?}"
    );

    assert_replayed_report(run, &report, logged, after).await;
}

/// The next poll finds the fleet's remaining work rather than stalling on it.
///
/// # The hazard
///
/// The fleet still holds the second event this suite admitted, and that
/// admission's wake ring fired before the first run settled. A settle that
/// closed more than its own admission leaves the documented outcome intact on
/// THIS report and loses the next event entirely, which is why the remaining
/// work is claimed rather than counted.
async fn the_fleet_is_still_discoverable_for_its_remaining_work(
    run: &Scenario,
    second_event: &Uuid7,
) {
    // Offers are read until the second event arrives: another fleet's head may
    // come first, and the helper carries the diagnosis if none in its budget
    // is this one.
    let (_lease_id, _fence) = wait_for_offer(run, second_event).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_cluster_preserves_application_outcomes() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();

    prove_capabilities(&run).await;
    let second_event = a_second_admission_is_a_second_event(&run).await;
    let (lease_id, fence) = the_runner_is_offered_the_fleets_oldest_event(&run).await;
    a_frame_crosses_the_cluster_to_its_subscriber(&http, &run, &lease_id).await;
    a_report_settles_once(&run, &lease_id, fence).await;
    the_fleet_is_still_discoverable_for_its_remaining_work(&run, &second_event).await;

    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn assert_replayed_report(
    run: &Scenario,
    report: &serde_json::Value,
    logged: i64,
    after: Option<i64>,
) {
    let repeated = crate::wire::report(run, report).await;
    assert!(repeated.is_ok(), "native report refused: {repeated:?}");
    assert_eq!(
        ledger_rows(run).await,
        logged,
        "the repeat logs no usage line"
    );
    // No new line, so nothing for the relay to charge: the balance is final.
    assert_eq!(
        balance(run).await,
        after,
        "and it charges nothing: the settlement keys on the ledger row, so a \
         replayed receipt cannot debit twice"
    );
    assert_eq!(
        counter_column(run, "succeeded_count").await.as_deref(),
        Some("1"),
        "and the lifetime tally counts one completed run, not two"
    );
}
