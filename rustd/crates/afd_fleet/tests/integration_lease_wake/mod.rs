//! The readiness doorbell as a lease poll uses it: fresh work is leased on
//! the next poll whichever partition the cursor names, an idle poll costs one
//! Dragonfly command and no Postgres, and neither a lost ring nor a busy
//! partition costs more than the rotation.
//!
//! Each test polls a PRIVATE index whose channels ring its own doorbell, so
//! its partitions and rings hold only what it wrote, and a fresh store, so
//! its cursor starts at partition 0.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture as Trace;
use afd_observability::metrics::label::fleet::ReadyWake;
use afd_observability::test_util::Capture;

use crate::seed::select_within_one_rotation;

mod behind;
mod broken;
mod order;
mod scene;
mod tally;

use scene::{partition, scene};

/// The counter a failed ring or rung peek moves, and its label.
const WAKE_FAILURES: &str = "agentsfleet_fleet_ready_wake_failures_total";
const KIND: &str = "kind";
/// What a test puts where a hash should be.
const NOT_A_HASH: &str = "not-a-hash";
const CMD_SET: &str = "SET";
const CMD_DEL: &str = "DEL";

/// Held by every test that makes a rung peek fail: the failure counter is the
/// process's, so one test's failure would land inside another's window.
static RUNG_PEEK_FAILURES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A fleet marked away from the cursor's partition is leased on the very next
/// poll, which costs the rotation peek, the rung peek beside it, and the two
/// stream reads that take its event; its partition is left clear, because its
/// only fleet is the one now running.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_marked_fleet_is_leased_on_the_next_poll() {
    let scene = scene("next-poll").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;

    let (leased, cost) = scene.poll(&fleet.runners[0]).await;

    let leased = leased.expect("the ring sends the first poll to the fleet's partition");
    assert_eq!(leased.fleet_id.as_str(), fleet.id);
    assert_eq!(cost.dragonfly_commands, 4, "{cost:?}");
    assert_eq!(cost.database_roundtrips, 2, "scan and claim: {cost:?}");
    assert!(
        !scene.rung(fleet.partition()),
        "the only fleet there is running"
    );
    scene.fixtures.cleanup().await;
}

/// An idle poll asks Dragonfly once and Postgres never; a ring over a
/// partition that has since emptied costs one more peek, still no Postgres,
/// and is not rung again.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_idle_poll_cost_is_bounded() {
    let scene = scene("idle").await;
    let fleet = scene.fleet(&[partition(0), partition(1)]).await;

    let (none, idle) = scene.poll(&fleet.runners[0]).await;
    assert!(none.is_none());
    assert_eq!(
        (idle.dragonfly_commands, idle.database_roundtrips),
        (1, 0),
        "{idle:?}"
    );

    scene.mark(&fleet).await;
    scene.index.force_clear(&fleet.id).await.expect("clear");
    let (none, stale) = scene.poll(&fleet.runners[0]).await;
    assert!(none.is_none());
    assert_eq!(
        (stale.dragonfly_commands, stale.database_roundtrips),
        (2, 0),
        "{stale:?}"
    );
    assert!(!scene.rung(fleet.partition()), "the stale ring is taken");
    scene.fixtures.cleanup().await;
}

/// A claim that finds its fleet drained counts every command it sent: the
/// takeover and the new read that both came back empty, and the clear of the
/// mark that sent it there, beside both peeks.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_drained_claim_counts_its_stream_reads_and_clear() {
    let scene = scene("drained").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    afd_dragonfly::FleetStreams::new(scene.fixtures.queue().clone())
        .ensure_group(&fleet.id)
        .await
        .expect("the group exists before a read");
    scene.mark(&fleet).await;

    let (none, cost) = scene.poll(&fleet.runners[0]).await;

    assert!(none.is_none(), "the fleet holds nothing to lease");
    assert_eq!(
        cost.dragonfly_commands, 5,
        "rotation, rung peek, takeover, new read, clear: {cost:?}"
    );
    assert!(
        scene
            .index
            .token_for(&fleet.id)
            .await
            .expect("readable")
            .is_none(),
        "the drained fleet's mark is cleared"
    );
    scene.fixtures.cleanup().await;
}

/// A mark whose ring is lost is still leased within one rotation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_lost_ring_falls_back_to_rotation() {
    let scene = scene("lost").await;
    let fleet = scene.fleet(&[]).await;
    scene.send_quietly(&fleet).await;
    assert!(!scene.rung(fleet.partition()), "nothing rang");

    let leased = select_within_one_rotation(&scene.leases, &fleet.runners[0]).await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id)
    );
    scene.fixtures.cleanup().await;
}

