//! The readiness directory as a lease poll uses it: fresh work is leased on
//! the next poll whichever partition the cursor names, an idle poll costs two
//! Dragonfly commands and no Postgres, a mark racing a delete survives it,
//! and neither a lost entry nor a busy partition costs more than today's
//! rotation.
//!
//! Each test polls a PRIVATE index, so its directory and partitions hold only
//! what it wrote, and a fresh store, so its cursor starts at partition 0.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_observability::test_util::Capture;

use crate::requests::ENROLLED_AT;
use crate::seed::select_within_one_rotation;

#[path = "integration_lease_wake/scene.rs"]
mod scene;

use scene::{partition, scene};

/// The counter a failed or late directory command moves, and its label.
const HINT_FAILURES: &str = "agentsfleet_fleet_ready_hint_failures_total";
const KIND: &str = "kind";

/// A fleet marked away from the cursor's partition is leased on the very next
/// poll, which costs the directory read, the rotation peek, the hinted peek
/// and the delete of its now-spent entry.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_marked_fleet_is_leased_on_the_next_poll() {
    let scene = scene("next-poll").await;
    let fleet = scene.fleet(&[partition(0)]).await;
    scene.send(&fleet).await;

    let (leased, cost) = scene.poll(&fleet.runners[0]).await;

    let leased = leased.expect("the hint sends the first poll to the fleet's partition");
    assert_eq!(leased.fleet_id.as_str(), fleet.id);
    assert_eq!(cost.dragonfly_commands, 4, "{cost:?}");
    assert!(
        scene.dirty().await.is_empty(),
        "the only fleet there is running"
    );
    scene.fixtures.cleanup().await;
}

/// An idle poll asks Dragonfly twice and Postgres never; a stale entry over an
/// empty partition costs one peek and one delete, and still no Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_idle_poll_cost_is_bounded() {
    let scene = scene("idle").await;
    let fleet = scene.fleet(&[partition(0), partition(1)]).await;

    let (none, idle) = scene.poll(&fleet.runners[0]).await;
    assert!(none.is_none());
    assert_eq!(
        (idle.dragonfly_commands, idle.database_roundtrips),
        (2, 0),
        "{idle:?}"
    );

    scene.index.mark(&fleet.id).await.expect("mark");
    scene.index.force_clear(&fleet.id).await.expect("clear");
    let (none, stale) = scene.poll(&fleet.runners[0]).await;
    assert!(none.is_none());
    assert_eq!(
        (stale.dragonfly_commands, stale.database_roundtrips),
        (4, 0),
        "{stale:?}"
    );
    assert!(scene.dirty().await.is_empty(), "the stale entry is spent");
    scene.fixtures.cleanup().await;
}

/// A mark written between a poll's read and its delete survives the delete,
/// because it rewrote the token the delete compares.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_mark_racing_a_clear_survives() {
    let scene = scene("race").await;
    let fleet = scene.fleet(&[]).await;
    scene.index.mark(&fleet.id).await.expect("mark");
    let read = scene.index.dirty().await.expect("readable");
    let seen = read.first().expect("the mark named its partition");

    scene.index.mark(&fleet.id).await.expect("the racing mark");

    assert!(
        !scene.index.spend(seen).await.expect("the delete runs"),
        "the newer entry stays"
    );
    let now = scene.index.dirty().await.expect("readable");
    let kept = now.first().expect("still dirty");
    assert_eq!(kept.partition, fleet.partition());
    assert_ne!(kept.token, seen.token, "it carries the racing mark's token");
    assert!(scene.index.spend(kept).await.expect("the delete runs"));
    scene.fixtures.cleanup().await;
}

/// With its directory entry lost, a fleet is still leased within one rotation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lost_dirty_bit_falls_back_to_rotation() {
    let scene = scene("lost").await;
    let fleet = scene.fleet(&[]).await;
    scene.send(&fleet).await;
    scene.lose_directory().await;

    let leased = select_within_one_rotation(
        &scene.leases,
        &fleet.runners[0],
        UnixMillis::from_millis(ENROLLED_AT),
    )
    .await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id)
    );
    scene.fixtures.cleanup().await;
}

/// Two fleets marked in two partitions are leased by two workers, one poll
/// each, oldest mark first.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_marked_fleets_lease_within_one_interval() {
    let scene = scene("two").await;
    let first = scene.fleet(&[partition(0), partition(1)]).await;
    let second = scene
        .fleet(&[partition(0), partition(1), first.partition()])
        .await;
    scene.send(&first).await;
    scene.send(&second).await;
    assert_eq!(
        scene.dirty().await,
        [first.partition().index(), second.partition().index()],
        "oldest mark first"
    );

    let (one, _) = scene.poll(&first.runners[0]).await;
    let (two, _) = scene.poll(&second.runners[0]).await;

    assert_eq!(
        one.map(|acquired| acquired.fleet_id.to_string()),
        Some(first.id)
    );
    assert_eq!(
        two.map(|acquired| acquired.fleet_id.to_string()),
        Some(second.id)
    );
    scene.fixtures.cleanup().await;
}

/// A running fleet's partition gives up its entry after one dry poll, so a
/// steer to another fleet is leased on the next one rather than waiting out
/// the rotation behind it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_busy_dirty_partition_does_not_starve_rotation() {
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
        scene.dirty().await.is_empty(),
        "the dry poll spent the busy entry"
    );

    scene.send(&waiting).await;
    let (next, _) = scene.poll(&waiting.runners[0]).await;
    assert_eq!(
        next.map(|acquired| acquired.fleet_id.to_string()),
        Some(waiting.id)
    );
    scene.fixtures.cleanup().await;
}

/// A directory that cannot be read or written costs the hint and nothing else:
/// the mark lands, both failures are counted, and the rotation leases the
/// fleet.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_hint_deadline_falls_through() {
    let capture = Capture::install();
    let scene = scene("broken").await;
    let fleet = scene.fleet(&[]).await;
    scene.raw("SET", Some("not-a-hash")).await;
    let writes = capture.sum(HINT_FAILURES, &[(KIND, "write")]);
    let reads = capture.sum(HINT_FAILURES, &[(KIND, "read")]);

    scene.send(&fleet).await;
    assert!(
        scene
            .index
            .token_for(&fleet.id)
            .await
            .expect("readable")
            .is_some(),
        "the mark landed though its entry did not"
    );
    let leased = select_within_one_rotation(
        &scene.leases,
        &fleet.runners[0],
        UnixMillis::from_millis(ENROLLED_AT),
    )
    .await;

    assert_eq!(
        leased.map(|acquired| acquired.fleet_id.to_string()),
        Some(fleet.id)
    );
    assert!(
        capture.sum(HINT_FAILURES, &[(KIND, "write")]) > writes,
        "the write is counted"
    );
    assert!(
        capture.sum(HINT_FAILURES, &[(KIND, "read")]) > reads,
        "and every read"
    );
    scene.raw("DEL", None).await;
    scene.fixtures.cleanup().await;
}
