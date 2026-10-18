//! The purges that must not happen, and the one that must survive a dead queue.
//!
//! Each refusal is settled under the row lock before the delete runs, so none
//! of them reaches Dragonfly: the lane here is Postgres with a queue nobody
//! answers on. Every refusal asserts the row is still there afterwards, because
//! an error answered AFTER a delete committed would read the same to a caller.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;

use crate::support::Lane;

/// The resource version a seeded fleet starts at, which a kill moves past.
const SEEDED_VERSION: i64 = 1;

/// A tag the row has moved past is refused with the tag it holds now, and the
/// fleet survives.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_stale_tag_purges_nothing_and_answers_the_current_tag() {
    let lane = Lane::without_queue().await;
    let fleet = lane.fleet().await;
    let stale = afd_core::etag::versioned(&fleet, SEEDED_VERSION);
    let current = lane.kill(&fleet).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &fleet, &stale)
        .await
        .expect_err("a tag from before the kill must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_SOURCE_STALE);
    assert_eq!(refused.stale_tag(), Some(current.as_str()));
    assert!(
        lane.fleet_column(&fleet, "id").await.is_some(),
        "the fleet survives"
    );
    lane.cleanup().await;
}

/// A fleet that was never killed is refused even with its current tag.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_live_fleet_is_refused_until_it_is_killed() {
    let lane = Lane::without_queue().await;
    let fleet = lane.fleet().await;
    let tag = lane.tag(&fleet).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect_err("an active fleet must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_ALREADY_TERMINAL);
    assert_eq!(refused.stale_tag(), None);
    assert_eq!(
        lane.fleet_column(&fleet, "status").await.as_deref(),
        Some("active"),
        "the refused fleet is untouched"
    );
    lane.cleanup().await;
}

/// Another workspace's fleet answers as absent, never as its status.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn another_workspaces_fleet_is_not_found() {
    let lane = Lane::without_queue().await;
    let neighbour = lane.another_workspace().await;
    let theirs = lane.fleet_at(&neighbour, Lane::now()).await;
    let tag = lane.kill(&theirs).await;

    let refused = lane
        .fleets
        .purge_tagged(&lane.workspace, &theirs, &tag)
        .await
        .expect_err("a fleet outside the caller's workspace must not purge");

    assert_eq!(refused.code(), error_code::AGENTSFLEET_NOT_FOUND);
    assert!(
        lane.fleet_column(&theirs, "id").await.is_some(),
        "their fleet survives"
    );
    lane.cleanup().await;
}

/// The queue's cleanup runs after the commit and cannot undo it: with no
/// Dragonfly answering, the purge still succeeds and the row is gone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_purge_commits_when_the_queue_cannot_forget() {
    let lane = Lane::without_queue().await;
    let fleet = lane.fleet().await;
    let tag = lane.kill(&fleet).await;

    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect("the stream cleanup is best-effort and after the commit");

    assert!(
        lane.fleet_column(&fleet, "id").await.is_none(),
        "the row is gone"
    );
    lane.cleanup().await;
}
