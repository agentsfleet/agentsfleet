//! The readiness index a lease poll reads before it opens a Postgres
//! connection.
//!
//! Split from `integration_streams.rs` per RULE FLL, along the seam that
//! matters: that file is the event stream, this one is the index. The client
//! both ride on is `integration_client.rs`.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::ready::{Partition, ReadyIndex};

use crate::support::DragonflyHarness;

/// The readiness index only clears a mark the caller actually saw.
///
/// The race this closes: a poll finds a fleet idle and moves to clear it while
/// ingress appends and re-marks. An unconditional delete erases a mark for
/// genuinely undelivered work, and nothing rediscovers it until a sweep.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_ready_index_clear_respects_the_token() {
    let harness = DragonflyHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("fleet");

    let observed = index.mark(&fleet).await.expect("mark");
    let partition = Partition::of(&fleet);
    assert!(
        index
            .peek(partition, 50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "a marked fleet must be visible to a poll"
    );

    // Ingress marks again — a new generation — while the poll still holds the
    // token it read. The index mints the generation, so the two differ.
    let newer = index.mark(&fleet).await.expect("re-mark");
    assert_ne!(newer, observed, "a re-mark must write a new generation");

    assert!(
        !index
            .clear_if_unchanged(&fleet, &observed)
            .await
            .expect("clear"),
        "a stale token must not clear a fleet that was re-marked"
    );
    assert!(
        index
            .peek(partition, 50)
            .await
            .expect("peek")
            .iter()
            .any(|ready| ready.fleet_id == fleet),
        "the newer mark must survive the stale clear"
    );

    // The current token does clear it.
    let current = index.mark(&fleet).await.expect("mark");
    assert!(
        index
            .clear_if_unchanged(&fleet, &current)
            .await
            .expect("clear"),
        "the token the caller observed must clear the fleet"
    );

    cleanup_fields(&harness, &fleet).await;
}

/// The index's read surface: a count, an emptiness question, and a sample.
///
/// `peek` is what a lease poll calls before it opens a Postgres connection, so
/// a pairing bug here — a field read as a token, or a truncated last pair —
/// sends every replica at the wrong fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_ready_index_read_surface() {
    let harness = DragonflyHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());

    let fleets: Vec<String> = (0..3).map(|n| harness.name(&format!("fleet{n}"))).collect();
    let mut minted = Vec::with_capacity(fleets.len());
    for fleet in &fleets {
        minted.push(index.mark(fleet).await.expect("mark"));
    }

    // Both aggregate accessors are EXERCISED, and neither is asserted against a
    // magnitude. The ready index is one key shared by the whole lane. This file
    // was once its own test binary, which cargo ran while no sibling suite was
    // writing; aggregating the crate's suites into one binary runs them
    // concurrently, and any sibling marking or clearing between two reads moves
    // both answers. An exact delta failed that way first, `>= marked` was the
    // repair, and `>= marked` failed the same way second — a sibling's cleanup
    // between the mark and the read left 2 where this test had written 3.
    //
    // A count over a shared key cannot be made stable by choosing a weaker
    // comparison, so it is not asserted at all. `runtime_suite.rs` states the
    // invariant this test kept breaking: no suite asserts over global state —
    // no `total()`, `COUNT(`, or unfiltered listing over a shared table.
    //
    // What the read surface actually promises is graded below, per fleet and in
    // each fleet's OWN partition, which no sibling can move: every fleet this
    // test marked is sampled, paired with ITS OWN token and not a neighbour's.
    // That is a strictly stronger statement than any count, and it is the
    // dimension this file names.
    let _counted = index.len().await.expect("len");
    let _empty = index.is_empty().await.expect("is_empty");

    // Every sampled pair must be a field with ITS value, not a shifted pairing.
    // Each fleet is looked for in ITS partition: the marks are spread by hash,
    // and a sample of one partition says nothing about a fleet in another.
    for (fleet, token) in fleets.iter().zip(&minted) {
        let sample = index.peek(Partition::of(fleet), 100).await.expect("peek");
        let found = sample
            .iter()
            .find(|ready| &ready.fleet_id == fleet)
            .expect("a marked fleet must be sampled from its own partition");
        assert_eq!(
            &found.token, token,
            "the sample paired a fleet with another fleet's token"
        );
    }

    for fleet in &fleets {
        cleanup_fields(&harness, fleet).await;
    }
}

/// An exact field read is deterministic even when the shared index is crowded.
///
/// Lease polls sample a partition, but tests that prove one fleet was awakened
/// need a precise lookup: they must read the fleet's OWN partition rather than
/// whichever one a sample happened to visit, or an unrelated fleet sharing the
/// index makes the assertion flaky.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_ready_index_token_for_reads_one_fleet_exactly() {
    let harness = DragonflyHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("fleet-token");

    assert_eq!(
        index.token_for(&fleet).await.expect("read missing token"),
        None
    );

    let minted = index.mark(&fleet).await.expect("mark");

    assert_eq!(
        index.token_for(&fleet).await.expect("read marked token"),
        Some(minted)
    );

    cleanup_fields(&harness, &fleet).await;
}

/// An exact read answers for the asked fleet and no other, ACROSS partitions.
///
/// The test above marks one fleet and reads it back. That proves `mark` and
/// `token_for` agree with each other, which they would even if both derived the
/// wrong key, and it cannot prove the "exactly" its name claims: exactness is a
/// statement about the OTHER fleets in the index, and a one-fleet index has
/// none.
///
/// So this seeds two fleets whose ids land in DIFFERENT partitions — asserted,
/// not assumed, because `Partition::of` is a checksum and a chosen pair could
/// silently collide and make the test vacuous. Each fleet must return its own
/// token and neither may return the other's. A `token_for` that read a fixed
/// key, or sampled a partition the way a poll does, fails here and passes
/// above.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_token_for_answers_for_one_fleet_across_partitions() {
    let harness = DragonflyHarness::connect().await;
    let index = ReadyIndex::new(harness.redis.clone());

    let (left, right) = (0..64)
        .map(|n| {
            (
                harness.name(&format!("part-a{n}")),
                harness.name(&format!("part-b{n}")),
            )
        })
        .find(|(a, b)| Partition::of(a) != Partition::of(b))
        .expect("two fleet ids landing in different partitions");
    assert_ne!(
        Partition::of(&left),
        Partition::of(&right),
        "the pair must straddle a partition boundary or this test proves nothing"
    );

    let left_token = index.mark(&left).await.expect("mark left");
    let right_token = index.mark(&right).await.expect("mark right");

    for (fleet, expected) in [(&left, left_token), (&right, right_token)] {
        assert_eq!(
            index.token_for(fleet).await.expect("read the marked token"),
            Some(expected),
            "{fleet} in partition {:?} must answer with its OWN token",
            Partition::of(fleet)
        );
    }

    let absent = harness.name("part-never-marked");
    assert_eq!(
        index
            .token_for(&absent)
            .await
            .expect("read an absent fleet"),
        None,
        "an unmarked fleet must answer None rather than a neighbour's token"
    );

    cleanup_fields(&harness, &left).await;
    cleanup_fields(&harness, &right).await;
}

/// Removes a fleet's field from the shared index, so one test's marks never
/// appear in another's sample.
async fn cleanup_fields(harness: &DragonflyHarness, fleet: &str) {
    let key = Partition::of(fleet).key();
    let mut cmd = redis::cmd("HDEL");
    cmd.arg(&key).arg(fleet);
    let _: Result<i64, _> = harness.redis.command("HDEL", &key, &cmd).await;
}
