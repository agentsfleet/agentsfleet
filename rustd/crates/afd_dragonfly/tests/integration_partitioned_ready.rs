//! The partitioned readiness index under the two things that move: ingress
//! racing a poll's clear, and a partition's slot moving between primaries.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::{BTreeMap, BTreeSet};

use afd_dragonfly::ready::{
    Partition, READY_PARTITIONS, ReadyCursor, ReadyIndex, ReadyPrefix, ReadyToken,
};

use crate::cluster::{CLUSTER_LANE, ClusterHarness};
use crate::support::DragonflyHarness;

/// How many fleets the race spreads over the partitions.
const FLEETS: usize = 96;

/// How many generations ingress marks each fleet through.
const GENERATIONS: u32 = 4;

/// How many candidates one poll may take.
const CANDIDATE_BUDGET: usize = 8;

/// How many rotations the poller makes while ingress is marking.
const ROTATIONS: u16 = 6;

/// Concurrent ingress and stale clears cannot lose newly ready work across
/// partitions, and a poll never exceeds its candidate budget.
///
/// Ingress re-marks every fleet through several generations while a poller
/// rotates over the partitions clearing what it saw. The invariant graded:
/// when both stop, a fleet is absent from the index ONLY if the poll that
/// cleared it saw the generation ingress wrote last. A fleet re-marked after
/// a poll's read is never removed by that poll's clear, in any partition.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_ready_races_preserve_work_and_bound_poll_cost() {
    let harness = DragonflyHarness::connect().await;
    // The poller clears every sampled field. Keep this race inside one key
    // family so it cannot erase a concurrent test's production mark.
    let index = ReadyIndex::under(
        harness.redis.clone(),
        ReadyPrefix::private(&harness.name("race")),
    );
    let fleets: Vec<String> = (0..FLEETS)
        .map(|n| harness.name(&format!("f{n}")))
        .collect();
    let partitions: BTreeSet<u16> = fleets
        .iter()
        .map(|fleet| Partition::of(fleet).index())
        .collect();
    assert!(
        partitions.len() > 1,
        "the fixture must span partitions for the race to be across them"
    );

    for fleet in &fleets {
        index.mark(fleet, "g0").await.expect("the first mark lands");
    }

    let ingress = tokio::spawn(remark_through_generations(index.clone(), fleets.clone()));
    let poller = tokio::spawn(poll_and_clear(index.clone()));
    ingress.await.expect("ingress does not panic");
    let (cleared, widest) = poller.await.expect("the poller does not panic");
    assert!(
        widest <= CANDIDATE_BUDGET,
        "a poll read {widest} candidates past its budget of {CANDIDATE_BUDGET}"
    );

    let lost = lost_work(&index, &fleets, &cleared).await;
    assert!(
        lost.is_empty(),
        "fleets re-marked after a poll's read were cleared by that poll: {lost:?}"
    );
}

/// The race poller may clear only marks in its own key family.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_private_poll_keeps_another_index_mark() {
    let harness = DragonflyHarness::connect().await;
    let other = ReadyIndex::under(
        harness.redis.clone(),
        ReadyPrefix::private(&harness.name("other-index")),
    );
    let private = ReadyIndex::under(
        harness.redis.clone(),
        ReadyPrefix::private(&harness.name("poll-isolation")),
    );
    let outside = harness.name("outside-mark");
    let inside = harness.name("private-mark");
    other
        .mark(&outside, "outside")
        .await
        .expect("mark other index");
    private.mark(&inside, "inside").await.expect("mark private");

    let (cleared, _) = poll_and_clear(private.clone()).await;
    let outside_token = other.token_for(&outside).await.expect("read other index");
    private.force_clear(&inside).await.expect("cleanup private");
    other
        .force_clear(&outside)
        .await
        .expect("cleanup other index");
    assert_eq!(cleared.get(&inside).map(String::as_str), Some("inside"));
    assert_eq!(
        outside_token.as_ref().map(ReadyToken::as_str),
        Some("outside"),
        "the test poller must not clear a mark in another index"
    );
}

/// Ingress: re-marks every fleet through [`GENERATIONS`] generations,
/// yielding between marks so the poller interleaves.
async fn remark_through_generations(index: ReadyIndex, fleets: Vec<String>) {
    for generation in 1..=GENERATIONS {
        for fleet in &fleets {
            index
                .mark(fleet, &format!("g{generation}"))
                .await
                .expect("a re-mark lands");
            tokio::task::yield_now().await;
        }
    }
}

