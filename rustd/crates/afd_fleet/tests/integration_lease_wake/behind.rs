//! A rung partition a poll leaves work in behind its lease is rung again: a
//! fleet the scan offered and the poll never tried, or one the scan never
//! offered because the cursor's partition filled it to its ceiling.
//!
//! Every fleet here carries one shared tag and every runner carries it, so a
//! runner is eligible for all of them and the only thing deciding which fleet
//! a poll takes is the order the scan offers them in.

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::Partition;
use afd_fleet::lease::assign::MAX_READY_CANDIDATES_PER_POLL;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use super::scene::{Fleet, Scene, partition, scene};
use crate::requests::{ENROLLED_AT, enrolment_tagged, placement_tag};

/// The partition a fresh store's cursor names first.
const CURSOR: u16 = 0;
/// The partition the doorbell rings, away from the cursor's.
const RUNG: u16 = 5;
/// How far the id walk looks; sixteen partitions are reached long before it.
const SEARCH: u32 = 0xffff;

/// Ids walked by every fleet this file seeds, so no two collide.
static NEXT: AtomicU32 = AtomicU32::new(0);

/// A fresh id in the uuidv7 shape the schema checks, its last group walked.
fn fresh_id() -> String {
    let run = afd_core::clock::now().as_millis() % 0xfff;
    let pid = std::process::id();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("0195b4ba-8d3a-7{run:03x}-9bcd-{pid:08x}{n:04x}")
}

/// Two runners and the tag they and every fleet below share.
async fn shared_runners(scene: &Scene) -> (String, [Uuid7; 2]) {
    let tag = placement_tag(&fresh_id());
    let mut runners = Vec::with_capacity(2);
    for _ in 0..2 {
        let request = enrolment_tagged(
            SandboxTier::LandlockFull,
            NetworkPolicy::AllowListEgress,
            1,
            &tag,
        );
        let enrolled = scene
            .fixtures
            .runners()
            .register(&request, UnixMillis::from_millis(ENROLLED_AT))
            .await
            .expect("enrolment must succeed");
        runners.push(enrolled.runner_id);
    }
    let runners = runners.try_into().unwrap_or_else(|_| unreachable!("two"));
    (tag, runners)
}

/// A fleet seeded in `partition` under `tag`, served by `runners`.
async fn fleet_in(scene: &Scene, partition: Partition, tag: &str, runners: &[Uuid7; 2]) -> Fleet {
    let id = (0..SEARCH)
        .map(|_| fresh_id())
        .find(|id| Partition::of(id) == partition)
        .expect("sixteen partitions are all reachable");
    let (workspace, tenant) = (fresh_id(), fresh_id());
    scene
        .fixtures
        .seed_fleet(&id, &workspace, &tenant, tag, ENROLLED_AT)
        .await;
    Fleet {
        id,
        workspace,
        runners: runners.clone(),
    }
}

/// The fleet a poll leased.
fn leased(scene_poll: Option<afd_fleet::lease::Acquired>) -> String {
    scene_poll.expect("the poll leases").fleet_id.to_string()
}

/// The cursor's fleet is leased ahead of the rung one the scan also offered,
/// so the rung partition is rung again and the next poll leases its fleet.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_offered_fleet_left_untried_is_rung_again() {
    let scene = scene("untried").await;
    let (tag, runners) = shared_runners(&scene).await;
    let ahead = fleet_in(&scene, partition(CURSOR), &tag, &runners).await;
    let behind = fleet_in(&scene, partition(RUNG), &tag, &runners).await;
    scene.send_quietly(&ahead).await;
    scene.send(&behind).await;

    let (first, _) = scene.poll(&runners[0]).await;

    assert_eq!(leased(first), ahead.id, "the cursor's fleet leads the scan");
    assert!(
        scene.rung(partition(RUNG)),
        "the untried fleet's partition rings again"
    );
    let (second, _) = scene.poll(&runners[1]).await;
    assert_eq!(leased(second), behind.id, "the next poll takes the ring");
    scene.fixtures.cleanup().await;
}

/// The cursor's partition fills the scan to its ceiling, so the rung fleet is
/// never offered at all; the partition is rung again all the same, and the
/// next poll leases it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_scan_cut_at_its_ceiling_rings_again() {
    let scene = scene("cut").await;
    let (tag, runners) = shared_runners(&scene).await;
    for _ in 0..MAX_READY_CANDIDATES_PER_POLL {
        let filler = fleet_in(&scene, partition(CURSOR), &tag, &runners).await;
        scene.send_quietly(&filler).await;
    }
    let behind = fleet_in(&scene, partition(RUNG), &tag, &runners).await;
    scene.send(&behind).await;

    let (first, _) = scene.poll(&runners[0]).await;

    assert_ne!(
        leased(first),
        behind.id,
        "the cursor's fleets fill the scan"
    );
    assert!(scene.rung(partition(RUNG)), "the cut partition rings again");
    let (second, _) = scene.poll(&runners[1]).await;
    assert_eq!(leased(second), behind.id, "the next poll takes the ring");
    scene.fixtures.cleanup().await;
}
