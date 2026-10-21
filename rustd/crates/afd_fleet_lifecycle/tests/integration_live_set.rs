//! Concurrent misses on one workspace's live set share one load, and its
//! failure.
//!
//! A load the cache coalesced hands every waiter a clone of ONE `Arc`; loads
//! that ran apart each answer their own. So "all twenty answers are the same
//! allocation" is the observable form of "the statement ran once", read
//! without counting statements on a database other suites share.
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::Fleets;
use tokio::task::JoinSet;

use crate::support::Lane;

/// Viewers missing at once, as a busy wall's tabs do when an entry expires.
const VIEWERS: usize = 20;

/// Every viewer reads `workspace` at once; their answers in arrival order.
async fn read_together(
    fleets: &Fleets,
    workspace: &Uuid7,
) -> Vec<afd_fleet_lifecycle::Result<Arc<BTreeSet<String>>>> {
    let mut viewers = JoinSet::new();
    for _ in 0..VIEWERS {
        let (fleets, workspace) = (fleets.clone(), workspace.clone());
        viewers.spawn(async move { fleets.live_set(&workspace).await });
    }
    viewers.join_all().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres + Dragonfly"]
async fn test_concurrent_misses_read_once() {
    let lane = Lane::create().await;
    let seeded = [lane.fleet().await, lane.fleet().await];
    lane.fleets.invalidate_live_set(&lane.workspace).await;
    let answers = read_together(&lane.fleets, &lane.workspace).await;
    let answers: Vec<_> = answers
        .into_iter()
        .map(|answer| answer.expect("the workspace's set is readable"))
        .collect();
    let first = answers.first().expect("every viewer answered");
    assert!(
        answers.iter().all(|set| Arc::ptr_eq(set, first)),
        "every concurrent miss received the one load's answer"
    );
    let expected: BTreeSet<String> = seeded.iter().map(ToString::to_string).collect();
    assert_eq!(**first, expected);
    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_a_shared_load_that_fails_reaches_every_waiter_as_an_outage() {
    let fleets = Lane::with_dead_database();
    let workspace = crate::support::mint();
    for answer in read_together(&fleets, &workspace).await {
        let error = answer.expect_err("nobody answers on the dead database");
        assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
        assert!(
            error.is_datastore_unavailable(),
            "an outage stays an outage"
        );
    }
    let again = fleets.live_set(&workspace).await;
    assert!(again.is_err(), "a failed load is not cached as an answer");
}