/// The poller: [`ROTATIONS`] rotations of bounded peeks, clearing what each
/// saw. Answers which fleets it cleared on which token, and the widest poll.
async fn poll_and_clear(index: ReadyIndex) -> (BTreeMap<String, String>, usize) {
    let cursor = ReadyCursor::new();
    let mut cleared = BTreeMap::new();
    let mut widest = 0;
    for _ in 0..(ROTATIONS * READY_PARTITIONS) {
        let sample = index
            .peek(cursor.advance(), CANDIDATE_BUDGET)
            .await
            .expect("a peek answers");
        widest = widest.max(sample.len());
        for ready in sample {
            if index
                .clear_if_unchanged(&ready.fleet_id, &ready.token)
                .await
                .expect("a clear answers")
            {
                cleared.insert(ready.fleet_id, ready.token.as_str().to_owned());
            }
        }
    }
    (cleared, widest)
}

/// The fleets absent from the index whose clear did NOT see the final
/// generation — work lost to a stale clear — each with the token it was
/// cleared on. Clears every fleet as it goes.
async fn lost_work(
    index: &ReadyIndex,
    fleets: &[String],
    cleared: &BTreeMap<String, String>,
) -> Vec<(String, Option<String>)> {
    let final_generation = format!("g{GENERATIONS}");
    let mut lost = Vec::new();
    for fleet in fleets {
        let present = index
            .peek(Partition::of(fleet), FLEETS)
            .await
            .expect("a peek answers")
            .into_iter()
            .any(|ready| &ready.fleet_id == fleet);
        // Absent is fine only when the clear saw the final generation; a
        // fleet cleared on an older token and not re-marked is lost work.
        if !present && cleared.get(fleet) != Some(&final_generation) {
            lost.push((fleet.clone(), cleared.get(fleet).cloned()));
        }
        index.force_clear(fleet).await.expect("cleanup");
    }
    lost
}

/// Partition slot movement and worker loss preserve ordered recovery.
///
/// A mark written before its partition's slot moves is found after; a poll
/// that read it and died without clearing leaves it for the next poll; and
/// the token comparison still refuses a stale clear on the moved slot, so a
/// mark's generations stay ordered across the move.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_coordination_recovers_during_partition_movement() {
    let _lane = CLUSTER_LANE.lock().await;
    let cluster = ClusterHarness::from_lane();
    let harness = DragonflyHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());
    let mut raw = cluster.connect().await;

    let fleet = harness.name("moving");
    let partition = Partition::of(&fleet);
    let slot = ClusterHarness::keyslot(&mut raw, &partition.key()).await;
    let owner = ClusterHarness::canonical_primary(slot);
    let target = ClusterHarness::other_primary(owner);

    let first = mark_and_lose_a_poll(&index, &fleet).await;
    cluster.move_slot(slot, owner, target).await;

    let found = index
        .peek(partition, CANDIDATE_BUDGET)
        .await
        .expect("a peek answers on the moved slot")
        .into_iter()
        .find(|ready| ready.fleet_id == fleet)
        .expect("the mark a lost poll left behind is found after the move");
    assert_eq!(found.token, first, "the mark followed its slot unchanged");

    let second = index
        .mark(&fleet, "after-move")
        .await
        .expect("a re-mark lands on the moved slot");
    assert!(
        !index
            .clear_if_unchanged(&fleet, &first)
            .await
            .expect("a stale clear answers"),
        "a token from before the move must not clear the newer mark"
    );
    assert!(
        index
            .clear_if_unchanged(&fleet, &second)
            .await
            .expect("the current clear answers"),
        "the current token clears on the moved slot"
    );

    cluster.move_slot(slot, target, owner).await;
    index.force_clear(&fleet).await.expect("cleanup");
}

/// Marks `fleet`, then plays a poll that read the mark and died before it
/// could clear. Answers the token the mark minted.
async fn mark_and_lose_a_poll(index: &ReadyIndex, fleet: &str) -> ReadyToken {
    let token = index.mark(fleet, "before-move").await.expect("mark");
    let lost_poll = index
        .peek(Partition::of(fleet), CANDIDATE_BUDGET)
        .await
        .expect("peek")
        .into_iter()
        .find(|ready| ready.fleet_id == fleet)
        .expect("the mark is visible before the move");
    drop(lost_poll);
    token
}