/// A running fleet's partition is left clear after one dry poll, so a steer
/// to another fleet is leased on the next one rather than waiting out the
/// rotation behind it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_busy_rung_partition_does_not_starve_rotation() {
    let scene = scene("busy").await;
    let running = scene
        .fleet(&[partition(0), partition(1), partition(2)])
        .await;
    let waiting = scene
        .fleet(&[
            partition(0),
            partition(1),
            partition(2),
            running.partition(),
        ])
        .await;
    scene.send(&running).await;
    let (leased, _) = scene.poll(&running.runners[0]).await;
    assert!(leased.is_some(), "the first fleet is running");

    scene.send(&running).await;
    let (dry, _) = scene.poll(&running.runners[1]).await;
    assert!(dry.is_none(), "its slot is held by a live runner");
    assert!(
        !scene.rung(running.partition()),
        "the dry poll left the busy partition clear"
    );

    scene.send(&waiting).await;
    let (next, _) = scene.poll(&waiting.runners[0]).await;
    assert_eq!(
        next.map(|acquired| acquired.fleet_id.to_string()),
        Some(waiting.id)
    );
    scene.fixtures.cleanup().await;
}

/// A partition whose only marked fleet is running is not rung again by a poll
/// that leased from its rotation: the scan never offered the running fleet,
/// so nothing there waits untried, and later polls are not sent back to it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_running_fleets_ring_is_not_rung_again() {
    let scene = scene("running").await;
    let running = scene.fleet(&[partition(0)]).await;
    let other = scene.fleet(&[partition(0), running.partition()]).await;
    scene.send(&running).await;
    let (leased, _) = scene.poll(&running.runners[0]).await;
    assert!(leased.is_some(), "the first fleet is running");
    scene.send_quietly(&other).await;
    for _idle in 1..other.partition().index() {
        let (none, _) = scene.poll(&other.runners[1]).await;
        assert!(
            none.is_none(),
            "nothing leasable before the other fleet's partition"
        );
    }

    scene.send(&running).await;
    let (leased, _) = scene.poll(&other.runners[0]).await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(other.id.clone()),
        "the poll leased from its rotation"
    );
    assert!(
        !scene.rung(running.partition()),
        "the running fleet's ring was not rung again"
    );
    scene.fixtures.cleanup().await;
}

/// A ring on the partition the cursor names is served by the one peek the
/// poll makes anyway: that peek and the two stream reads that take the event.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_ring_on_the_rotations_partition_is_peeked_once() {
    let scene = scene("same").await;
    let fleet = scene.fleet(&[]).await;
    for _idle in 0..fleet.partition().index() {
        let (none, _cost) = scene.poll(&fleet.runners[0]).await;
        assert!(none.is_none(), "nothing is marked yet");
    }
    scene.send(&fleet).await;

    let (leased, cost) = scene.poll(&fleet.runners[0]).await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id.clone())
    );
    assert_eq!(cost.dragonfly_commands, 3, "{cost:?}");
    assert!(!scene.rung(fleet.partition()), "the ring is taken");
    scene.fixtures.cleanup().await;
}

/// A rung peek that fails is counted, logged with its partition, and left
/// clear so the next poll is not sent back; the poll keeps its rotation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_rung_peek_that_fails_leaves_the_ring_clear() {
    let _counted_alone = RUNG_PEEK_FAILURES.lock().await;
    let capture = Capture::install();
    let scene = scene("unpeekable").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.mark(&fleet).await;
    let broken = scene.partition_key(fleet.partition());
    scene.raw(&broken, CMD_SET, Some(NOT_A_HASH)).await;
    let peeks = capture.sum(WAKE_FAILURES, &[(KIND, ReadyWake::Peek.as_str())]);

    let log = Trace::install();

    let (none, cost) = scene.poll(&fleet.runners[0]).await;

    assert!(none.is_none(), "the rotation's partition holds nothing");
    assert_eq!(cost.dragonfly_commands, 2, "rotation, rung peek: {cost:?}");
    let refused = log.only("assign_ready_wake_peek_failed");
    let named = fleet.partition().index().to_string();
    assert_eq!(refused.field("partition"), Some(named.as_str()));
    assert_eq!(
        capture.sum(WAKE_FAILURES, &[(KIND, ReadyWake::Peek.as_str())]),
        peeks + 1,
        "this poll's rung peek counted once"
    );
    assert!(
        !scene.rung(fleet.partition()),
        "the unreadable partition is left clear"
    );
    scene.raw(&broken, CMD_DEL, None).await;
    scene.fixtures.cleanup().await;
}

/// A held fleet's readiness read joins the poll's Dragonfly tally once,
/// however often the runner names it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_held_fleet_reads_join_the_dragonfly_tally() {
    let scene = scene("held").await;
    let fleet = scene.fleet(&[]).await;
    let held = Uuid7::parse(&fleet.id).expect("a fleet id");

    let (polled, cost) = scene
        .leases
        .select_measured(&fleet.runners[0], &[held.clone(), held])
        .await;

    assert!(polled.expect("the poll must not fault").is_none());
    assert_eq!(
        cost.dragonfly_commands, 2,
        "one held read, then the rotation: {cost:?}"
    );
    scene.fixtures.cleanup().await;
}
