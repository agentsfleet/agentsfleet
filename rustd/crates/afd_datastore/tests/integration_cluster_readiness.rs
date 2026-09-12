//! Dimension 0.4 — readiness spread over partitions keeps every partition's
//! work discoverable inside a bounded poll while one partition is hot.
//!
//! The readiness index today is one hash, and a poll samples it with
//! `HRANDFIELD`. On a cluster that hash is one slot on one node, and under a
//! skewed population the sample is dominated by whichever fleets are many:
//! three cold fleets among four thousand hot ones are found by luck. This
//! measures the partitioned shape the target design names — `ready:{p}` for a
//! fixed `p` count, a poll rotating over partitions under a candidate budget —
//! against the single hash, with the same poll budget, and asserts the
//! property that matters: every cold fleet is discovered within one rotation,
//! and no single poll reads more than its budget.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use redis::cluster_async::ClusterConnection;

use crate::cluster::ClusterHarness;

/// The partition count under measurement. Sixteen is a starting point, not a
/// decision: the production count is chosen from the section that adopts this
/// and recorded in the architecture page.
const PARTITIONS: u16 = 16;

/// The partition every hot fleet lands in.
const HOT_PARTITION: u16 = 3;

/// How many fleets make a partition hot, and how many a cold one holds.
const HOT_FLEETS: u32 = 4_000;
const COLD_FLEETS: u32 = 3;

/// How many candidates one poll may take: `HRANDFIELD`'s count argument.
const CANDIDATE_BUDGET: usize = 8;

const CMD_HSET: &str = "HSET";
const CMD_HRANDFIELD: &str = "HRANDFIELD";
const CMD_DEL: &str = "DEL";
const ARG_WITHVALUES: &str = "WITHVALUES";

/// The token every mark carries; its value is not under test here.
const TOKEN: &str = "t";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live Dragonfly cluster: make test-integration-rustd"]
async fn test_partitioned_readiness_bounds_hotspots() {
    let harness = ClusterHarness::from_lane();
    let mut connection = harness.connect().await;

    // The partitioned index: one hash per partition, the partition number as
    // the hash tag so `p` alone decides the slot.
    let partition_keys: Vec<String> = (0..PARTITIONS)
        .map(|p| format!("{}:ready:{{{p}}}", harness.name("partitioned")))
        .collect();
    // The single hash the daemon uses today, populated identically.
    let single_key = harness.name("single");

    let mut cold_fleets = BTreeSet::new();
    for (p, key) in (0_u16..).zip(&partition_keys) {
        let count = if p == HOT_PARTITION {
            HOT_FLEETS
        } else {
            COLD_FLEETS
        };
        let fleets: Vec<String> = (0..count).map(|n| format!("fleet-{p}-{n}")).collect();
        if p != HOT_PARTITION {
            cold_fleets.extend(fleets.iter().cloned());
        }
        mark_all(&mut connection, key, &fleets).await;
        mark_all(&mut connection, &single_key, &fleets).await;
    }

    // One rotation of the partitioned poll: PARTITIONS polls, one per
    // partition, each bounded to CANDIDATE_BUDGET candidates.
    let mut discovered_partitions = BTreeSet::new();
    let mut partitioned_cold_seen = BTreeSet::new();
    let mut widest_poll = 0;
    for (p, key) in (0_u16..).zip(&partition_keys) {
        let candidates = peek(&mut connection, key).await;
        widest_poll = widest_poll.max(candidates.len());
        if !candidates.is_empty() {
            discovered_partitions.insert(p);
        }
        for fleet in candidates {
            if cold_fleets.contains(&fleet) {
                partitioned_cold_seen.insert(fleet);
            }
        }
    }

    // The same poll budget against the single hash.
    let mut single_cold_seen = BTreeSet::new();
    for _poll in 0..PARTITIONS {
        for fleet in peek(&mut connection, &single_key).await {
            if cold_fleets.contains(&fleet) {
                single_cold_seen.insert(fleet);
            }
        }
    }

    for key in partition_keys.iter().chain(std::iter::once(&single_key)) {
        let mut del = redis::cmd(CMD_DEL);
        del.arg(key);
        let _removed: i64 = del.query_async(&mut connection).await.expect("cleanup");
    }

    println!(
        "evidence: partitioned poll found {} of {} cold fleets in one rotation, widest poll {} candidates; single hash found {} with the same budget",
        partitioned_cold_seen.len(),
        cold_fleets.len(),
        widest_poll,
        single_cold_seen.len()
    );
    assert_eq!(
        discovered_partitions.len(),
        usize::from(PARTITIONS),
        "every partition is visited inside one rotation"
    );
    assert!(
        widest_poll <= CANDIDATE_BUDGET,
        "a poll never reads past its candidate budget: {widest_poll}"
    );
    assert_eq!(
        partitioned_cold_seen, cold_fleets,
        "every cold fleet is discovered inside one rotation, hot partition or not"
    );
    assert!(
        single_cold_seen.len() < cold_fleets.len(),
        "the single hash under the same budget misses cold fleets, which is the hotspot: saw {}",
        single_cold_seen.len()
    );
}

/// Marks every fleet ready in `key`, one command.
async fn mark_all(connection: &mut ClusterConnection, key: &str, fleets: &[String]) {
    let mut hset = redis::cmd(CMD_HSET);
    hset.arg(key);
    for fleet in fleets {
        hset.arg(fleet).arg(TOKEN);
    }
    let _added: i64 = hset.query_async(connection).await.expect("mark");
}

/// One bounded poll of `key`: up to `CANDIDATE_BUDGET` ready fleets.
///
/// Decoded as PAIRS rather than a flat list: RESP2 answers `WITHVALUES` flat
/// and RESP3 answers it nested, and the driver's tuple decoding accepts both
/// where a flat `Vec<String>` accepts only the first. `ReadyIndex::peek`
/// decodes flat today, which is one of the things this prototype learned.
async fn peek(connection: &mut ClusterConnection, key: &str) -> Vec<String> {
    let mut cmd = redis::cmd(CMD_HRANDFIELD);
    cmd.arg(key).arg(CANDIDATE_BUDGET).arg(ARG_WITHVALUES);
    let pairs: Vec<(String, String)> = cmd.query_async(connection).await.expect("peek");
    pairs.into_iter().map(|(fleet, _token)| fleet).collect()
}
